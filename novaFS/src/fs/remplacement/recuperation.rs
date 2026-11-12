//! Qualification hôte uniquement. L'observateur du banc connaît les reçus des
//! appels terminés ; un montage réel ne dispose ni de cet oracle ni de la trace.
use super::*;
use crate::{Block, Manifest, PairStatus, Ref, check_image, codec::Anchor};

/// (début, longueur), en octets dans la commande d'ancrage de 4 Kio.
const PLAGES: &[(usize, usize)] = &[
    (0, 1),
    (0, 32),
    (0, 56),
    (0, 64),
    (0, 96),
    (0, 128),
    (0, 183),
    (0, 184),
    (0, 256),
    (0, 512),
    (0, 1024),
    (0, 2048),
    (0, 4096),
    (1, 32),
    (55, 2),
    (56, 8),
    (63, 2),
    (64, 1),
    (64, 32),
    (95, 2),
    (96, 32),
    (128, 64),
    (136, 8),
    (144, 8),
    (152, 32),
    (183, 2),
    (256, 256),
    (511, 2),
    (512, 512),
    (512, 1024),
    (1024, 2048),
    (4095, 1),
];

fn index(trace: &[Event], e: Event) -> usize {
    trace
        .iter()
        .position(|&vu| vu == e)
        .expect("événement de publication")
}

/// Le contenu attendu vient des octets stables de deux transactions témoins,
/// indépendamment du décodeur du montage et des décisions du simulateur.
fn verifier_image(
    cas: &Cas,
    image: &SimSnapshot,
    slot: u64,
    attendu: &Block,
    normal: Option<(Ref, bool)>,
    precedent: Ref,
) -> Result<()> {
    assert_eq!(&image.device().read(slot)?, attendu);
    for mode in [Mode::ReadOnly, Mode::ReadWrite] {
        let disque = image.device();
        let ctl = disque.control();
        ctl.trace(None, None);
        let montage = Engine::mount_current(disque, IDS, mode);
        if let Some((recu, publie)) = normal {
            let fs = montage?;
            assert_eq!(fs.receipt(), recu);
            cas.verifier(&fs, publie)?;
        } else {
            assert!(matches!(montage, Err(Error::Corrupt(_))));
            assert!(ctl.events().is_empty());
        }
    }
    if normal.is_none() {
        let disque = image.device();
        let ctl = disque.control();
        ctl.trace(None, None);
        let rapport = check_image(&disque, IDS);
        assert!(matches!(rapport.pair, PairStatus::Unavailable));
        assert!(rapport.slots[slot as usize].anchor.is_none());
        let autre = &rapport.slots[(1 - slot) as usize];
        assert!(autre.issue.is_none());
        assert_eq!(autre.anchor.unwrap().manifest, precedent);
        let mut secours = Engine::salvage(disque, IDS, 1 - slot)?;
        cas.verifier(&secours, false)?;
        assert!(matches!(secours.create("/interdit"), Err(Error::ReadOnly)));
        assert!(ctl.events().is_empty());
    }
    Ok(())
}

#[test]
fn plages_et_frontieres_des_deux_synchronisations() -> Result<()> {
    for secteur in [512, 4096] {
        for copie in [false, true] {
            for decaler in [false, true] {
                let cas = Cas::neuf(secteur, copie, decaler)?;
                let (mut fs, ctl) = cas.ouvrir()?;
                let g = fs.receipt();
                ctl.trace(None, None);
                remplacer(&mut fs, cas.source, cas.destination)?;
                let suivant = fs.receipt();
                let slot = suivant.birth % 2;
                let avant = cas.image.device().read(slot)?;
                let apres = ctl.snapshot().device().read(slot)?;
                let trace = ctl.events();
                let soumise = index(&trace, Event::WriteSubmitted(slot));
                let stable = index(&trace, Event::FlushEffect(2));
                let phases = [
                    Event::FlushSubmitted(1),
                    Event::FlushEffect(1),
                    Event::FlushCompleted(1),
                    Event::BeforeWrite(slot),
                    Event::WriteSubmitted(slot),
                    Event::WriteCompleted(slot),
                    Event::FlushSubmitted(2),
                    Event::FlushEffect(2),
                    Event::FlushCompleted(2),
                ];
                assert!(
                    phases
                        .windows(2)
                        .all(|p| index(&trace, p[0]) < index(&trace, p[1]))
                );
                let mut bilan = [0; 3];
                for phase in phases {
                    let i = index(&trace, phase);
                    for &(offset, bytes) in PLAGES {
                        let image = cas.interrompre(
                            i,
                            Persistence::Range {
                                block: slot,
                                offset,
                                bytes,
                            },
                        )?;
                        let mut attendu = avant;
                        if i >= stable {
                            attendu = apres;
                        } else if i >= soumise {
                            let r = offset..offset + bytes;
                            attendu[r.clone()].copy_from_slice(&apres[r]);
                        }
                        let (categorie, normal) = if attendu == avant {
                            (0, Some((g, false)))
                        } else if attendu == apres {
                            (1, Some((suivant, true)))
                        } else {
                            (2, None)
                        };
                        verifier_image(&cas, &image, slot, &attendu, normal, g)?;
                        bilan[categorie] += 1;
                    }
                }
                // En géométrie 4 Kio, une ancre occupe exactement un secteur.
                // Avant F1, deux commandes de dépendances peuvent persister ensemble.
                let f1 = index(&trace, Event::FlushSubmitted(1));
                let partiel = cas.interrompre(f1, Persistence::Newest(2))?;
                let d = partiel.device();
                let base = cas.image.device();
                let finalise = ctl.snapshot().device();
                let commandes: Vec<_> = trace[..f1]
                    .iter()
                    .filter_map(|e| match e {
                        Event::WriteCompleted(b) => Some(*b),
                        _ => None,
                    })
                    .collect();
                assert!(commandes.len() > 2);
                // Comparer les adresses écrites à la propriété effective des
                // DEUX racines préexistantes, pas seulement aux octets finaux.
                let mut proteges = alloc::collections::BTreeSet::from([0, 1]);
                for s in [0, 1] {
                    let vue = Engine::salvage(cas.image.device(), IDS, s)?;
                    proteges.insert(vue.receipt().block);
                    for f in Family::ALL {
                        tree::walk(
                            &vue,
                            vue.manifest().roots[f.index()],
                            f,
                            |r| {
                                proteges.insert(r.block);
                                Ok(())
                            },
                            |_, _| Ok(()),
                        )?;
                    }
                    data_pages(&vue, |b| {
                        proteges.insert(b);
                        Ok(())
                    })?;
                }
                assert!(commandes.iter().all(|b| !proteges.contains(b)));
                let deux = &commandes[commandes.len() - 2..];
                assert_ne!(deux[0], deux[1]);
                for b in 0..d.blocks() {
                    let initial = base.read(b)?;
                    if deux.contains(&b) {
                        assert_ne!(d.read(b)?, initial);
                        assert_eq!(d.read(b)?, finalise.read(b)?);
                    } else {
                        assert_eq!(d.read(b)?, initial);
                    }
                }
                verifier_image(&cas, &partiel, slot, &avant, Some((g, false)), g)?;
                println!(
                    "ANCRES secteur={secteur} copie={copie} slot={slot} plages={} phases={} ancien={} nouveau={} refus={} deux_blocs_F1=ancien",
                    PLAGES.len(),
                    phases.len(),
                    bilan[0],
                    bilan[1],
                    bilan[2]
                );
            }
        }
    }
    Ok(())
}

/// Expérience sur une copie : reconstruire uniquement le parent directement
/// engagé par la tête dont le reçu *frais* est imposé par l'observateur.
/// Un manifeste trouvé par balayage n'acquiert aucune autorité de publication.
fn paire_sur_copie(image: &SimSnapshot, survivant: u64, temoin: Ref) -> Result<Engine<SimDevice>> {
    let d = image.device();
    let fs = Engine::salvage(image.device(), IDS, survivant)?;
    fs.verify()?;
    if fs.receipt() != temoin {
        return Err(Error::Corrupt("reçu externe différent"));
    }
    let m = fs.manifest();
    let mut candidat = None;
    for block in 2..d.blocks() {
        let r = Ref {
            block,
            birth: m.previous,
            hash: m.previous_hash,
        };
        if Manifest::decode(&d.read(block)?, IDS, r).is_ok() && candidat.replace(r).is_some() {
            return Err(Error::Corrupt("prédécesseur ambigu"));
        }
    }
    let parent = candidat.ok_or(Error::Corrupt("prédécesseur absent"))?;
    let slot = 1 - survivant;
    let ancre = Anchor {
        ids: IDS,
        blocks: d.blocks(),
        manifest: parent,
    };
    d.control().replace_stable(slot, ancre.encode(slot)?)?;
    Engine::mount_current(d, IDS, Mode::ReadOnly)
}

#[test]
fn meme_image_deux_historiques_et_limites_du_recu() -> Result<()> {
    for secteur in [512, 4096] {
        for decaler in [false, true] {
            let cas = Cas::neuf(secteur, false, decaler)?;
            let (mut fs, ctl) = cas.ouvrir()?;
            let g = fs.receipt();
            ctl.trace(None, None);
            remplacer(&mut fs, cas.source, cas.destination)?;
            let suivant = fs.receipt(); // Succès effectivement rendu dans ce second historique.
            let slot = suivant.birth % 2;
            let trace = ctl.events();
            let ancien_slot = cas.image.device().read(slot)?;
            let publiee = ctl.snapshot();
            cas.verifier(
                &Engine::mount_current(publiee.device(), IDS, Mode::ReadOnly)?,
                true,
            )?;
            let torn = cas.interrompre(
                index(&trace, Event::FlushSubmitted(2)),
                Persistence::Prefix {
                    block: slot,
                    bytes: 96,
                },
            )?;
            // Altération indépendante APRÈS acquittement : une ancienne référence
            // revient à son emplacement, sans toucher au checksum de l'enveloppe.
            let d = publiee.device();
            let mut endommagee = d.read(slot)?;
            endommagee[136..184].copy_from_slice(&ancien_slot[136..184]);
            d.control().replace_stable(slot, endommagee)?;
            let octets = torn.bytes()?;
            assert!(d.control().snapshot().bytes()? == octets);
            // La nouvelle arborescence existe même AVANT la soumission de l'ancre.
            let orpheline =
                cas.interrompre(index(&trace, Event::BeforeWrite(slot)), Persistence::None)?;
            let orphelin = orpheline.device();
            Manifest::decode(&orphelin.read(suivant.block)?, IDS, suivant)?;
            for b in 0..d.blocks() {
                if b != slot {
                    assert_eq!(orphelin.read(b)?, d.read(b)?);
                }
            }
            cas.verifier(
                &Engine::mount_current(orphelin, IDS, Mode::ReadOnly)?,
                false,
            )?;
            // Une seule cause connue par l'observateur, deux diagnostics identiques sur disque.
            verifier_image(&cas, &torn, slot, &endommagee, None, g)?;
            let extrait = paire_sur_copie(&torn, 1 - slot, g)?;
            assert_eq!(extrait.receipt(), g);
            cas.verifier(&extrait, false)?;
            assert!(matches!(
                paire_sur_copie(&torn, 1 - slot, suivant),
                Err(Error::Corrupt(_))
            ));
            // Le digest neuf reste entier dans le préfixe de 96 octets.
            // Même une reconstruction exacte de l'enveloppe ne date pas F2 :
            // le même candidat monte sur la branche qui n'a jamais écrit l'ancre.
            let complete = Anchor {
                ids: IDS,
                blocks: d.blocks(),
                manifest: suivant,
            }
            .encode(slot)?;
            assert_eq!(&complete[64..96], &endommagee[64..96]);
            for image in [&torn, &orpheline] {
                let copie = image.device();
                copie.control().replace_stable(slot, complete)?;
                let relu = Engine::mount_current(copie, IDS, Mode::ReadOnly)?;
                assert_eq!(relu.receipt(), suivant);
                cas.verifier(&relu, true)?;
            }
            assert!(torn.bytes()? == octets);

            // Une tête lisible avec un payload endommagé ne constitue pas un secours.
            let mut page = None;
            data_pages(&extrait, |b| {
                if d.read(b)?.starts_with(ANCIEN) {
                    page = Some(b);
                }
                Ok(())
            })?;
            let b = page.expect("page de la destination précédente");
            let mut bloc = d.read(b)?;
            bloc[0] ^= 1;
            d.control().replace_stable(b, bloc)?;
            let ctl = d.control();
            ctl.trace(None, None);
            let rapport = check_image(&d, IDS);
            assert!(rapport.slots.iter().all(|s| s.issue.is_some()));
            assert!(paire_sur_copie(&ctl.snapshot(), 1 - slot, g).is_err());
            assert!(ctl.events().is_empty());
            println!(
                "HISTOIRES secteur={secteur} slot={slot} image_identique={} reçus={}/{} parent_vérifié=true payload_altéré=refus",
                octets.len(),
                g.birth,
                suivant.birth
            );
        }
    }
    Ok(())
}
