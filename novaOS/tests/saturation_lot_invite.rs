//! Sonde réservée à l'archive de qualification. Les deux acteurs emploient l'ABI du candidat.
use super::*;
use crate::hardware::block;
use core::sync::atomic::{AtomicU8, Ordering::SeqCst};

static RENDEZ_VOUS: AtomicU8 = AtomicU8::new(0);
const RESERVER: u64 = 0x1002e;
const TERMINER: u64 = 0x10032;
const SOUMETTRE_SG: u64 = 0x1003a;

// Inséré après le véritable kick du premier lot, interruptions rétablies. Le banc hôte
// suspend la lecture QEMU et atteste wait_break avant de permettre la concurrence.
pub(crate) fn apres_kick() {
    if RENDEZ_VOUS.compare_exchange(1, 2, SeqCst, SeqCst).is_ok() {
        until(|| RENDEZ_VOUS.load(SeqCst) == 3);
    }
}

fn signal(octet: u8) {
    until(|| crate::arch::serial::input() == Some(octet));
}

fn mots(acteur: Actor, adresse: u64, valeurs: &[u64]) {
    for (i, valeur) in valeurs.iter().enumerate() {
        acteur.put(adresse + 8 * i as u64, &valeur.to_le_bytes());
    }
}

fn verifier_deux_pages(acteur: Actor, adresse: u64, depart: Option<usize>) {
    for offset in (0..8192).step_by(64) {
        let octets = acteur.bytes::<64>(adresse + offset as u64);
        for (i, octet) in octets.iter().enumerate() {
            let attendu = depart.map_or(0x5a, |d| 0x71 + ((d + offset + i) % 83) as u8);
            assert_eq!(*octet, attendu, "offset={offset} i={i}");
        }
    }
}

pub(crate) fn exercice() {
    for retrait in [false, true] {
        // Même ordre que storage::volume : les tables privées x86 héritent des MMIO superviseur.
        let mut cle = block::begin_session(1 | 8).unwrap();
        let (enfants, [pilote, moteur]) = group([ALL, SEND | RECEIVE]);
        assert!(block::attach(pilote.0.number, cle));
        let (cap, mut pair) = (pilote.boot(), moteur.boot());
        with(|s| s.ipc.bind_lane(pilote.0, cap, moteur.0, pair)).unwrap();
        pilote.put(DESC, &block::wire::blank(block::wire::INFO));
        assert_eq!(pilote.call(block::PORTAL, cle, DESC, 128), Ok(128));
        let secteur = u64::from(block::wire::number(&pilote.bytes::<128>(DESC), 4));
        assert!(matches!(secteur, 512 | 4096));
        let base = moteur.call(HEAP_RESIZE, 0, 0, 0).unwrap();
        assert_eq!(
            moteur.call(HEAP_RESIZE, base, base + 16384, 0),
            Ok(base + 16384)
        );
        for offset in (0..16384).step_by(64) {
            moteur.put(base + offset, &[0x5a; 64]);
        }
        let lba1 = 8192 / secteur;
        let lba2 = 16384 / secteur;
        let liste = |lba| mots(moteur, OUTPUT, &[lba, lba + 4096 / secteur]);
        let descriptor = |tag, base| mots(moteur, DESC, &[tag, base, 2, 0, OUTPUT]);
        let demande = |tag, bail, lba| mots(pilote, DESC, &[tag, cap, bail, lba, 0, 2, 2, 0]);
        let fin = |tag| {
            mots(pilote, DESC, &[tag, 0, 2, 0]);
            pilote.call(TERMINER, cap, DESC, 32)
        };
        liste(lba1);
        descriptor(71, base);
        assert_eq!(moteur.call(RESERVER, pair, DESC, 40), Ok(0));
        liste(lba2);
        descriptor(72, base + 8192);
        demande(71, cle, lba1);
        hal::log(format_args!("SATURATION armer retrait={retrait}"));
        signal(b'A');
        let avant = block::kicks();
        RENDEZ_VOUS.store(1, SeqCst);
        pilote.start(SOUMETTRE_SG, DESC, 64, 0);
        until(|| RENDEZ_VOUS.load(SeqCst) == 2);
        assert_eq!(block::kicks(), avant + 1);
        hal::log(format_args!("SATURATION kick retrait={retrait}"));
        signal(b'B');
        assert_eq!(moteur.call(RESERVER, pair, DESC, 40), Err(DENIED));
        if retrait {
            assert_eq!(moteur.call(IPC_CLOSE, pair, 0, 0), Err(WOULD_BLOCK));
            assert_eq!(moteur.call(RESERVER, pair, DESC, 40), Err(DENIED));
        }
        assert_eq!(block::kicks(), avant + 1);
        verifier_deux_pages(moteur, base, None);
        verifier_deux_pages(moteur, base + 8192, None);
        hal::log(format_args!(
            "SATURATION nouveau tag refuse retrait={retrait}"
        ));
        signal(b'C');
        RENDEZ_VOUS.store(3, SeqCst);
        if retrait {
            assert_eq!(pilote.result(), Err(REFUSED));
            verifier_deux_pages(moteur, base, None);
            assert_eq!(fin(71), Err(REVOKED));
            assert_eq!(moteur.call(IPC_CLOSE, pair, 0, 0), Ok(0));
            assert_eq!(moteur.call(RESERVER, pair, DESC, 40), Err(DENIED));
            block::retire().unwrap();
            cle = block::begin_session(1 | 8).unwrap();
            assert!(block::attach(pilote.0.number, cle));
            pair = with(|s| {
                s.ipc
                    .derive(pilote.0, cap, moteur.0, (SEND | RECEIVE).into())
            })
            .unwrap();
            with(|s| s.ipc.bind_lane(pilote.0, cap, moteur.0, pair)).unwrap();
        } else {
            assert_eq!(pilote.result(), Ok(2));
            verifier_deux_pages(moteur, base, Some(0));
            // La collecte matérielle seule ne libère pas la réservation.
            assert_eq!(moteur.call(RESERVER, pair, DESC, 40), Err(DENIED));
            assert_eq!(fin(71), Ok(0));
        }
        // Le descripteur du moteur et ses deux adresses sont identiques à la tentative refusée.
        assert_eq!(moteur.call(RESERVER, pair, DESC, 40), Ok(0));
        demande(72, cle, lba2);
        assert_eq!(pilote.call(SOUMETTRE_SG, DESC, 64, 0), Ok(2));
        assert_eq!(fin(72), Ok(0));
        verifier_deux_pages(moteur, base, if retrait { None } else { Some(0) });
        verifier_deux_pages(moteur, base + 8192, Some(8192));
        assert_eq!(block::kicks(), avant + 2);
        block::retire().unwrap();
        stop(enfants, [pilote, moteur]);
        hal::log(format_args!(
            "SATURATION reprise exacte retrait={retrait} kicks=2 secteur={secteur}"
        ));
    }
    hal::log(format_args!("SATURATION deux lots qualifies"));
}
