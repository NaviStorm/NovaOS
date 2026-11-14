//! Scénarios sur deux processus en EL0/CPL3, compilés uniquement dans l'archive de qualification.
use super::*;
use crate::hardware::block;

const TEMOIN: u8 = 0x5a;
const PORT_LOT: u64 = 0x10031;
const PORT_SG: u64 = 0x1003a;
const RESERVER: u64 = 0x1002e;
const LIVRER: u64 = 0x1002f;
const FINIR: u64 = 0x10032;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Faute {
    Aucune,
    Bail,
    Reservation,
    Adresse,
}

fn mots(acteur: Actor, adresse: u64, valeurs: &[u64]) {
    for (i, x) in valeurs.iter().enumerate() {
        acteur.put(adresse + i as u64 * 8, &x.to_le_bytes());
    }
}

fn verifier_page(acteur: Actor, adresse: u64, offset: usize, livree: bool) {
    for j in (0..4096).step_by(64) {
        let octets = acteur.bytes::<64>(adresse + j as u64);
        for (k, v) in octets.iter().enumerate() {
            assert_eq!(
                *v,
                if livree {
                    0x71 + ((offset + j + k) % 83) as u8
                } else {
                    TEMOIN
                }
            );
        }
    }
}

fn portails_historiques() {
    let (enfants, [pilote]) = group([ALL]);
    let cle = block::begin_session(1 | 8).unwrap();
    assert!(block::attach(pilote.0.number, cle));
    pilote.put(DESC, &block::wire::blank(block::wire::INFO));
    assert_eq!(pilote.call(block::PORTAL, cle, DESC, 128), Ok(128));
    let taille = block::wire::number(&pilote.bytes::<128>(DESC), 4) as usize;
    let lba = (8192 / taille) as u64;
    let trame = |phase| {
        let mut w = block::wire::blank(block::wire::READ);
        w[2] = phase;
        w[8..16].copy_from_slice(&lba.to_le_bytes());
        w
    };
    let mut debut = trame(block::wire::BEGIN);
    debut[40..48].copy_from_slice(&(8192u64 / 512).to_le_bytes());
    for (i, (objet, flags, suivant, n)) in [(1, 1, 1, 16), (2, 3, 2, taille), (3, 2, 0, 1)]
        .into_iter()
        .enumerate()
    {
        let d = 48 + 8 * i;
        debut[d..d + 3].copy_from_slice(&[objet, flags, suivant]);
        block::wire::put(&mut debut, d + 4, n as u32);
    }
    pilote.put(DESC, &debut);
    assert_eq!(pilote.call(block::PORTAL, cle, DESC, 128), Ok(128));
    pilote.put(DESC, &trame(block::wire::END));
    assert_eq!(pilote.call(block::PORTAL, cle, DESC, 128), Ok(128));
    for offset in (0..taille).step_by(block::wire::MAX) {
        let mut w = trame(block::wire::PIECE);
        let n = block::wire::MAX.min(taille - offset);
        block::wire::put(&mut w, 4, n as u32);
        block::wire::put(&mut w, 16, offset as u32);
        pilote.put(DESC, &w);
        assert_eq!(pilote.call(block::PORTAL, cle, DESC, 128), Ok(128));
        let reponse = pilote.bytes::<128>(DESC);
        for (k, x) in reponse[32..32 + n].iter().enumerate() {
            assert_eq!(*x, 0x71 + ((offset + k) % 83) as u8);
        }
    }
    let base = pilote.call(HEAP_RESIZE, 0, 0, 0).unwrap();
    assert_eq!(
        pilote.call(HEAP_RESIZE, base, base + 4096, 0),
        Ok(base + 4096)
    );
    let mut page = trame(block::wire::PAGE_PHASE);
    block::wire::put(&mut page, 4, 4096);
    pilote.put(DESC, &page);
    assert_eq!(
        pilote.call(block::wire::PAGE_PORTAL, cle, DESC, base),
        Ok(4096)
    );
    verifier_page(pilote, base, 0, true);
    block::retire().unwrap();
    stop(enfants, [pilote]);
    hal::log(format_args!("DMA lecture historique et PAGE integrales"));
}

pub(super) fn exercice() {
    portails_historiques();
    for multi in [false, true] {
        for faute in [
            Faute::Aucune,
            Faute::Bail,
            Faute::Reservation,
            Faute::Adresse,
        ] {
            let (enfants, [pilote, moteur]) = group([ALL, SEND | RECEIVE]);
            let cle = block::begin_session(1 | 8).unwrap();
            assert!(block::attach(pilote.0.number, cle));
            let (cap, pair) = (pilote.boot(), moteur.boot());
            with(|s| s.ipc.bind_lane(pilote.0, cap, moteur.0, pair)).unwrap();
            pilote.put(DESC, &block::wire::blank(block::wire::INFO));
            assert_eq!(pilote.call(block::PORTAL, cle, DESC, 128), Ok(128));
            let secteur = block::wire::number(&pilote.bytes::<128>(DESC), 4) as u64;
            let n = if multi { 2 } else { 1 };
            let lba = 8192 / secteur;
            let base = moteur.call(HEAP_RESIZE, 0, 0, 0).unwrap();
            assert_eq!(
                moteur.call(HEAP_RESIZE, base, base + 4096 * n, 0),
                Ok(base + 4096 * n)
            );
            let sortie = pilote.call(HEAP_RESIZE, 0, 0, 0).unwrap();
            assert_eq!(
                pilote.call(HEAP_RESIZE, sortie, sortie + 4096, 0),
                Ok(sortie + 4096)
            );
            for i in (0..4096).step_by(64) {
                pilote.put(sortie + i, &[TEMOIN; 64]);
                for j in 0..n {
                    moteur.put(base + 4096 * j + i, &[TEMOIN; 64]);
                }
            }
            mots(moteur, OUTPUT, &[lba, lba + 4096 / secteur]);
            mots(moteur, DESC, &[71, base, n, 0, OUTPUT]);
            assert_eq!(moteur.call(RESERVER, pair, DESC, 40), Ok(0));
            let adresse = if !multi && faute == Faute::Adresse {
                sortie + 16
            } else {
                sortie
            };
            // La destination décalée traverse la borne du tas à la fin de la première page.
            let mut w = block::wire::blank(block::wire::READ);
            w[2] = block::wire::PAGE_PHASE;
            block::wire::put(&mut w, 4, 4096);
            w[8..16].copy_from_slice(&lba.to_le_bytes());
            pilote.put(OUTPUT, &w);
            let (port, len) = if multi {
                mots(pilote, DESC, &[71, cap, cle, lba, 0, n, n, 0]);
                (PORT_SG, 64)
            } else {
                mots(pilote, DESC, &[71, cap, cle, OUTPUT, adresse, 0, n]);
                (PORT_LOT, 56)
            };
            let mode = match faute {
                Faute::Aucune => 0,
                Faute::Bail => 1,
                Faute::Reservation => 2,
                Faute::Adresse if multi => 3,
                Faute::Adresse => 0,
            };
            super::super::bulk::armer_livraison(mode, cap);
            let kicks = block::kicks();
            let res = pilote.call(port, DESC, len, 0);
            assert_eq!(block::kicks(), kicks + 1, "soumission {multi} {faute:?}");
            if faute == Faute::Aucune {
                assert_eq!(res, Ok(if multi { n } else { 0 }));
            } else {
                assert_eq!(
                    res,
                    Err(if multi {
                        REFUSED
                    } else if faute == Faute::Adresse {
                        BAD_ADDRESS
                    } else {
                        REVOKED
                    }),
                    "retour {multi} {faute:?}"
                );
            }
            let (index, effectues, vol, retrait) =
                super::super::bulk::constat_livraison(pilote.0, cap);
            let succes = faute == Faute::Aucune;
            assert_eq!(index, if multi && succes { n as usize } else { 0 });
            assert_eq!(effectues, if succes { n as usize } else { 0 });
            assert!(!vol);
            assert_eq!(retrait, !succes);
            if !succes {
                assert!(pilote.call(port, DESC, len, 0).is_err());
                assert_eq!(
                    block::kicks(),
                    kicks + 1,
                    "seconde soumission {multi} {faute:?}"
                );
            }
            if !multi {
                verifier_page(pilote, sortie, 0, succes);
                mots(pilote, DESC, &[71, sortie, n, 0]);
                let r = pilote.call(LIVRER, cap, DESC, 32);
                assert_eq!(r, if succes { Ok(4096) } else { Err(REVOKED) });
            }
            verifier_page(moteur, base, 0, succes);
            if multi && faute != Faute::Adresse {
                verifier_page(moteur, base + 4096, 4096, succes);
            }
            mots(pilote, DESC, &[71, sortie, n, 0]);
            assert_eq!(
                pilote.call(FINIR, cap, DESC, 32),
                if succes { Ok(0) } else { Err(REVOKED) }
            );
            block::retire().unwrap();
            stop(enfants, [pilote, moteur]);
            hal::log(format_args!(
                "DMA livraison multi={multi} faute={faute:?} avance={effectues} kicks=1"
            ));
        }
    }
}
