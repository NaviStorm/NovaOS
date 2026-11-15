//! Exerce le backend de l’ELF NovaFS sur les deux géométries, sans court-circuit P9.
extern crate alloc;
#[path = "../user/novafs/files/config_store.rs"]
mod store;
use novafs::{
    Device, Engine, Error, Ids, Kind, Mode, Result, VolumePolicy,
    device::{Event, FileImage, Persistence, SimDevice, SimFault},
    time::Atime,
};
const IDS: Ids = Ids {
    fs: [76; 16],
    lv: [43; 16],
};
fn value<D: Device>(f: &Engine<D>, previous: bool) -> Result<(u64, Vec<u8>)> {
    Ok(store::load(f, previous)?.map_or((0, Vec::new()), |r| (r.generation, r.data)))
}
fn cases(sector: u32) -> Result<()> {
    let mut f = Engine::format_p9(FileImage::create(512, sector)?, IDS, Atime::No, None)?;
    let receipt = f.receipt();
    assert_eq!(value(&f, false)?, (0, Vec::new()));
    assert_eq!(receipt, f.receipt());
    assert_eq!(store::save(&mut f, 0, b"v1")?, 1);
    let receipt = f.receipt();
    assert!(matches!(
        store::save(&mut f, 0, b"rejeu"),
        Err(Error::Invalid(store::CONFLICT))
    ));
    assert!(store::save(&mut f, 1, &vec![0; store::LIMIT + 1]).is_err());
    assert_eq!(receipt, f.receipt());
    assert_eq!(store::save(&mut f, 1, b"v2")?, 2);
    let mut f = Engine::mount_current(f.unmount()?, IDS, Mode::ReadWrite)?;
    assert_eq!(value(&f, false)?, (2, b"v2".to_vec()));
    assert_eq!(value(&f, true)?, (1, b"v1".to_vec()));
    // Préparation partielle non publiée ignorée au remontage.
    let root = f.root_id();
    let stage = f.create_at(root, "\x1fNovaOS-services-new", Kind::File)?;
    f.prepare_write_id(stage.id, 0, b"incomplet")?.commit()?;
    let mut f = Engine::mount_current(f.unmount()?, IDS, Mode::ReadWrite)?;
    assert_eq!(value(&f, false)?, (2, b"v2".to_vec()));
    assert!(store::hidden("\x1fNovaOS-services-new"));
    // Un seul slot corrompu : seul le chemin explicite de récupération peut lire.
    let latest = f.lookup_at(root, "\x1fNovaOS-services-0")?;
    f.prepare_write_id(latest.id, 64, b"XY")?.commit()?;
    let r = f.receipt();
    assert!(matches!(value(&f, false), Err(Error::Corrupt(_))));
    assert!(store::save(&mut f, 2, b"danger").is_err());
    assert_eq!(value(&f, true)?, (1, b"v1".to_vec()));
    assert_eq!(r, f.receipt());
    let mut f = Engine::format_p9(FileImage::create(256, sector)?, IDS, Atime::No, None)?;
    store::save(&mut f, 0, b"conserver")?;
    let a = f.accounting()?;
    f.set_volume_policy(VolumePolicy {
        maintenance_blocks: a.available_blocks + a.maintenance_reserved,
        logical_quota_bytes: None,
    })?;
    assert!(store::save(&mut f, 1, b"refus").is_err());
    assert_eq!(value(&f, false)?, (1, b"conserver".to_vec()));
    let mut p8 = Engine::format(FileImage::create(256, sector)?, IDS)?;
    let r = p8.receipt();
    assert!(store::save(&mut p8, 0, b"refus").is_err());
    assert!(value(&p8, false).is_err());
    assert_eq!(r, p8.receipt());
    println!(
        "secteur={sector} absence, remontage, CAS, préparation, corruption, reprise en lecture seule, saturation et refus P8 vérifiés"
    );
    Ok(())
}
// Simule la perte de la VM après création durable d’un objet sans enveloppe.
fn empty_record(sector: u32) -> Result<()> {
    for published in [false, true] {
        for prior in [false, true] {
            let dev = SimDevice::new(384, sector)?;
            let ctl = dev.control();
            let mut fs = Engine::format_p9(dev, IDS, Atime::No, None)?;
            if prior {
                store::save(&mut fs, 0, b"confirmee")?;
            }
            let name = match (published, prior) {
                (false, _) => "\x1fNovaOS-services-new",
                (true, true) => "\x1fNovaOS-services-0",
                (true, false) => "\x1fNovaOS-services-1",
            };
            fs.create_at(fs.root_id(), name, Kind::File)?;
            drop(fs);
            ctl.power_cut(Persistence::None)?;
            let mut fs = Engine::mount_current(ctl.snapshot().device(), IDS, Mode::ReadWrite)?;
            assert_eq!(fs.lookup_at(fs.root_id(), name)?.size, 0);
            let receipt = fs.receipt();
            if published {
                assert!(matches!(store::load(&fs, false), Err(Error::Corrupt(_))));
                assert!(matches!(
                    store::save(&mut fs, u64::from(prior), b"rejeter"),
                    Err(Error::Corrupt(_))
                ));
                if prior {
                    assert_eq!(value(&fs, true)?, (1, b"confirmee".to_vec()));
                } else {
                    assert!(matches!(store::load(&fs, true), Err(Error::Corrupt(_))));
                }
            } else if prior {
                assert_eq!(value(&fs, false)?, (1, b"confirmee".to_vec()));
            } else {
                assert!(store::load(&fs, false)?.is_none());
            }
            assert_eq!(receipt, fs.receipt());
        }
    }
    println!(
        "secteur={sector} coupure après création d’un objet vide : 2 préparations ignorées, 2 slots publiés illisibles refusés, repli explicite si témoin valide, aucune écriture"
    );
    Ok(())
}
fn barriers(sector: u32, base_generation: u64) -> Result<()> {
    let device = SimDevice::new(512, sector)?;
    let ctl = device.control();
    let mut f = Engine::format_p9(device, IDS, Atime::No, None)?;
    for g in 0..base_generation {
        store::save(&mut f, g, &[g as u8 + 1])?;
    }
    drop(f);
    let baseline = ctl.snapshot();
    let dev = baseline.device();
    let c = dev.control();
    let mut f = Engine::mount_current(dev, IDS, Mode::ReadWrite)?;
    c.trace(None, None);
    let new = vec![0xa5; store::LIMIT];
    store::save(&mut f, base_generation, &new)?;
    let events = c.events();
    let stops: Vec<_> = events
        .iter()
        .enumerate()
        .filter_map(|(i, e)| matches!(e, Event::FlushCompleted(_)).then_some(i))
        .collect();
    assert!(stops.len() >= 2);
    let expected_old = if base_generation == 0 {
        vec![]
    } else {
        vec![base_generation as u8]
    };
    for i in &stops {
        let dev = baseline.device();
        let c = dev.control();
        let mut f = Engine::mount_current(dev, IDS, Mode::ReadWrite)?;
        c.trace(Some(*i), None);
        let _ = store::save(&mut f, base_generation, &new);
        drop(f);
        c.power_cut(Persistence::None)?;
        let f = Engine::mount_current(c.snapshot().device(), IDS, Mode::ReadOnly)?;
        let (g, d) = value(&f, false)?;
        if g == base_generation {
            assert_eq!(d, expected_old);
        } else {
            assert_eq!(g, base_generation + 1);
            assert_eq!(d, new);
        }
    }
    for ordinal in 1..=stops.len() {
        for persistent in [0, 1] {
            let dev = baseline.device();
            let c = dev.control();
            let mut f = Engine::mount_current(dev, IDS, Mode::ReadWrite)?;
            c.trace(None, None);
            c.inject(SimFault::Flush {
                ordinal: ordinal.try_into().unwrap(),
                persist_newest: persistent,
            })?;
            let e = store::save(&mut f, base_generation, &new).unwrap_err();
            let unknown = matches!(e, Error::Publication { .. } | Error::UnknownOutcome);
            assert_eq!(unknown, ordinal == stops.len(), "{e}");
            drop(f);
            c.power_cut(Persistence::None)?;
            let f = Engine::mount_current(c.snapshot().device(), IDS, Mode::ReadOnly)?;
            if unknown && persistent == 1 {
                assert_eq!(value(&f, false)?, (base_generation + 1, new.clone()));
            } else {
                assert_eq!(value(&f, false)?, (base_generation, expected_old.clone()));
            }
        }
    }
    println!(
        "secteur={sector} génération initiale={base_generation} coupures FLUSH={} erreurs FLUSH={} dernière barrière UNCERTAIN, validation intégrale au remontage",
        stops.len(),
        2 * stops.len()
    );
    Ok(())
}
// Interdire que la panne de lecture du slot publié soit interprétée comme
// une absence de configuration ou qu’elle autorise une nouvelle publication.
fn read_fault(sector: u32) -> Result<()> {
    let dev = SimDevice::new(384, sector)?;
    let ctl = dev.control();
    let mut f = Engine::format_p9(dev, IDS, Atime::No, None)?;
    store::save(&mut f, 0, b"temoin-configuration-123456")?;
    drop(f);
    let snap = ctl.snapshot();
    let raw = snap.bytes()?;
    let mut reached = 0;
    for (block, page) in raw.chunks(4096).enumerate() {
        if !page.windows(8).any(|w| w == b"NSVCCFG1") {
            continue;
        }
        let dev = snap.device();
        let c = dev.control();
        let f = Engine::mount_current(dev, IDS, Mode::ReadOnly)?;
        c.inject(SimFault::Read {
            block: block as u64,
        })?;
        let r = store::load(&f, false);
        if let Err(Error::Io(_)) = r {
            reached += 1;
        } else {
            assert!(r?.is_some());
        }
    }
    assert!(
        reached > 0,
        "l’erreur injectée doit atteindre la lecture de la configuration"
    );
    println!("secteur={sector} erreurs de lecture effectivement propagées={reached}");
    Ok(())
}
fn policy_bytes(entries: u64, private_tmp: bool) -> Vec<u8> {
    // Ce banc isole l'enveloppe et les décisions de stockage. Le gestionnaire
    // et son codec qualifient séparément le contenu réel des déclarations.
    let mut data = b"NVSVCC01".to_vec();
    data.extend_from_slice(&entries.to_le_bytes());
    data.extend_from_slice(&[9, u8::from(private_tmp)]);
    data
}
fn authority_cases(sector: u32) -> Result<()> {
    const CURRENT: &str = "\x1fNovaOS-services-1";
    const PREVIOUS: &str = "\x1fNovaOS-services-0";
    const MARKER: &str = "\x1fNovaOS-services-enrolled";
    let dev = SimDevice::new(512, sector)?;
    let control = dev.control();
    let mut fs = Engine::format_p9(dev, IDS, Atime::No, None)?;
    let old = policy_bytes(4, true);
    let restricted = policy_bytes(5, false);
    for g in 0..12 {
        assert_eq!(store::save(&mut fs, g, &old)?, g + 1);
    }
    assert_eq!(value(&fs, false)?, (12, old.clone()));
    assert!(matches!(
        fs.lookup_at(fs.root_id(), store::AUTHORITY),
        Err(Error::NotFound)
    ));
    let generation12 = control.snapshot();
    assert_eq!(store::save(&mut fs, 12, &restricted)?, 13);
    drop(fs);
    control.power_cut(Persistence::None)?;
    let baseline = control.snapshot();
    let mut recovered = Engine::mount_current(baseline.device(), IDS, Mode::ReadWrite)?;
    assert_eq!(value(&recovered, false)?, (13, restricted.clone()));
    assert!(value(&recovered, true).is_err());
    assert!(store::save(&mut recovered, 13, &old).is_err());
    assert!(matches!(
        store::save(&mut recovered, 12, &restricted),
        Err(Error::Invalid(store::CONFLICT))
    ));
    assert!(store::hidden(store::AUTHORITY) && store::hidden(MARKER));
    for name in [CURRENT, PREVIOUS, store::AUTHORITY, MARKER] {
        for damage in ["contenu", "suppression"] {
            let mut fs = Engine::mount_current(baseline.device(), IDS, Mode::ReadWrite)?;
            let id = fs.lookup_at(fs.root_id(), name)?.id;
            if damage == "contenu" {
                fs.prepare_write_id(id, 0, b"X")?.commit()?;
            } else {
                fs.unlink_at(fs.root_id(), name, Kind::File)?;
            }
            let dev = fs.unmount()?;
            let mut fs = Engine::mount_current(dev, IDS, Mode::ReadWrite)?;
            let receipt = fs.receipt();
            assert!(
                matches!(value(&fs, false), Err(Error::Corrupt(_))),
                "{name}/{damage}"
            );
            assert!(
                matches!(value(&fs, true), Err(Error::Corrupt(_))),
                "{name}/{damage}"
            );
            assert!(store::save(&mut fs, 12, &old).is_err());
            assert_eq!(receipt, fs.receipt());
        }
    }
    // Réintroduire l'enveloppe 11 réellement écrite, CRC/P9 corrects; ainsi
    // que 13/permissive avec CRC recalculé. La référence doit refuser les deux.
    let history = Engine::mount_current(generation12.device(), IDS, Mode::ReadOnly)?;
    let old_slot = history.lookup_at(history.root_id(), CURRENT)?;
    let old_valid = history.inspect_id(old_slot.id, 0, old_slot.size as usize)?;
    let id = recovered.lookup_at(recovered.root_id(), CURRENT)?.id;
    let mut forged = recovered.inspect_id(id, 0, 64 + store::LIMIT)?;
    forged[64 + restricted.len() - 1] = 1;
    let crc = store::crc(&forged[..52]) ^ store::crc(&forged[64..]);
    forged[52..56].copy_from_slice(&crc.to_le_bytes());
    for replacement in [old_valid, forged] {
        let mut fs = Engine::mount_current(baseline.device(), IDS, Mode::ReadWrite)?;
        fs.prepare_write_id(id, 0, &replacement)?.commit()?;
        let fs = Engine::mount_current(fs.unmount()?, IDS, Mode::ReadOnly)?;
        assert!(matches!(value(&fs, false), Err(Error::Corrupt(_))));
        assert!(matches!(value(&fs, true), Err(Error::Corrupt(_))));
    }
    // La perte de l'acquittement et l'interruption entre commits sont distinctes.
    // Chaque barrière du premier enrôlement et d'une modification postérieure
    // est interrompue puis le volume remonté par une instance neuve du moteur.
    for (snap, current, next_data) in [
        (generation12, 12, restricted.clone()),
        (baseline, 13, policy_bytes(5, true)),
    ] {
        let d = snap.device();
        let ctl = d.control();
        let mut f = Engine::mount_current(d, IDS, Mode::ReadWrite)?;
        let current_data = value(&f, false)?.1;
        ctl.trace(None, None);
        assert_eq!(store::save(&mut f, current, &next_data)?, current + 1);
        let positions: Vec<_> = ctl
            .events()
            .into_iter()
            .enumerate()
            .filter_map(|(i, e)| matches!(e, Event::FlushCompleted(_)).then_some(i))
            .collect();
        let mut counts = [0; 3];
        for i in &positions {
            let d = snap.device();
            let ctl = d.control();
            let mut f = Engine::mount_current(d, IDS, Mode::ReadWrite)?;
            ctl.trace(Some(*i), None);
            let result = store::save(&mut f, current, &next_data);
            drop(f);
            ctl.power_cut(Persistence::None)?;
            let f = Engine::mount_current(ctl.snapshot().device(), IDS, Mode::ReadOnly)?;
            match value(&f, false) {
                Ok((g, bytes)) if g == current + 1 => {
                    assert_eq!(bytes, next_data);
                    counts[0] += 1;
                }
                Ok((g, bytes)) if result.is_err() && g == current => {
                    assert_eq!(bytes, current_data);
                    counts[1] += 1;
                }
                Err(Error::Corrupt(_)) if result.is_err() => {
                    assert!(matches!(value(&f, true), Err(Error::Corrupt(_))));
                    counts[2] += 1;
                }
                other => panic!("état après coupure {other:?}"),
            }
        }
        for ordinal in 1..=positions.len() {
            for persist_newest in [0,1] {
                let d = snap.device();
                let ctl = d.control();
                let mut f = Engine::mount_current(d, IDS, Mode::ReadWrite)?;
                ctl.trace(None, None);
                ctl.inject(SimFault::Flush { ordinal: ordinal.try_into().unwrap(), persist_newest })?;
                let result = store::save(&mut f, current, &next_data).unwrap_err();
                let uncertain = matches!(result, Error::UnknownOutcome | Error::Publication { .. });
                assert_eq!(uncertain, ordinal == positions.len(), "{result}");
                drop(f);
                ctl.power_cut(Persistence::None)?;
                let f = Engine::mount_current(ctl.snapshot().device(), IDS, Mode::ReadOnly)?;
                if uncertain && persist_newest == 1 {
                    assert_eq!(value(&f, false)?, (current + 1, next_data.clone()));
                } else {
                    match value(&f, false) {
                        Ok((g, bytes)) => {
                            assert_eq!(g, current);
                            assert_eq!(bytes, current_data);
                        }
                        Err(Error::Corrupt(_)) => assert!(matches!(value(&f,true), Err(Error::Corrupt(_)))),
                        other => panic!("état après échec FLUSH {other:?}"),
                    }
                }
            }
        }
        assert!(counts.iter().all(|n| *n > 0), "{counts:?}");
        println!(
            "secteur={sector} autorité génération={current} barrières={} nouvelle={} ancienne={} refus_sûr={} erreurs_FLUSH={}",
            positions.len(),
            counts[0],
            counts[1],
            counts[2], positions.len()*2
        );
    }
    println!(
        "secteur={sector} génération 13 acquittée : quatre objets corrompus ou supprimés, deux substitutions valides, PREVIOUS et CAS refusés après remontage"
    );
    Ok(())
}

fn main() -> Result<()> {
    for sector in [512, 4096] {
        cases(sector)?;
        empty_record(sector)?;
        read_fault(sector)?;
        authority_cases(sector)?;
        for g in [0, 1, 2] {
            barriers(sector, g)?;
        }
    }
    Ok(())
}
