#[path = "../../kernel/src/hardware/block/net_trames/retrait/propriete.rs"]
mod propriete;
use propriete::{restants, restituable};

#[test]
fn bouclage_used_et_identifiants_sans_ordre_impose() {
    let mut file = [(0, 0); 16];
    file[15] = (3, 72);
    file[0] = (1, 1526);
    assert_eq!(restants(65535, 1, 0b1110, true, |i| file[i]), Some(0b0100));
    assert_eq!(restants(65536, 0, 0b11, false, |_| unreachable!()), Some(3));
}

#[test]
fn retour_incoherent_conserve_toute_la_preuve_en_suspens() {
    assert_eq!(restants(0, 2, 1, true, |_| unreachable!()), None);
    assert_eq!(restants(0, 2, 3, true, |_| (0, 72)), None);
    assert_eq!(restants(0, 1, 1, true, |_| (16, 72)), None);
    assert_eq!(restants(0, 1, 1, true, |_| (1, 72)), None);
    for taille in [0, 71, 1527] {
        assert_eq!(restants(0, 1, 1, true, |_| (0, taille)), None);
    }
    assert_eq!(restants(0, 1, 1, false, |_| (0, 1)), None);
    assert_eq!(restants(0, 1, 1, false, |_| (0, 0)), Some(0));
}

#[test]
fn chaque_moitie_conditionne_la_restitution_dune_page() {
    for id in 0..16 {
        let rx = u16::MAX ^ (1 << id);
        let tx = 1 << id;
        for page in 0..22 {
            assert_eq!(
                restituable(rx, tx, page),
                page >= 6 && page != 6 + id / 2 && page != 14 + id / 2
            );
            assert!(restituable(u16::MAX, 0, page));
        }
    }
    assert!(!restituable(u16::MAX, 0, 22));
}
