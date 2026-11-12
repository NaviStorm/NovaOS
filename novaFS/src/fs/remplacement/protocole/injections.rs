use super::*;
use crate::device::SimFault;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Etat {
    Ancien,
    Nouveau,
    Refus,
}

fn verifier(
    e: &Experience,
    image: &SimSnapshot,
    attendu: Etat,
    copies: usize,
    ancres: usize,
) -> Result<()> {
    let disque = image.device();
    let controle = disque.control();
    controle.trace(None, None);
    let reprise = recuperer(&disque, IDS);
    if attendu == Etat::Refus {
        assert!(
            matches!(reprise, Err(Error::Corrupt(_))),
            "un graphe ambigu a été ouvert"
        );
    } else {
        let mut reprise = reprise?;
        let publie = attendu == Etat::Nouveau;
        let recu = if publie {
            e.decision.tete
        } else {
            e.checkpoint.tete
        };
        assert_eq!(reprise.fs.receipt(), recu);
        e.cas.verifier(&reprise.fs, publie)?;
        assert_eq!(reprise.copies_manquantes_ou_invalides, copies);
        assert_eq!(reprise.ancres_invalides, ancres);
        assert!(matches!(
            reprise.fs.create("/interdit"),
            Err(Error::ReadOnly)
        ));
    }
    assert!(
        controle.events().is_empty(),
        "écriture ou flush pendant la reprise"
    );
    Ok(())
}

/// Oracle indépendant du lecteur : comparer aux deux commandes réellement
/// émises sur une autre branche. Seul le test connaît ces octets.
fn oracle(e: &Experience, image: &SimSnapshot) -> Result<(Etat, usize)> {
    let d = image.device();
    let mut completes = 0;
    let mut vides = 0;
    for b in adresses(e.taille, 1) {
        let page = d.read(b)?;
        completes += usize::from(page == e.decision.encoder(IDS, e.taille, b));
        vides += usize::from(page == [0; PAGE]);
    }
    Ok(if completes > 0 {
        (Etat::Nouveau, 2 - completes)
    } else if vides == 2 {
        (Etat::Ancien, 0)
    } else {
        (Etat::Refus, 0)
    })
}

fn couper(
    e: &Experience,
    inverse: bool,
    point: usize,
    politique: Persistence,
) -> Result<SimSnapshot> {
    let mut d = e.depart.device();
    let ctl = d.control();
    ctl.trace(Some(point), None);
    assert!(e.publier(&mut d, inverse).is_err());
    ctl.power_cut(politique)?;
    Ok(ctl.snapshot())
}

const PLAGES: &[(usize, usize)] = &[
    (0, 1),
    (0, 32),
    (0, 64),
    (0, 96),
    (0, 128),
    (0, 184),
    (0, 512),
    (0, 2048),
    (0, PAGE),
    (55, 2),
    (104, 48),
    (135, 32),
    (511, 2),
    (1024, 2048),
    (SOMME - 1, 2),
    (SOMME, 32),
    (PAGE - 1, 1),
];

fn frontieres(e: &Experience, secteur: u32, inverse: bool, trace: &[Event]) -> Result<[usize; 3]> {
    let mut bilan = [0; 3];
    let index = |ev| trace.iter().position(|&v| v == ev).unwrap();
    let f1 = index(Event::FlushCompleted(1));
    let stable = index(Event::FlushEffect(2));
    let mut examiner = |point, politique| -> Result<()> {
        let image = couper(e, inverse, point, politique)?;
        let (attendu, copies) = oracle(e, &image)?;
        if point <= f1 {
            assert_eq!(attendu, Etat::Ancien);
        }
        if point >= stable {
            assert_eq!((attendu, copies), (Etat::Nouveau, 0));
        }
        verifier(e, &image, attendu, copies, 0)?;
        bilan[attendu as usize] += 1;
        Ok(())
    };
    for point in 0..trace.len() {
        for politique in [
            Persistence::None,
            Persistence::Newest(1),
            Persistence::Newest(usize::MAX),
        ] {
            examiner(point, politique)?;
        }
        for block in adresses(e.taille, 1) {
            for &(offset, bytes) in PLAGES {
                examiner(
                    point,
                    Persistence::Range {
                        block,
                        offset,
                        bytes,
                    },
                )?;
            }
        }
    }
    for block in adresses(e.taille, 1) {
        for masque in 0..(1u16 << (PAGE / secteur as usize)) {
            examiner(
                index(Event::FlushSubmitted(2)),
                Persistence::Sectors {
                    block,
                    mask: masque as u8,
                },
            )?;
        }
    }
    assert!(bilan.iter().all(|&n| n > 0));
    Ok(bilan)
}

/// Une persistance contraire à F1 est imposée sur une branche indépendante.
/// Explorer chaque sous-ensemble de dépendances avec zéro, un ou deux commits.
fn reordonnements(e: &Experience) -> Result<[usize; 3]> {
    let mut bilan = [0; 3];
    let complet = (1 << e.dependances.len()) - 1;
    for masque in 0..=complet {
        let d = e.depart.device();
        for (i, (b, page)) in e.dependances.iter().enumerate() {
            if masque & (1 << i) != 0 {
                d.control().replace_stable(*b, *page)?;
            }
        }
        for copies in 0u8..4 {
            let branche = d.control().snapshot().device();
            for (i, b) in adresses(e.taille, 1).into_iter().enumerate() {
                if copies & (1 << i) != 0 {
                    branche
                        .control()
                        .replace_stable(b, e.decision.encoder(IDS, e.taille, b))?;
                }
            }
            let (attendu, degradees) = match (copies, masque == complet) {
                (0, _) => (Etat::Ancien, 0),
                (_, false) => (Etat::Refus, 0),
                (_, true) => (Etat::Nouveau, 2 - copies.count_ones() as usize),
            };
            verifier(e, &branche.control().snapshot(), attendu, degradees, 0)?;
            bilan[attendu as usize] += 1;
        }
    }
    Ok(bilan)
}

fn alterer(d: &SimDevice, bloc: u64, offset: usize) -> Result<()> {
    let mut page = d.read(bloc)?;
    page[offset] ^= 1;
    d.control().replace_stable(bloc, page)
}

fn defauts_apres_confirmation(e: &Experience, succes: &SimSnapshot) -> Result<usize> {
    let mut nombre = 0;
    for position in [0, 1] {
        for b in adresses(e.taille, position) {
            for offset in [0, 8, 48, 56, 104, 152, 511, 512, SOMME, PAGE - 1] {
                let d = succes.device();
                alterer(&d, b, offset)?;
                verifier(e, &d.control().snapshot(), Etat::Nouveau, 1, 0)?;
                nombre += 1;
            }
            let d = succes.device();
            d.control().replace_stable(b, [0; PAGE])?;
            verifier(e, &d.control().snapshot(), Etat::Nouveau, 1, 0)?;
            nombre += 1;
        }
    }
    for masque in 1u8..4 {
        let d = succes.device();
        for slot in 0..2 {
            if masque & (1 << slot) != 0 {
                alterer(&d, slot, 136)?;
            }
        }
        verifier(
            e,
            &d.control().snapshot(),
            Etat::Nouveau,
            0,
            masque.count_ones() as usize,
        )?;
        nombre += 1;
    }
    for b in e
        .dependances
        .iter()
        .map(|(b, _)| *b)
        .chain([e.decision.parent.block])
    {
        let d = succes.device();
        alterer(&d, b, 64)?;
        verifier(e, &d.control().snapshot(), Etat::Refus, 0, 0)?;
        nombre += 1;
    }
    let [a, b] = adresses(e.taille, 1);
    let d = succes.device();
    alterer(&d, a, SOMME)?;
    alterer(&d, b, SOMME)?;
    verifier(e, &d.control().snapshot(), Etat::Refus, 0, 0)?;
    nombre += 1;

    // Deux branches P9 valides ont le même parent et la même génération.
    // Le lecteur doit refuser leur désaccord avant toute sélection de graphe.
    let (mut soeur, _) = e.cas.ouvrir()?;
    soeur.create("/branche")?;
    soeur.verify()?;
    assert_eq!(soeur.receipt().birth, e.decision.tete.birth);
    assert_ne!(soeur.receipt(), e.decision.tete);
    let d = succes.device();
    let autre = Decision {
        tete: soeur.receipt(),
        ..e.decision
    };
    d.control()
        .replace_stable(b, autre.encoder(IDS, e.taille, b))?;
    verifier(e, &d.control().snapshot(), Etat::Refus, 0, 0)?;
    nombre += 1;
    let d = succes.device();
    let autre_parent = Decision {
        parent: Ref {
            hash: [0x93; 32],
            ..e.decision.parent
        },
        ..e.decision
    };
    for b in adresses(e.taille, 1) {
        d.control()
            .replace_stable(b, autre_parent.encoder(IDS, e.taille, b))?;
    }
    verifier(e, &d.control().snapshot(), Etat::Refus, 0, 0)?;
    nombre += 1;
    let d = succes.device();
    for adresse in adresses(e.taille, 1) {
        d.control().replace_stable(adresse, [0; PAGE])?;
    }
    for b in adresses(e.taille, 2) {
        d.control()
            .replace_stable(b, e.decision.encoder(IDS, e.taille, b))?;
    }
    verifier(e, &d.control().snapshot(), Etat::Refus, 0, 0)?;
    nombre += 1;

    let d = succes.device();
    let mauvaise_identite = Ids {
        lv: [0x91; 16],
        ..IDS
    };
    for adresse in [a, b] {
        d.control().replace_stable(
            adresse,
            e.decision.encoder(mauvaise_identite, e.taille, adresse),
        )?;
    }
    verifier(e, &d.control().snapshot(), Etat::Refus, 0, 0)?;
    nombre += 1;
    let d = succes.device();
    d.control().replace_stable(a, d.read(b)?)?;
    verifier(e, &d.control().snapshot(), Etat::Nouveau, 1, 0)?;
    nombre += 1;
    Ok(nombre)
}

fn histoires(e: &Experience, succes: &SimSnapshot, trace: &[Event]) -> Result<[String; 2]> {
    let index = |ev| trace.iter().position(|&v| v == ev).unwrap();
    let [a, b] = adresses(e.taille, 1);
    let avant = couper(e, false, index(Event::BeforeWrite(a)), Persistence::None)?;
    let apres_f2 = couper(e, false, index(Event::FlushCompleted(2)), Persistence::None)?;
    // Le premier appel a échoué avant de livrer son succès, le second l'a rendu.
    assert!(apres_f2.bytes()? == succes.bytes()?);
    verifier(e, &apres_f2, Etat::Nouveau, 0, 0)?;

    // Deux pertes corrélées après un vrai F2 produisent le support entier de
    // la branche interrompue après F1. Il ne reste aucune anomalie à signaler.
    let d = succes.device();
    for bloc in [a, b] {
        d.control().replace_stable(bloc, [0; PAGE])?;
    }
    let identique = avant.bytes()?;
    assert!(d.control().snapshot().bytes()? == identique);
    verifier(e, &avant, Etat::Ancien, 0, 0)?;
    verifier(e, &d.control().snapshot(), Etat::Ancien, 0, 0)?;

    // Même effacement observable si F2 ment et que la vraie panne perd le cache.
    let mut d = e.depart.device();
    let ctl = d.control();
    ctl.trace(None, Some(2));
    e.publier(&mut d, false)?;
    ctl.power_cut(Persistence::None)?;
    assert!(ctl.snapshot().bytes()? == identique);
    verifier(e, &ctl.snapshot(), Etat::Ancien, 0, 0)?;

    // Le pilote reçoit une erreur de F2 après l'effet possible d'une copie.
    // La lecture décide de la publication, l'erreur interdit l'acquittement.
    for n in 0..=2 {
        let mut d = e.depart.device();
        d.control().inject(SimFault::Flush {
            ordinal: 2,
            persist_newest: n,
        })?;
        assert!(e.publier(&mut d, false).is_err());
        d.control().power_cut(Persistence::None)?;
        let image = d.control().snapshot();
        let (attendu, copies) = oracle(e, &image)?;
        assert_eq!(attendu, if n == 0 { Etat::Ancien } else { Etat::Nouveau });
        verifier(e, &image, attendu, copies, 0)?;
    }
    Ok([
        format!("{:x}", Sha256::digest(identique)),
        format!("{:x}", Sha256::digest(succes.bytes()?)),
    ])
}

fn compatibilite(e: &Experience, succes: &SimSnapshot, secteur: u32) -> Result<()> {
    let octets = succes.bytes()?;
    let prefixe = SimDevice::from_bytes(&octets[..e.taille as usize * PAGE], secteur)?;
    let ancien_lecteur = Engine::mount_current(prefixe, IDS, Mode::ReadOnly)?;
    assert_eq!(ancien_lecteur.receipt(), e.checkpoint.tete);
    e.cas.verifier(&ancien_lecteur, false)?;
    assert!(Engine::mount_current(succes.device(), IDS, Mode::ReadOnly).is_err());
    assert!(recuperer(&e.cas.image.device(), IDS).is_err());
    Ok(())
}

fn suite_du_journal(e: &Experience, succes: &SimSnapshot) -> Result<()> {
    let (mut source, ctl) = e.cas.ouvrir()?;
    remplacer(&mut source, e.cas.source, e.cas.destination)?;
    assert_eq!(source.receipt(), e.decision.tete);
    ctl.trace(None, None);
    source.create("/suite")?;
    let seconde = Decision {
        parent: e.decision.tete,
        tete: source.receipt(),
    };
    let pages = pages_preparees(&ctl, seconde.tete)?;

    let mut degrade = succes.device();
    alterer(&degrade, adresses(e.taille, 1)[1], 152)?;
    let ctl = degrade.control();
    ctl.trace(None, None);
    assert!(ajouter(&mut degrade, seconde, &pages, false).is_err() && ctl.events().is_empty());

    let mut disque = succes.device();
    ajouter(&mut disque, seconde, &pages, true)?;
    let relu = recuperer(&disque, IDS)?;
    assert_eq!(relu.prochaine, PAIRES);
    assert_eq!(relu.fs.receipt(), seconde.tete);
    e.cas.verifier(&relu.fs, true)?;
    assert_eq!(relu.fs.stat("/suite")?.id, source.stat("/suite")?.id);
    let ctl = disque.control();
    ctl.trace(None, None);
    assert!(matches!(
        ajouter(&mut disque, seconde, &pages, false),
        Err(Error::NoSpace)
    ));
    assert!(ctl.events().is_empty());

    // Une génération visible après une lacune interdit d'arrêter le scan au
    // premier trou. Les deux graphes peuvent pourtant être entièrement intacts.
    for b in adresses(e.taille, 1) {
        ctl.replace_stable(b, [0; PAGE])?;
    }
    assert!(matches!(
        recuperer(&disque, IDS),
        Err(Error::Corrupt("rupture du journal"))
    ));
    Ok(())
}

fn erreurs_avant_f2(e: &Experience) -> Result<usize> {
    let mut nombre = 0;
    for n in [0, e.dependances.len()] {
        let mut d = e.depart.device();
        let ctl = d.control();
        ctl.trace(None, None);
        ctl.inject(SimFault::Flush {
            ordinal: 1,
            persist_newest: n,
        })?;
        assert!(e.publier(&mut d, false).is_err());
        for b in adresses(e.taille, 1) {
            assert!(!ctl.events().contains(&Event::BeforeWrite(b)));
        }
        ctl.power_cut(Persistence::None)?;
        verifier(e, &ctl.snapshot(), Etat::Ancien, 0, 0)?;
        nombre += 1;
    }
    for block in adresses(e.taille, 1) {
        for prefix in [0, 96, PAGE] {
            let mut d = e.depart.device();
            let ctl = d.control();
            ctl.inject(SimFault::Write { block, prefix })?;
            assert!(e.publier(&mut d, false).is_err());
            ctl.power_cut(Persistence::Newest(usize::MAX))?;
            let image = ctl.snapshot();
            let (attendu, copies) = oracle(e, &image)?;
            verifier(e, &image, attendu, copies, 0)?;
            nombre += 1;
        }
    }
    Ok(nombre)
}

#[test]
#[ignore = "audit du protocole de publication, sur support synthétique uniquement"]
fn journal_sur_un_seul_support() -> Result<()> {
    for secteur in [512, 4096] {
        for copie in [false, true] {
            let e = Experience::neuve(secteur, copie)?;
            assert_eq!(e.dependances.len(), 5);
            let mut compte = [0; 3];
            let mut evenements = 0;
            let mut defauts = 0;
            let mut images = None;
            for inverse in [false, true] {
                let mut d = e.depart.device();
                let ctl = d.control();
                ctl.trace(None, None);
                e.publier(&mut d, inverse)?;
                let succes = ctl.snapshot();
                let trace = ctl.events();
                let [a, b] = adresses(e.taille, 1);
                let (premier, second) = if inverse { (b, a) } else { (a, b) };
                let idx = |ev| trace.iter().position(|&v| v == ev).unwrap();
                assert!(idx(Event::FlushCompleted(1)) < idx(Event::BeforeWrite(premier)));
                assert!(idx(Event::WriteCompleted(premier)) < idx(Event::BeforeWrite(second)));
                assert!(idx(Event::WriteCompleted(second)) < idx(Event::FlushSubmitted(2)));
                assert_eq!(trace.last(), Some(&Event::FlushCompleted(2)));
                verifier(&e, &succes, Etat::Nouveau, 0, 0)?;
                ctl.trace(None, None);
                assert!(e.publier(&mut d, inverse).is_err() && ctl.events().is_empty());
                for (c, n) in compte
                    .iter_mut()
                    .zip(frontieres(&e, secteur, inverse, &trace)?)
                {
                    *c += n;
                }
                evenements += trace.len();
                if !inverse {
                    defauts = defauts_apres_confirmation(&e, &succes)?;
                    images = Some(histoires(&e, &succes, &trace)?);
                    compatibilite(&e, &succes, secteur)?;
                    suite_du_journal(&e, &succes)?;
                }
            }
            let reordres = reordonnements(&e)?;
            println!(
                "PROTOCOLE {}",
                serde_json::json!({
                    "secteur":secteur, "copie":copie, "generation":e.decision.tete.birth,
                    "octets_support":e.depart.bytes()?.len(), "dependances":e.dependances.len(),
                    "evenements_deux_ordres":evenements, "plages":PLAGES.len(),
                    "coupures_ancien_nouveau_refus":compte, "sous_ensembles_ancien_nouveau_refus":reordres,
                    "defauts_apres_succes":defauts, "erreurs_avant_F2":erreurs_avant_f2(&e)?, "erreurs_F2":3,
                    "sha256_ambigu_et_publie":images.unwrap()
                })
            );
        }
    }
    Ok(())
}
