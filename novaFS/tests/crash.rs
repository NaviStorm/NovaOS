//! Coupures du périphérique et reçus gérés par l'observateur extérieur.
use novafs::device::{Event, Persistence, SimDevice, SimSnapshot};
use novafs::{Device, Engine, Error, Ids, Mode, Ref, Result, View};

const LAB_POOL: [u8; 16] = [0x31; 16];
const IDS: Ids = Ids {
    fs: [0xfa; 16],
    lv: [0xa1; 16],
};
type Fs = Engine<SimDevice>;

fn setup(sector: u32) -> Result<Fs> {
    let mut fs = Engine::format(SimDevice::new(640, sector)?, IDS)?;
    for (name, msg) in [
        ("/A", b"preuve du client A".as_slice()),
        ("/B", b"preuve du client B"),
    ] {
        fs.create(name)?;
        fs.write(name, 0, msg)?;
    }
    fs.create("/scratch")?;
    Ok(fs)
}

fn protected_files(fs: &Fs) -> Result<()> {
    assert_eq!(fs.read("/A", 0, 64)?, b"preuve du client A");
    assert_eq!(fs.read("/B", 0, 64)?, b"preuve du client B");
    Ok(())
}

fn recover(snapshot: &SimSnapshot) -> Result<Fs> {
    Engine::mount(snapshot.device(), IDS, Mode::ReadOnly)
}
fn next_policy(snapshot: &SimSnapshot) -> Result<Fs> {
    Engine::mount(snapshot.device(), IDS, Mode::ReadWrite)
}

fn directory_state(fs: &Fs) -> Result<Vec<(String, Vec<u8>)>> {
    let names = fs.list("/", None, 64)?;
    assert!(
        names.len() < 64,
        "la fixture de coupure doit tenir dans une page de noms"
    );
    names
        .into_iter()
        .map(|name| {
            let data = fs.read(&format!("/{name}"), 0, 8192)?;
            Ok((name, data))
        })
        .collect()
}

fn exercise(label: &str, base: SimSnapshot, op: impl Fn(&mut Fs) -> Result<()>) -> Result<()> {
    let mut normal = next_policy(&base)?;
    let before = normal.receipt();
    let old_state = directory_state(&normal)?;
    let raw = normal.into_device();
    let ctl = raw.control();
    normal = Engine::mount(raw, IDS, Mode::ReadWrite)?;
    ctl.trace(None, None);
    op(&mut normal)?;
    let after = normal.receipt();
    let new_state = directory_state(&normal)?;
    let events = ctl.events();
    assert_eq!(after.birth, before.birth + 1);
    let f1 = events
        .iter()
        .position(|&e| e == Event::FlushCompleted(1))
        .unwrap();
    let f2 = events
        .iter()
        .position(|&e| e == Event::FlushEffect(2))
        .unwrap();
    let anchor = events
        .iter()
        .position(|&e| e == Event::WriteSubmitted(after.birth % 2))
        .unwrap();
    assert!(f1 < anchor && anchor < f2);
    assert_eq!(events.last(), Some(&Event::FlushCompleted(2)));
    let mut checks = 0;
    for i in 0..events.len() {
        for persistence in [
            Persistence::None,
            Persistence::Newest(1),
            Persistence::Newest(usize::MAX),
        ] {
            let d = base.device();
            let control = d.control();
            let mut engine = Engine::mount(d, IDS, Mode::ReadWrite)?;
            control.trace(Some(i), None);
            assert!(op(&mut engine).is_err(), "{label} n'a pas rencontré {i}");
            // Le contrôleur d'injection n'est pas un reçu d'application.
            drop(engine);
            control.power_cut(persistence)?;
            let fs = recover(&control.snapshot())?;
            fs.verify()?;
            protected_files(&fs)?;
            if i < anchor {
                assert_eq!(fs.receipt(), before, "{label}/{i}");
            } else if i >= f2 {
                assert_eq!(fs.receipt(), after, "{label}/{i} : F2 perdue");
            } else {
                assert!(
                    [before, after].contains(&fs.receipt()),
                    "{label}/{i} : état mixte"
                );
            }
            assert_eq!(
                directory_state(&fs)?.as_slice(),
                if fs.receipt() == before {
                    &old_state[..]
                } else {
                    &new_state[..]
                }
            );
            checks += 1;
        }
    }
    // Le succès API observé acquitte la racine et les deux témoins applicatifs.
    ctl.power_cut(Persistence::None)?;
    let new = recover(&ctl.snapshot())?;
    assert_eq!(new.receipt(), after);
    new.verify()?;
    println!(
        "N02 {label}: {} frontières, {checks} coupures et confirmation",
        events.len()
    );
    Ok(())
}

#[test]
fn n02_all_write_and_flush_boundaries() -> Result<()> {
    let mut fs = setup(512)?;
    fs.write("/scratch", 0, &[3; 8192])?;
    let base = fs.into_device().control().snapshot();
    exercise("deux blocs", base.clone(), |f| {
        f.write("/scratch", 0, &[11; 8192])
    })?;
    exercise("rename", base.clone(), |f| f.rename("/scratch", "/moved"))?;
    exercise("unlink", base.clone(), |f| f.unlink("/scratch"))?;
    let mut fs = next_policy(&base)?;
    fs.unlink("/scratch")?;
    exercise("collecte", fs.into_device().control().snapshot(), |f| {
        assert!(f.collect()?);
        Ok(())
    })?;
    let mut fs = next_policy(&base)?;
    let mut written = std::collections::BTreeSet::new();
    let mut reuse = false;
    for tag in 31u8..63 {
        let d = fs.into_device();
        let ctl = d.control();
        let old = ctl.snapshot();
        fs = Engine::mount(d, IDS, Mode::ReadWrite)?;
        ctl.trace(None, None);
        fs.write("/scratch", 0, &[tag; 8192])?;
        let blocks: Vec<_> = ctl
            .events()
            .into_iter()
            .filter_map(|e| match e {
                Event::WriteCompleted(b) if b >= 2 => Some(b),
                _ => None,
            })
            .collect();
        if blocks.iter().any(|b| written.contains(b)) {
            exercise("réutilisation de bloc", old, |f| {
                f.write("/scratch", 0, &[tag; 8192])
            })?;
            reuse = true;
            break;
        }
        written.extend(blocks);
    }
    assert!(reuse, "réutilisation jamais atteinte");
    // Déclencher et observer la scission effective de la racine des noms.
    let mut fs = next_policy(&base)?;
    let mut split = false;
    for i in 0..100 {
        let d = fs.into_device();
        let ctl = d.control();
        let old = ctl.snapshot();
        fs = Engine::mount(d, IDS, Mode::ReadWrite)?;
        let name = format!("/{i:03}{}", "a".repeat(183));
        let names = fs.manifest().roots[1];
        let old_level = old.device().read(names.block)?[98];
        fs.create(&name)?;
        let new_level = ctl.snapshot().device().read(fs.manifest().roots[1].block)?[98];
        if new_level > old_level {
            exercise("scission de B+Tree", old, |f| f.create(&name))?;
            split = true;
            break;
        }
    }
    assert!(split, "scission jamais atteinte");
    Ok(())
}

#[test]
fn n03_flush_order_sector_tears_and_restart() -> Result<()> {
    for sector in [512, 4096] {
        let fs = setup(sector)?;
        let old = fs.receipt();
        let base = fs.into_device().control().snapshot();
        let d = base.device();
        let ctl = d.control();
        let mut fs = Engine::mount(d, IDS, Mode::ReadWrite)?;
        ctl.trace(None, None);
        fs.write("/scratch", 0, &[44; 8192])?;
        let new = fs.receipt();
        let slot = new.birth % 2;
        let events = ctl.events();
        let before_f2 = events
            .iter()
            .position(|e| *e == Event::FlushSubmitted(2))
            .unwrap();
        let old_slot = base.device().read(slot)?;
        let new_slot = ctl.snapshot().device().read(slot)?;
        assert_eq!(old_slot[184..], new_slot[184..]);
        let mut old_cases = 0;
        let mut new_cases = 0;
        let masks = 1u16 << (4096 / sector);
        for mask in 0..masks {
            let d = base.device();
            let h = d.control();
            let mut e = Engine::mount(d, IDS, Mode::ReadWrite)?;
            h.trace(Some(before_f2), None);
            assert!(e.write("/scratch", 0, &[44; 8192]).is_err());
            h.power_cut(Persistence::Sectors {
                block: slot,
                mask: mask as u8,
            })?;
            let e = recover(&h.snapshot())?;
            if mask & 1 == 0 {
                assert_eq!(e.receipt(), old);
                old_cases += 1;
            } else {
                assert_eq!(e.receipt(), new);
                new_cases += 1;
            }
            e.verify()?;
        }
        println!("N03 secteur={sector}, {old_cases} anciennes + {new_cases} nouvelles ancres");
        let d = base.device();
        let h = d.control();
        let mut fs = Engine::mount(d, IDS, Mode::ReadWrite)?;
        h.trace(Some(before_f2), None);
        assert!(fs.write("/scratch", 0, &[44; 8192]).is_err());
        h.power_cut(Persistence::Prefix {
            block: slot,
            bytes: 96,
        })?;
        let torn = h.snapshot();
        assert!(recover(&torn).is_err());
        let salvage_device = torn.device();
        let observer = salvage_device.control();
        observer.trace(None, None);
        let mut salvage = Engine::salvage(salvage_device, IDS, 1 - slot)?;
        assert_eq!(salvage.receipt(), old);
        protected_files(&salvage)?;
        assert!(matches!(salvage.unlink("/A"), Err(Error::ReadOnly)));
        assert!(matches!(salvage.write("/A", 0, b"x"), Err(Error::ReadOnly)));
        assert!(matches!(salvage.create("/secours"), Err(Error::ReadOnly)));
        assert!(matches!(salvage.mkdir("/d"), Err(Error::ReadOnly)));
        assert!(matches!(salvage.rename("/A", "/AA"), Err(Error::ReadOnly)));
        salvage.verify()?;
        salvage.unmount()?;
        assert!(
            observer.events().is_empty(),
            "le secours a soumis une écriture ou un flush"
        );
        assert_eq!(observer.snapshot().bytes()?, torn.bytes()?);
        assert!(Engine::salvage(torn.device(), IDS, slot).is_err());
        // Support hors modèle : une faute physique couvre les deux ancres.
        let collateral = torn.device();
        let both = collateral.control();
        both.replace_stable(1 - slot, [0xbd; 4096])?;
        let destroyed = both.snapshot();
        assert!(recover(&destroyed).is_err());
        for s in [0, 1] {
            assert!(Engine::salvage(destroyed.device(), IDS, s).is_err());
        }
        // Arrêt du service seul et deuxième coupure lors de la stabilisation
        // du montage. Aucun écrivain si l'issue de ce flush est incertaine.
        for (point, should_survive) in [(0, false), (1, true), (2, true)] {
            let d = base.device();
            let h = d.control();
            let mut run = Engine::mount(d, IDS, Mode::ReadWrite)?;
            h.trace(Some(before_f2), None);
            assert!(run.write("/scratch", 0, &[44; 8192]).is_err());
            let d = run.into_device();
            h.restart_service();
            h.trace(Some(point), None);
            assert!(Engine::mount(d, IDS, Mode::ReadWrite).is_err());
            h.power_cut(Persistence::None)?;
            assert_eq!(
                recover(&h.snapshot())?.receipt(),
                if should_survive { new } else { old }
            );
        }
        let d = base.device();
        let h = d.control();
        let mut run = Engine::mount(d, IDS, Mode::ReadWrite)?;
        h.trace(Some(before_f2), None);
        assert!(run.write("/scratch", 0, &[44; 8192]).is_err());
        let d = run.into_device();
        h.restart_service();
        h.trace(None, None);
        let reopened = Engine::mount(d, IDS, Mode::ReadWrite)?;
        assert_eq!(reopened.receipt(), new);
        assert_eq!(
            h.events(),
            vec![
                Event::FlushSubmitted(1),
                Event::FlushEffect(1),
                Event::FlushCompleted(1)
            ]
        );
        h.power_cut(Persistence::None)?;
        assert_eq!(recover(&h.snapshot())?.receipt(), new);
        // Témoin négatif F1 : seule l'ancre atteint le support. L'erreur de
        // montage attendue traduit une dépendance absente sur un support atomique.
        let d = base.device();
        let h = d.control();
        let mut run = Engine::mount(d, IDS, Mode::ReadWrite)?;
        h.trace(Some(before_f2), Some(1));
        assert!(run.write("/scratch", 0, &[44; 8192]).is_err());
        h.power_cut(Persistence::Newest(1))?;
        assert!(recover(&h.snapshot()).is_err());
        // Témoin négatif F2 : le moteur a rendu un vrai succès en faisant
        // confiance à un périphérique menteur. L'oracle observe le recul.
        let d = base.device();
        let h = d.control();
        let mut run = Engine::mount(d, IDS, Mode::ReadWrite)?;
        h.trace(None, Some(2));
        run.write("/scratch", 0, &[44; 8192])?;
        let ack = run.receipt();
        h.power_cut(Persistence::None)?;
        let observed = recover(&h.snapshot())?;
        assert_eq!(observed.receipt(), old);
        assert_ne!(observed.receipt(), ack);
    }
    Ok(())
}

#[derive(Clone, Copy)]
struct Witness {
    pool: [u8; 16],
    ids: Ids,
    head: Ref,
}
#[derive(Debug, PartialEq, Eq)]
enum Verdict {
    Current,
    PreviousLinked,
    StaleOrFork,
    Unknown,
    WrongLineage,
}
fn compare(pool: [u8; 16], fs: &Fs, witness: Option<Witness>) -> Verdict {
    let Some(w) = witness else {
        return Verdict::Unknown;
    };
    if w.pool != pool || w.ids != fs.ids() {
        return Verdict::WrongLineage;
    }
    let h = fs.receipt();
    if h == w.head {
        return Verdict::Current;
    }
    if h.birth <= w.head.birth {
        return Verdict::StaleOrFork;
    }
    if fs.manifest().previous == w.head.birth && fs.manifest().previous_hash == w.head.hash {
        return Verdict::PreviousLinked;
    }
    Verdict::Unknown
}

#[test]
fn n05_external_witness_detects_unreachable_durable_state() -> Result<()> {
    let fs = setup(512)?;
    let old = fs.receipt();
    let d = fs.into_device();
    let control = d.control();
    let earlier = control.snapshot();
    let mut fs = Engine::mount(d, IDS, Mode::ReadWrite)?;
    fs.write("/scratch", 0, &[0x95; 4096])?;
    let latest = Witness {
        pool: LAB_POOL,
        ids: IDS,
        head: fs.receipt(),
    };
    let intact = control.snapshot();
    assert_eq!(compare(LAB_POOL, &fs, Some(latest)), Verdict::Current);
    assert_eq!(
        compare(
            LAB_POOL,
            &fs,
            Some(Witness {
                pool: LAB_POOL,
                ids: IDS,
                head: old
            })
        ),
        Verdict::PreviousLinked
    );
    assert_eq!(compare(LAB_POOL, &fs, None), Verdict::Unknown);
    let old_image = recover(&earlier)?;
    old_image.verify()?;
    assert_eq!(
        compare(LAB_POOL, &old_image, Some(latest)),
        Verdict::StaleOrFork
    );
    let mut wrong = latest;
    wrong.ids.fs[0] ^= 1;
    assert_eq!(compare(LAB_POOL, &fs, Some(wrong)), Verdict::WrongLineage);
    assert_eq!(
        compare([0xb6; 16], &fs, Some(latest)),
        Verdict::WrongLineage
    );
    let mut fork = latest;
    fork.head.hash[1] ^= 1;
    assert_eq!(compare(LAB_POOL, &fs, Some(fork)), Verdict::StaleOrFork);
    fs.create("/later")?;
    assert_eq!(
        compare(
            LAB_POOL,
            &fs,
            Some(Witness {
                pool: LAB_POOL,
                ids: IDS,
                head: old
            })
        ),
        Verdict::Unknown
    );
    let broken = intact.device();
    let ctrl = broken.control();
    let data = (2..broken.blocks())
        .find(|&b| broken.read(b).unwrap() == [0x95; 4096])
        .unwrap();
    ctrl.replace_stable(data, [0; 4096])?;
    match Engine::mount(broken, IDS, Mode::ReadOnly) {
        Ok(e) => {
            assert!(e.read("/scratch", 0, 4096).is_err());
            assert!(e.verify().is_err());
        }
        Err(Error::Corrupt(_)) => {}
        Err(e) => return Err(e),
    }
    // Même observation locale après déchirure sans reçu ou défaut latent
    // après reçu. La seule ancre lisible garde la génération antérieure.
    let mut bad_anchor = intact.device().read(latest.head.birth % 2)?;
    bad_anchor[64] ^= 1;
    for medium in [intact.clone(), earlier] {
        let damaged = medium.device();
        let h = damaged.control();
        h.replace_stable(latest.head.birth % 2, bad_anchor)?;
        let image = h.snapshot();
        assert!(recover(&image).is_err());
        let saved = Engine::salvage(image.device(), IDS, 1 - latest.head.birth % 2)?;
        assert_eq!(saved.receipt(), old);
        assert_eq!(
            compare(LAB_POOL, &saved, Some(latest)),
            Verdict::StaleOrFork
        );
    }
    Ok(())
}
