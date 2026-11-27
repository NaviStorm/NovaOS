use ethernet_v1::{
    Binding, Error, Frame, Port, RX_RIGHT, Stage, TX_RIGHT, Ticket, negotiate,
    wire::{FRAME, LIMIT},
};
#[path = "../../user/killer-e2400/adaptateur.rs"]
mod adaptateur;
#[path = "../../user/killer-e2400/boucle.rs"]
mod boucle;
#[path = "../../user/killer-e2400/profil.rs"]
mod profil;
use adaptateur::{Adaptateur, Erreur, Filtre, Reception};
use boucle::{Boucle, PAIR, STATION, Tampons};

fn liaison() -> Binding {
    Binding {
        boot: [3; 16],
        port: 17,
        owner: 4,
        incarnation: 8,
        channel: 20,
        epoch: 6,
    }
}
fn adaptation() -> Adaptateur {
    Adaptateur::nouveau(liaison(), Filtre::laboratoire(STATION, PAIR).unwrap()).unwrap()
}
fn trame(tx: bool, len: usize) -> Vec<u8> {
    let mut f = vec![0x42; len];
    f[..6].copy_from_slice(if tx { &PAIR } else { &STATION });
    f[6..12].copy_from_slice(if tx { &STATION } else { &PAIR });
    f[12..14].copy_from_slice(&[0x88, 0xb5]);
    f
}
fn rrd(index: u16, nombre: u8, etat: u32) -> [u8; 16] {
    let mut d = [0; 16];
    d[..4].copy_from_slice(&((u32::from(index) << 20) | (u32::from(nombre) << 16)).to_le_bytes());
    d[12..].copy_from_slice(&etat.to_le_bytes());
    d
}
const UPDATE: u32 = 1 << 31;

#[test]
fn geometrie_directionnelle_et_fenetre_de_4_gio() {
    use profil::{AccesNic::*, REGIONS};
    let pages = |acc| {
        REGIONS
            .iter()
            .filter(|r| r.acces == acc)
            .map(|r| r.cadres())
            .sum::<usize>()
    };
    assert_eq!(
        (pages(Lecture), pages(Ecriture), profil::cadres_dma()),
        (130, 129, 259)
    );
    assert_eq!(
        REGIONS.map(|r| (r.role, r.cadres())),
        [
            ("tpd:r", 1),
            ("rfd:r", 1),
            ("rrd:w", 1),
            ("tx:r", 128),
            ("rx:w", 128)
        ]
    );
    assert!(profil::bases_anneaux_valides([
        0xffff_d000,
        0xffff_e000,
        0xffff_f000
    ]));
    assert!(profil::bases_anneaux_valides([
        0xffff_ffff_ffff_d000,
        0xffff_ffff_ffff_e000,
        0xffff_ffff_ffff_f000
    ]));
    for bases in [
        [0x1000, 0x1000, 0x2000],
        [0x1000, 0x2001, 0x3000],
        [0xffff_f000, 1 << 32, (1 << 32) + 4096],
    ] {
        assert!(!profil::bases_anneaux_valides(bases));
    }
}
#[test]
fn filtres_et_bourrage_avant_toute_reservation() {
    let k = liaison();
    let mut a = adaptation();
    let mut dst = [0x57; FRAME + 1];
    assert!(Filtre::laboratoire([0; 6], PAIR).is_err());
    assert!(Filtre::laboratoire(STATION, STATION).is_err());
    for n in [14, 59, 60, 1514] {
        let f = trame(true, n);
        assert_eq!(a.preparer(k, 0, &f, &mut dst), Ok(n.max(60)));
        assert_eq!(&dst[..n], f);
        assert!(dst[n..n.max(60)].iter().all(|&x| x == 0));
        assert_eq!(a.preparer(k, 0, &f, &mut dst), Err(Erreur::Etat));
        let publi = a.publier(k, 1).unwrap();
        assert_eq!(a.completions(k, publi.producteur.into()), Ok(1));
    }
    for f in [vec![0; 13], trame(true, 1515)] {
        let avant = dst;
        assert_eq!(a.preparer(k, 0, &f, &mut dst), Err(Erreur::Taille));
        assert_eq!(dst, avant);
    }
    for pos in [0, 6, 12] {
        let mut f = trame(true, 60);
        f[pos] ^= 1;
        let avant = dst;
        assert_eq!(a.preparer(k, 7, &f, &mut dst), Err(Erreur::Filtre));
        assert_eq!(dst, avant);
    }
    assert_eq!(
        a.preparer(k, 0, &trame(true, 59), &mut dst[..59]),
        Err(Erreur::Taille)
    );
    assert_eq!(a.retirer(k).unwrap().tx_incertains, 0);
}
#[test]
fn huit_credits_prefixes_et_bouclages_consommateur() {
    let k = liaison();
    let mut a = adaptation();
    let mut dst = [0; 60];
    let frame = trame(true, 60);
    let mut compteur = 0u8;
    for _ in 0..64 {
        for i in (0..LIMIT).rev() {
            a.preparer(k, i, &frame, &mut dst).unwrap();
        }
        assert_eq!(a.preparer(k, 0, &frame, &mut dst), Err(Erreur::Plein));
        assert!(a.publier(k, 9).is_err());
        let p = a.publier(k, 3).unwrap();
        assert_eq!(&p.coupons[..p.nombre], &[7, 6, 5]);
        compteur = compteur.wrapping_add(3);
        assert_eq!(p.producteur, compteur);
        assert_eq!(a.completions(k, compteur.into()), Ok(0xe0));
        let p = a.publier(k, 5).unwrap();
        assert_eq!(p.nombre, 5);
        compteur = compteur.wrapping_add(5);
        assert_eq!(a.completions(k, compteur.into()), Ok(0x1f));
        assert_eq!(a.completions(k, compteur.into()), Ok(0));
    }
}
#[test]
fn avance_impossible_ou_index_seize_bits_suspend_les_deux_sens() {
    let k = liaison();
    for mauvais in [2, 255, 256, 65535] {
        let mut a = adaptation();
        a.preposter_rx(k, 1).unwrap();
        a.preparer(k, 4, &trame(true, 60), &mut [0; 60]).unwrap();
        a.publier(k, 1).unwrap();
        assert_eq!(a.completions(k, mauvais), Err(Erreur::Quarantaine));
        assert_eq!(a.preposter_rx(k, 1), Err(Erreur::Quarantaine));
        assert_eq!(a.completions(k, 1), Err(Erreur::Quarantaine));
        let bilan = a.retirer(k).unwrap();
        assert_eq!(
            (
                bilan.tx_incertains,
                bilan.rx_pretes,
                bilan.cadres_a_conserver_si_exposes
            ),
            (1, 1, 259)
        );
    }
}
#[test]
fn rx_consomme_un_rfd_et_exclut_fcs_et_queue_du_tampon() {
    let k = liaison();
    let mut a = adaptation();
    assert_eq!(a.preposter_rx(k, 256), Err(Erreur::Plein));
    a.preposter_rx(k, 255).unwrap();
    assert_eq!(a.preposter_rx(k, 1), Err(Erreur::Plein));
    for i in 0..513u16 {
        let len = if i % 2 == 0 { 60 } else { 1514 };
        let mut buf = trame(false, len);
        buf.extend_from_slice(&[0xca; 16]);
        assert_eq!(
            a.reception(k, &rrd(4095, 15, 0), &[]),
            Ok(Reception::Attente)
        );
        assert_eq!(
            a.reception(k, &rrd(i % 256, 1, UPDATE | (len as u32 + 4)), &buf),
            Ok(Reception::Trame(&buf[..len]))
        );
        a.preposter_rx(k, 1).unwrap();
    }
}
#[test]
fn rx_erreurs_trames_filtres_et_tailles_sans_restitution_dma() {
    let k = liaison();
    let mut a = adaptation();
    let f = trame(false, 60);
    let mut id = 0;
    for statut in [0, 3, 63, 1519, 2048]
        .into_iter()
        .chain([16, 20, 21, 22, 23, 24, 25, 29, 30].map(|b| 64 | (1 << b)))
    {
        a.preposter_rx(k, 1).unwrap();
        assert_eq!(
            a.reception(k, &rrd(id, 1, UPDATE | statut), &f),
            Ok(Reception::Ecartee)
        );
        id += 1;
    }
    for i in [0, 6, 12] {
        let mut f = f.clone();
        f[i] ^= 1;
        a.preposter_rx(k, 1).unwrap();
        assert_eq!(
            a.reception(k, &rrd(id, 1, UPDATE | 64), &f),
            Ok(Reception::Ecartee)
        );
        id += 1;
    }
    a.preposter_rx(k, 1).unwrap();
    assert_eq!(
        a.reception(k, &rrd(id, 1, UPDATE | 64), &f),
        Ok(Reception::Trame(&f))
    );
}
#[test]
fn rx_retour_ambigu_conserve_les_prets_et_refuse_le_rejeu() {
    let k = liaison();
    let mut impossible = adaptation();
    impossible.preposter_rx(k, 2).unwrap();
    assert_eq!(
        impossible.reception(k, &rrd(0, 1, UPDATE | 2049), &[]),
        Err(Erreur::Quarantaine)
    );
    assert_eq!(impossible.retirer(k).unwrap().rx_pretes, 2);
    for (index, count, octets) in [(1, 1, 60), (0, 0, 60), (0, 2, 60), (256, 1, 60), (0, 1, 59)] {
        let mut a = adaptation();
        a.preposter_rx(k, 2).unwrap();
        let f = trame(false, octets);
        assert_eq!(
            a.reception(k, &rrd(index, count, UPDATE | 64), &f),
            Err(Erreur::Quarantaine)
        );
        assert_eq!(a.retirer(k).unwrap().rx_pretes, 2);
    }
    let mut a = adaptation();
    a.preposter_rx(k, 1).unwrap();
    let f = trame(false, 60);
    let ancien = rrd(0, 1, UPDATE | 64);
    assert_eq!(a.reception(k, &ancien, &f), Ok(Reception::Trame(&f)));
    a.preposter_rx(k, 1).unwrap();
    assert_eq!(a.reception(k, &ancien, &f), Err(Erreur::Quarantaine));
}
#[test]
fn titulaire_incarnation_generation_et_canal_lies() {
    let k = liaison();
    let mut a = adaptation();
    let faux = [
        Binding { boot: [0; 16], ..k },
        Binding {
            incarnation: 9,
            ..k
        },
        Binding { epoch: 7, ..k },
        Binding { owner: 5, ..k },
        Binding { channel: 2, ..k },
        Binding { port: 7, ..k },
    ];
    for autre in faux {
        assert_eq!(a.preposter_rx(autre, 1), Err(Erreur::Liaison));
        assert_eq!(a.retirer(autre), Err(Erreur::Liaison));
    }
    let ancien = a.retirer(k).unwrap();
    assert_eq!(a.retirer(k).unwrap(), ancien);
    assert_eq!(
        a.preparer(k, 7, &trame(true, 60), &mut [0; 60]),
        Err(Erreur::Quarantaine)
    );
    let suivant = Binding {
        epoch: 7,
        incarnation: 9,
        ..k
    };
    let mut nouveau =
        Adaptateur::nouveau(suivant, Filtre::laboratoire(STATION, PAIR).unwrap()).unwrap();
    assert_eq!(nouveau.preposter_rx(k, 1), Err(Erreur::Liaison));
    assert_eq!(nouveau.preposter_rx(suivant, 1), Ok(()));
}
#[test]
fn publication_incertaine_reconcilie_les_tickets_v1_sans_rendre_le_prefixe() {
    let k = liaison();
    let mut a = adaptation();
    let droits = TX_RIGHT | RX_RIGHT;
    let mut port = Port::open(
        k,
        droits,
        droits,
        negotiate(1, 0, 0, 0, 0).unwrap(),
        STATION,
    )
    .unwrap();
    let octets = trame(true, 60);
    let frames: Vec<_> = (0..8)
        .map(|i| Frame {
            slot: port.tx_slot(k, i).unwrap(),
            bytes: &octets,
        })
        .collect();
    let ticket = port.submit(k, 1, &frames).unwrap();
    for i in 0..8 {
        let t = Ticket {
            index: i as u8,
            ..ticket
        };
        let mut backend = port.backend();
        backend
            .transition(t, Stage::Admitted, Stage::Publishing)
            .unwrap();
        a.preparer(k, i, backend.image(t).unwrap(), &mut [0; 60])
            .unwrap();
    }
    // Deux publications confirmées, trois d'issue incertaine, trois jamais tentées.
    a.publier(k, 2).unwrap();
    for i in 0..2 {
        port.backend()
            .transition(
                Ticket { index: i, ..ticket },
                Stage::Publishing,
                Stage::Published,
            )
            .unwrap();
    }
    a.preposter_rx(k, 8).unwrap();
    let bilan = a.publication_incertaine(k, 3).unwrap();
    assert_eq!(
        (bilan.tx_incertains, bilan.tx_annules, bilan.rx_pretes),
        (5, 3, 8)
    );
    for i in 0..8 {
        let t = Ticket { index: i, ..ticket };
        if i < 5 {
            port.backend().incertain(t).unwrap();
        } else {
            port.backend().cancel_unpublished(t).unwrap();
        }
        assert_eq!(
            port.status(k, t),
            Ok(if i < 5 {
                Stage::Unknown
            } else {
                Stage::Cancelled
            })
        );
    }
    assert_eq!(port.submit(k, 1, &frames), Ok(ticket));
    assert_eq!(port.credits(k), Ok(3));
    assert_eq!(port.ack(k, 1), Err(Error::State));
    assert_eq!(a.completions(k, 5), Err(Erreur::Quarantaine));
    let retraite = port.revoke();
    assert_eq!((retraite.uncertain, retraite.cancelled), (5, 3));
    assert_eq!(a.retirer(k), Ok(bilan));
}
#[test]
fn pair_cpu_contre_pression_et_retour_unique_par_coupon() {
    let mut tampons = Tampons::VIDES;
    let mut p = Boucle::nouvelle(liaison(), &mut tampons).unwrap();
    let frame = trame(true, 60);
    for i in 0..8 {
        p.preparer(i, &frame).unwrap();
    }
    p.publier(8).unwrap();
    assert_eq!(p.preparer(0, &frame), Err(Erreur::Etat));
    assert_eq!(p.completions(3), 7);
    assert_eq!(p.completions(8), 248);
    assert_eq!(p.completions(1), 0);
    let mut dst = [0; FRAME];
    for _ in 0..8 {
        assert_eq!(p.recevoir(&mut dst).unwrap(), Some(60));
        assert_eq!(&dst[..6], &STATION);
    }
    assert_eq!(p.recevoir(&mut dst).unwrap(), None);
    p.fermer().unwrap();
}

#[test]
fn perte_du_pair_apres_une_demande_de_publication_partielle() {
    let mut tampons = Tampons::VIDES;
    let mut p = Boucle::nouvelle(liaison(), &mut tampons).unwrap();
    for id in 0..8 {
        p.preparer(id, &trame(true, 60)).unwrap();
    }
    let bilan = p.perdre_recu(4).unwrap();
    assert_eq!(
        (bilan.tx_incertains, bilan.tx_annules, bilan.rx_pretes),
        (4, 4, 8)
    );
    assert_eq!(bilan.cadres_a_conserver_si_exposes, 259);
}

#[test]
fn retrait_dune_session_laisse_progresser_lautre() {
    let k = liaison();
    let voisin = Binding {
        owner: 11,
        port: 18,
        channel: 21,
        ..k
    };
    let mut a = adaptation();
    let mut b = Adaptateur::nouveau(voisin, Filtre::laboratoire(STATION, PAIR).unwrap()).unwrap();
    let f = trame(true, 60);
    a.preparer(k, 0, &f, &mut [0; 60]).unwrap();
    b.preparer(voisin, 0, &f, &mut [0; 60]).unwrap();
    a.publication_incertaine(k, 1).unwrap();
    assert_eq!(b.completions(k, 1), Err(Erreur::Liaison));
    let confirme = b.publier(voisin, 1).unwrap();
    assert_eq!(b.completions(voisin, u16::from(confirme.producteur)), Ok(1));
    assert_eq!(a.completions(k, 1), Err(Erreur::Quarantaine));
}
