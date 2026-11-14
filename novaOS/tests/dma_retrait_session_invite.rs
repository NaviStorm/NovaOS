//! Contrôle en invité de la restitution après une perte d'acquittement matériel.
//! Les deux processus gardent leurs handles pendant le drainage et sa reprise.
use super::*;

pub(super) fn exercice() {
    for (operation, multi, flush) in [
        ("mono", false, false),
        ("sg", true, false),
        ("flush", false, true),
    ] {
        let libres = block::cadres_temoin();
        let (enfants, [pilote, moteur]) = group([ALL, SEND | RECEIVE]);
        let cle = block::begin_session(if flush { 1 | 2 | 4 | 8 } else { 1 | 8 }).unwrap();
        assert!(block::attach(pilote.0.number, cle));
        let (cap, pair) = (pilote.boot(), moteur.boot());
        let table = with(|s| {
            let ipc = &mut s.ipc;
            ipc.bind_lane(pilote.0, cap, moteur.0, pair).unwrap();
            let envoi = ipc.create(moteur.0).unwrap();
            let reponse = ipc.create(moteur.0).unwrap();
            ipc.lier_attentes_lot(pilote.0, cap, moteur.0, envoi, reponse)
                .unwrap();
            let table = ipc.create(moteur.0).unwrap();
            let offsets = ipc
                .derive(moteur.0, table, pilote.0, RECEIVE.into())
                .unwrap();
            ipc.bind_lane(moteur.0, table, pilote.0, offsets).unwrap();
            ipc.lier_session_lots(cle, moteur.0, table).unwrap();
            table
        });
        pilote.put(DESC, &block::wire::blank(block::wire::INFO));
        assert_eq!(pilote.call(block::PORTAL, cle, DESC, 128), Ok(128));
        let secteur = block::wire::number(&pilote.bytes::<128>(DESC), 4) as u64;
        let n = if multi { 2 } else { 1 };
        let lba = 8192 / secteur;
        let base = moteur.call(HEAP_RESIZE, 0, 0, 0).unwrap();
        let sortie = pilote.call(HEAP_RESIZE, 0, 0, 0).unwrap();
        assert_eq!(
            moteur.call(HEAP_RESIZE, base, base + n * 4096, 0),
            Ok(base + n * 4096)
        );
        assert_eq!(
            pilote.call(HEAP_RESIZE, sortie, sortie + 4096, 0),
            Ok(sortie + 4096)
        );
        for offset in (0..n * 4096).step_by(64) {
            moteur.put(base + offset, &[TEMOIN; 64]);
        }
        for offset in (0..4096).step_by(64) {
            pilote.put(sortie + offset, &[TEMOIN; 64]);
        }
        mots(moteur, OUTPUT, &[lba, lba + 4096 / secteur]);
        let direction = if flush { 2 } else { 0 };
        mots(moteur, DESC, &[83, base, n, direction, OUTPUT]);
        assert_eq!(moteur.call(RESERVER, pair, DESC, 40), Ok(0));
        if flush {
            // Réécrire les octets déjà présents afin de vérifier ensuite l’image initiale.
            for offset in (0..4096).step_by(64) {
                let attendus: [u8; 64] = core::array::from_fn(|k| 0x71 + ((offset + k) % 83) as u8);
                moteur.put(base + offset as u64, &attendus);
            }
            mots(pilote, DESC, &[83, sortie, n, direction]);
            assert_eq!(pilote.call(LIVRER, cap, DESC, 32), Ok(4096));
        }
        let mut w = block::wire::blank(if flush {
            block::wire::WRITE
        } else {
            block::wire::READ
        });
        w[2] = block::wire::PAGE_PHASE;
        block::wire::put(&mut w, 4, 4096);
        w[8..16].copy_from_slice(&lba.to_le_bytes());
        pilote.put(OUTPUT, &w);
        let (port, len) = if multi {
            mots(pilote, DESC, &[83, cap, cle, lba, 0, n, n, 0]);
            (PORT_SG, 64)
        } else {
            mots(pilote, DESC, &[83, cap, cle, OUTPUT, sortie, 0, n]);
            (PORT_LOT, 56)
        };
        if flush {
            assert_eq!(pilote.call(port, DESC, len, 0), Ok(0));
            let mut barriere = block::wire::blank(block::wire::FLUSH);
            barriere[2] = block::wire::BEGIN;
            block::wire::put(&mut barriere, 32, 4);
            barriere[48..51].copy_from_slice(&[1, 1, 1]);
            block::wire::put(&mut barriere, 52, 16);
            barriere[56..59].copy_from_slice(&[3, 2, 0]);
            block::wire::put(&mut barriere, 60, 1);
            pilote.put(OUTPUT, &barriere);
            mots(pilote, DESC, &[83, cap, cle, OUTPUT, 0, n, n]);
        }
        let kicks = block::kicks();
        block::perdre_livraison_et_drainage();
        assert_eq!(pilote.call(port, DESC, len, 0), Err(REFUSED));
        let etat = |retrait| {
            super::super::super::bulk::constat_quarantaine(
                pilote.0, cap, moteur.0, table, 83, n, flush, retrait,
            );
        };
        // Le pilote a déjà fermé l’admission et lancé son retrait. La perte
        // de ce premier ACK laisse le propriétaire de la quarantaine intact.
        etat(Some(true));
        let epingles = block::cadres_temoin();
        assert!(epingles > libres);
        assert!(block::acquits_temoin_perdus());
        assert!(!block::admit(pilote.0.number, cle));
        assert!(pilote.call(port, DESC, len, 0).is_err());
        assert_eq!(block::kicks(), kicks + 1);
        block::retire().unwrap();
        block::retire().unwrap();
        etat(None);
        assert_eq!(block::cadres_temoin(), libres);
        assert!(pilote.call(port, DESC, len, 0).is_err());
        assert_eq!(block::kicks(), kicks + 1);
        verifier_page(pilote, sortie, 0, flush);
        for page in 0..n {
            verifier_page(moteur, base + page * 4096, 0, flush);
        }
        hal::log(format_args!("DMA retrait automatique op={operation} acteurs=2 tag=83 cadres={libres}/{epingles}/{libres} kicks=1 acquit_perdu=1"));
        stop(enfants, [pilote, moteur]);
    }
}
