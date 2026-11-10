//! Oracle applicatif indépendant du code de sérialisation. Les essais utilisent un
//! support neuf pour chaque injection. Les images et les reçus restent séparés.
use novafs::device::{Event, Persistence, SimControl, SimDevice, SimFault, SimSnapshot};
use novafs::{BLOCK_BYTES, Device, Engine, Error, Ids, Mode, Result};

const IDS: Ids = Ids {
    fs: [47; 16],
    lv: [91; 16],
};
const N: usize = BLOCK_BYTES;
const CONFIRMED: u64 = 5;
const MARKER: &[u8] = b"recu anterieur : garder ces octets";
const PATCH: [u8; N] = [0x9d; N];
const OFFSET: usize = N + 512;

struct Oracle {
    generation: u64,
    contents: Vec<u8>,
}

impl Oracle {
    fn initial() -> Self {
        let mut contents = vec![0; 3 * N];
        contents[N..].fill(0x35);
        Self {
            generation: CONFIRMED,
            contents,
        }
    }

    fn apply(&mut self, offset: usize, bytes: &[u8]) {
        self.contents[offset..offset + bytes.len()].copy_from_slice(bytes);
        self.generation += 1;
    }

    fn compare(&self, fs: &Engine<SimDevice>) -> Result<()> {
        fs.verify()?;
        assert_eq!(fs.generation(), self.generation);
        assert_eq!(fs.list("/", None, 64)?, ["journal", "marqueur"]);
        assert_eq!(fs.read("/marqueur", 0, N)?, MARKER);
        assert_eq!(fs.stat("/journal")?.size, self.contents.len() as u64);
        assert_eq!(fs.stat("/journal")?.revision, self.generation);
        assert_eq!(fs.read("/journal", 0, 4 * N)?, self.contents);
        assert!(
            fs.read("/journal", self.contents.len() as u64, N)?
                .is_empty()
        );
        Ok(())
    }
}

struct Trial {
    base: SimSnapshot,
    committed: SimSnapshot,
    events: Vec<Event>,
    old: Oracle,
    new: Oracle,
}

impl Trial {
    fn new(sector: u32) -> Result<Self> {
        let dev = SimDevice::new(384, sector)?;
        let ctl = dev.control();
        let mut fs = Engine::format(dev, IDS)?;
        fs.create("/marqueur")?;
        fs.write("/marqueur", 0, MARKER)?;
        fs.create("/journal")?;
        fs.write("/journal", N as u64, &[0x35; 2 * N])?;
        let old = Oracle::initial();
        old.compare(&fs)?;
        let base = ctl.snapshot();
        ctl.trace(None, None);
        fs.write("/journal", OFFSET as u64, &PATCH)?;
        let events = ctl.events();
        let mut new = Oracle::initial();
        new.apply(OFFSET, &PATCH);
        new.compare(&fs)?;
        Ok(Self {
            base,
            committed: ctl.snapshot(),
            events,
            old,
            new,
        })
    }

    fn mount(&self) -> Result<(Engine<SimDevice>, SimControl)> {
        let dev = self.base.device();
        let ctl = dev.control();
        Ok((Engine::mount(dev, IDS, Mode::ReadWrite)?, ctl))
    }

    fn position(&self, event: Event) -> usize {
        self.events
            .iter()
            .position(|x| *x == event)
            .expect("frontière du protocole absente")
    }

    fn expected(&self, new: bool) -> &Oracle {
        if new { &self.new } else { &self.old }
    }
}

fn check_recovery(stable: &SimSnapshot, oracle: &Oracle) -> Result<()> {
    let ro = Engine::mount(stable.device(), IDS, Mode::ReadOnly)?;
    oracle.compare(&ro)?;
    let dev = ro.unmount()?;
    let ctl = dev.control();
    let writer = Engine::mount(dev, IDS, Mode::ReadWrite)?;
    oracle.compare(&writer)?;
    drop(writer);
    ctl.power_cut(Persistence::None)?;
    oracle.compare(&Engine::mount(
        ctl.snapshot().device(),
        IDS,
        Mode::ReadOnly,
    )?)
}

fn check_error(fs: &mut Engine<SimDevice>, error: Error, phase: Option<&str>) {
    match (phase, error) {
        (Some(expected), Error::Publication { phase, cause }) => {
            assert_eq!(phase, expected);
            assert!(matches!(*cause, Error::Io(_)));
            assert!(matches!(fs.create("/interdit"), Err(Error::UnknownOutcome)));
        }
        (None, Error::Io(_)) => {
            assert!(matches!(fs.create("/interdit"), Err(Error::Unavailable)));
        }
        (phase, error) => panic!("fenêtre {phase:?}, erreur inattendue {error}"),
    }
}

#[test]
fn p1_crash_at_each_protocol_boundary() -> Result<()> {
    for sector in [512, 4096] {
        let t = Trial::new(sector)?;
        let slot = t.new.generation % 2;
        let first_anchor = t.position(Event::BeforeWrite(slot));
        let accepted = t.position(Event::WriteSubmitted(slot));
        let f2 = t.position(Event::FlushSubmitted(2));
        let stable = t.position(Event::FlushEffect(2));
        assert!(t.position(Event::FlushCompleted(1)) < first_anchor);
        assert_eq!(t.events.last(), Some(&Event::FlushCompleted(2)));
        let mut cases = 0;
        for i in 0..t.events.len() {
            for retained in [0, 1, usize::MAX] {
                let (mut fs, ctl) = t.mount()?;
                ctl.trace(Some(i), None);
                let failure = fs.write("/journal", OFFSET as u64, &PATCH).unwrap_err();
                let phase = if i < first_anchor {
                    None
                } else if i < f2 {
                    Some("ancre")
                } else {
                    Some("F2")
                };
                check_error(&mut fs, failure, phase);
                drop(fs);
                ctl.power_cut(Persistence::Newest(retained))?;
                let new = i >= stable || (i >= accepted && retained != 0);
                check_recovery(&ctl.snapshot(), t.expected(new))?;
                cases += 1;
            }
        }
        // Aucun succès rendu au client ci-dessus. Ici il a reçu son acquittement.
        check_recovery(&t.committed, &t.new)?;
        println!(
            "P1 frontières secteur={sector} points={} coupures={cases} confirmation=1",
            t.events.len()
        );
    }
    Ok(())
}

#[test]
fn p1_short_writes_and_partial_flushes() -> Result<()> {
    for sector in [512, 4096] {
        let t = Trial::new(sector)?;
        let slot = t.new.generation % 2;
        let blocks: Vec<u64> = t
            .events
            .iter()
            .filter_map(|e| match e {
                Event::BeforeWrite(block) => Some(*block),
                _ => None,
            })
            .collect();
        assert_eq!(blocks.last(), Some(&slot));
        let old_anchor = t.base.device().read(slot)?;
        let new_anchor = t.committed.device().read(slot)?;
        let mut prefixes = vec![
            0,
            1,
            64,
            96,
            183,
            184,
            sector as usize - 1,
            sector as usize,
            N - 1,
            N,
        ];
        prefixes.sort_unstable();
        prefixes.dedup();
        let mut refused = 0;
        let mut writes = 0;
        for block in &blocks {
            for &prefix in &prefixes {
                let (mut fs, ctl) = t.mount()?;
                ctl.inject(SimFault::Write {
                    block: *block,
                    prefix,
                })?;
                let error = fs.write("/journal", OFFSET as u64, &PATCH).unwrap_err();
                check_error(&mut fs, error, (*block == slot).then_some("ancre"));
                drop(fs);
                ctl.power_cut(Persistence::Newest(usize::MAX))?;
                let image = ctl.snapshot();
                if *block != slot {
                    // Une écriture CoW incomplète peut persister en dehors des deux racines.
                    check_recovery(&image, &t.old)?;
                } else {
                    let mut mixed = old_anchor;
                    mixed[..prefix].copy_from_slice(&new_anchor[..prefix]);
                    assert_eq!(image.device().read(slot)?, mixed);
                    if mixed == old_anchor || mixed == new_anchor {
                        check_recovery(&image, t.expected(mixed == new_anchor))?;
                    } else {
                        for mode in [Mode::ReadOnly, Mode::ReadWrite] {
                            assert!(matches!(
                                Engine::mount(image.device(), IDS, mode),
                                Err(Error::Corrupt(_))
                            ));
                        }
                        let survivor = Engine::salvage(image.device(), IDS, 1 - slot)?;
                        t.old.compare(&survivor)?;
                        refused += 1;
                    }
                }
                writes += 1;
            }
        }
        assert!(refused > 0);
        let mut flushes = 0;
        // F1 peut n'avoir persisté qu'une partie des dépendances. Une fois F1 passé,
        // F2 ne porte plus que l'ancre. Le moteur doit pourtant signaler l'EIO dans les deux cas.
        for (ordinal, dirty) in [(1, blocks.len() - 1), (2, 1)] {
            for retained in 0..=dirty {
                let (mut fs, ctl) = t.mount()?;
                ctl.inject(SimFault::Flush {
                    ordinal,
                    persist_newest: retained,
                })?;
                let error = fs.write("/journal", OFFSET as u64, &PATCH).unwrap_err();
                check_error(&mut fs, error, (ordinal == 2).then_some("F2"));
                drop(fs);
                ctl.power_cut(Persistence::None)?;
                check_recovery(&ctl.snapshot(), t.expected(ordinal == 2 && retained > 0))?;
                flushes += 1;
            }
        }
        println!(
            "P1 EIO secteur={sector} blocs={} écritures={writes} ancres_déchirées={refused} flushs={flushes}",
            blocks.len()
        );
    }
    Ok(())
}

#[test]
fn p1_read_errors_and_uninitialised_blocks() -> Result<()> {
    for sector in [512, 4096] {
        let t = Trial::new(sector)?;
        let image = t.committed.device();
        let data = (2..image.blocks())
            .find(|&b| image.read(b).unwrap().starts_with(MARKER))
            .unwrap();
        let fs = Engine::mount(t.committed.device(), IDS, Mode::ReadOnly)?;
        let metadata = fs.receipt().block;
        for block in [0, 1, metadata] {
            let dev = t.committed.device();
            let ctl = dev.control();
            ctl.inject(SimFault::Read { block })?;
            assert!(matches!(
                Engine::mount(dev, IDS, Mode::ReadWrite),
                Err(Error::Io(_))
            ));
            check_recovery(&ctl.snapshot(), &t.new)?;
        }
        let dev = t.committed.device();
        let ctl = dev.control();
        let mut fs = Engine::mount(dev, IDS, Mode::ReadWrite)?;
        ctl.inject(SimFault::Read { block: data })?;
        let error = fs.read("/marqueur", 0, N).unwrap_err();
        check_error(&mut fs, error, None);
        drop(fs);
        check_recovery(&ctl.snapshot(), &t.new)?;

        for block in [data, metadata, t.new.generation % 2] {
            let dev = t.committed.device();
            let ctl = dev.control();
            ctl.replace_stable(block, [0; N])?;
            if block == data {
                let fs = Engine::mount(dev, IDS, Mode::ReadOnly)?;
                assert!(matches!(fs.read("/marqueur", 0, N), Err(Error::Corrupt(_))));
                let audit = Engine::mount(ctl.snapshot().device(), IDS, Mode::ReadOnly)?;
                assert!(matches!(audit.verify(), Err(Error::Corrupt(_))));
            } else {
                assert!(matches!(
                    Engine::mount(dev, IDS, Mode::ReadWrite),
                    Err(Error::Corrupt(_))
                ));
            }
        }
        println!("P1 lectures secteur={sector} EIO=4 remises_à_zéro=3");
    }
    Ok(())
}

#[test]
fn p1_repeated_restarts_retained_acknowledgements() -> Result<()> {
    for sector in [512, 4096] {
        let t = Trial::new(sector)?;
        let mut image = t.base;
        let mut oracle = t.old;
        let mut acknowledged = 0;
        for i in 0..48 {
            let dev = image.device();
            let ctl = dev.control();
            let mut fs = Engine::mount(dev, IDS, Mode::ReadWrite)?;
            oracle.compare(&fs)?;
            // Chaque mutation a sa propre zone. Une nouvelle opération ne peut masquer la perte d'un reçu.
            let start = i * 39;
            let patch = [i as u8 + 1; 39];
            let mode = i % 4;
            let stable = mode >= 2;
            if mode != 3 {
                let ordinal = if mode == 0 { 1 } else { 2 };
                ctl.inject(SimFault::Flush {
                    ordinal,
                    persist_newest: usize::from(stable),
                })?;
                let failure = fs.write("/journal", start as u64, &patch).unwrap_err();
                check_error(&mut fs, failure, (ordinal == 2).then_some("F2"));
            } else {
                fs.write("/journal", start as u64, &patch)?;
                acknowledged += 1;
            }
            drop(fs);
            ctl.power_cut(Persistence::None)?;
            if stable {
                oracle.apply(start, &patch);
            }
            image = ctl.snapshot();
            check_recovery(&image, &oracle)?;
        }
        println!(
            "P1 redémarrages secteur={sector} tentatives=48 acquittées={acknowledged} génération={}",
            oracle.generation
        );
    }
    Ok(())
}

#[test]
fn p1_initial_format_needs_both_anchors() -> Result<()> {
    for sector in [512, 4096] {
        let dev = SimDevice::new(256, sector)?;
        let ctl = dev.control();
        ctl.trace(None, None);
        let fs = Engine::format(dev, IDS)?;
        assert_eq!(fs.generation(), 1);
        let events = ctl.events();
        let writes: Vec<_> = events
            .iter()
            .filter_map(|e| match e {
                Event::WriteSubmitted(slot @ (0 | 1)) => Some(*slot),
                _ => None,
            })
            .collect();
        assert_eq!(writes, [0, 1]);
        let accepted = events
            .iter()
            .position(|e| *e == Event::WriteSubmitted(1))
            .unwrap();
        let f2 = events
            .iter()
            .position(|e| *e == Event::FlushEffect(2))
            .unwrap();
        let mut rejected = 0;
        let mut mounted = 0;
        for i in 0..events.len() {
            for retained in [0, 1, 2] {
                let dev = SimDevice::new(256, sector)?;
                let ctl = dev.control();
                ctl.trace(Some(i), None);
                assert!(Engine::format(dev, IDS).is_err());
                ctl.power_cut(Persistence::Newest(retained))?;
                let attempt = Engine::mount(ctl.snapshot().device(), IDS, Mode::ReadOnly);
                if i >= f2 || (i >= accepted && retained == 2) {
                    let fs = attempt?;
                    fs.verify()?;
                    assert_eq!(fs.generation(), 1);
                    assert!(fs.list("/", None, 64)?.is_empty());
                    mounted += 1;
                } else {
                    assert!(matches!(attempt, Err(Error::Corrupt(_))));
                    rejected += 1;
                }
            }
        }
        println!(
            "P1 formatage secteur={sector} points={} refus={rejected} racines_vides={mounted}",
            events.len()
        );
    }
    Ok(())
}
