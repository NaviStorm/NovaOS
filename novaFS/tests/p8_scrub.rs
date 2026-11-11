//! Les oracles gardent les octets acquittés, la génération et la copie saine
//! hors du simulateur soumis au scrub. Les clés fixes sont des vecteurs publics.
use novafs::crypto::{Credential, Secret};
use novafs::device::{Event, Persistence, SimDevice, SimFault, SimSnapshot};
use novafs::secure_pool::SecurePool;
use novafs::secure_pool::scrub::{Authority, Mode, Report, Status, scrub};
use novafs::{Compression, Device, Ids, Result};
const POOL: [u8; 16] = [44; 16];
const DATA: [u8; 16384] = [77; 16384];
struct Fixture {
    base: SimSnapshot,
    domain: [u8; 16],
    generation: u64,
    snapshot: u64,
    clone: u64,
    pack: u64,
    root: u64,
}

impl Fixture {
    fn new(sector: u32) -> Result<Self> {
        let d = SimDevice::new(2600, sector)?;
        let ctl = d.control();
        let mut p = SecurePool::format(d, POOL, &Secret::from_bytes([3; 32]))?;
        let clear = p.create(
            [9; 16],
            Ids {
                fs: [4; 16],
                lv: [5; 16],
            },
            256,
            Compression::None,
            None,
        )?;
        p.create_file(clear, "/clair")?;
        p.write_file(clear, "/clair", 0, b"temoin independant")?;
        // Argon2id dispose d'une seule aire de travail dans le processus de test.
        static KEY_SLOT: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let slot = KEY_SLOT.lock().unwrap();
        let origin = p.create(
            [8; 16],
            Ids {
                fs: [6; 16],
                lv: [7; 16],
            },
            256,
            Compression::Lz4,
            Some((b"fixture publique", &Secret::from_bytes([23; 32]))),
        )?;
        drop(slot);
        p.create_file(origin, "/f")?;
        p.write_file(origin, "/f", 0, &DATA)?;
        let birth = p.generation();
        let domain = p.domain(origin)?;
        let snapshot = p.snapshot(origin)?;
        let clone = p.clone_snapshot(snapshot, [15; 16])?;
        p.remove(origin)?;
        let generation = p.generation();
        p.check()?;
        let base = ctl.snapshot();
        let d = base.device();
        use novafs::secure_unit::{Kind, context};
        let units = (46..d.blocks()).step_by(2).filter_map(|b| {
            let ctx = context(&d.read(b).unwrap()).ok()?;
            (ctx.domain == domain && ctx.generation == birth).then_some((b, ctx))
        });
        let pack = units
            .clone()
            .find(|(_, c)| c.kind == Kind::Packed)
            .expect("pack persistant")
            .0;
        let root = units
            .into_iter()
            .find(|(_, c)| c.kind == Kind::Metadata && c.logical == 1 << 32)
            .expect("racine partagée persistante")
            .0;
        Ok(Self {
            base,
            domain,
            generation,
            snapshot,
            clone,
            pack,
            root,
        })
    }
    fn inspect(&self, d: &mut SimDevice, mode: Mode) -> Result<Report> {
        let admin = Secret::from_bytes([3; 32]);
        let key = Secret::from_bytes([23; 32]);
        let domains = [(self.domain, Credential::Recovery(&key))];
        scrub(
            d,
            mode,
            Some(&Authority {
                pool: POOL,
                admin: &admin,
                domains: &domains,
            }),
            |_| true,
        )
    }
    fn cert(&self) -> u64 {
        2 + 2 * (self.generation % 4)
    }
    fn oracle(&self, d: SimDevice) -> Result<()> {
        let mut p = SecurePool::open(d, POOL, &Secret::from_bytes([3; 32]))?;
        p.unlock(
            self.domain,
            Credential::Recovery(&Secret::from_bytes([23; 32])),
        )?;
        assert_eq!(p.generation(), self.generation);
        p.check()?;
        for v in [self.snapshot, self.clone] {
            assert_eq!(p.read_file(v, "/f", 0, DATA.len())?, DATA);
        }
        Ok(())
    }
}
fn flip(d: &SimDevice, b: u64, offset: usize) -> Result<()> {
    let mut p = d.read(b)?;
    p[offset] ^= 1;
    d.control().replace_stable(b, p)
}
#[test]
fn sain_cle_absente_interruption_et_exclusivite() -> Result<()> {
    for sector in [512, 4096] {
        let f = Fixture::new(sector)?;
        let mut d = f.base.device();
        let ctl = d.control();
        ctl.trace(None, None);
        for (mode, status) in [
            (Mode::Diagnostic, Status::Incomplete),
            (Mode::Full, Status::MissingKeys),
            (Mode::Repair, Status::MissingKeys),
        ] {
            let r = scrub(&mut d, mode, None, |_| true)?;
            assert_eq!(r.status, status);
            assert!(!r.authenticated);
        }
        let a = f.inspect(&mut d, Mode::Full)?;
        assert_eq!(a.status, Status::Healthy);
        assert!(a.authenticated && a.views >= 6);
        assert!(a.distinct_blocks > 46 && a.distinct_blocks < a.reads);
        let gate = d.mutation_lock().unwrap();
        let (pause, paused) = std::sync::mpsc::sync_channel(0);
        let (resume, resumed) = std::sync::mpsc::sync_channel(0);
        let writer = std::thread::spawn(move || {
            paused.recv().unwrap();
            assert!(gate.try_lock().is_err());
            resume.send(()).unwrap();
        });
        let resumed_report = scrub(&mut d, Mode::Diagnostic, None, |progress| {
            if progress.reads == 64 {
                pause.send(()).unwrap();
                resumed
                    .recv_timeout(std::time::Duration::from_secs(5))
                    .unwrap();
            }
            true
        })?;
        writer.join().unwrap();
        assert_eq!(resumed_report.status, Status::Incomplete);
        assert!(!resumed_report.locked_views.is_empty());
        let lock = d.mutation_lock().unwrap();
        let r = scrub(&mut d, Mode::Diagnostic, None, |s| {
            assert!(lock.try_lock().is_err());
            s.reads < 64
        })?;
        assert_eq!(r.status, Status::Incomplete);
        assert_ne!(r.run, a.run);
        assert_eq!(r.writes, 0);
        let r = f.inspect(&mut d, Mode::Full)?;
        assert_eq!(r.status, Status::Healthy);
        assert_eq!(f.base.bytes()?, ctl.snapshot().bytes()?);
        assert!(ctl.events().is_empty());
    }
    Ok(())
}
#[test]
fn localiser_et_refuser_les_dommages_combines() -> Result<()> {
    for sector in [512, 4096] {
        let f = Fixture::new(sector)?;
        let c = f.cert();
        let bank = 10 + 9 * (f.generation % 4);
        for faults in [
            vec![(c, 4095), (1, 80)],
            vec![(bank + 8, 42), (c, 4095)],
            vec![(c, 4095), (f.pack + 1, 70)],
            vec![(f.pack, 112)],
            vec![(c, 4095), (f.root + 1, 42)],
            vec![(c, 4095), (c + 1, 4095)],
            vec![(0, 80), (1, 80)],
            vec![(f.pack, 48)],
            vec![(bank, 24)],
        ] {
            let mut d = f.base.device();
            for &(b, off) in &faults {
                flip(&d, b, off)?;
            }
            let ctl = d.control();
            let before = ctl.snapshot().bytes()?;
            ctl.trace(None, None);
            let r = f.inspect(&mut d, Mode::Repair)?;
            assert_ne!(r.status, Status::Healthy);
            assert_ne!(r.status, Status::Repaired);
            assert_eq!((r.writes, r.repaired, r.flushes), (0, 0, 0));
            assert_eq!(before, ctl.snapshot().bytes()?);
            for shared in [f.pack, f.root].into_iter().filter(|shared| {
                faults
                    .iter()
                    .any(|(b, _)| *b == *shared || *b == *shared + 1)
            }) {
                for id in [f.clone, f.snapshot] {
                    assert!(
                        r.issues
                            .iter()
                            .any(|i| i.view == Some(id) && i.blocks == Some((shared, 2))),
                        "{:?}",
                        r.issues
                    );
                }
            }
        }
        let mut d = f.base.device();
        flip(&d, c, 4095)?;
        assert_eq!(f.inspect(&mut d, Mode::Full)?.possible, 1);
        flip(&d, f.pack + 1, 200)?;
        assert_eq!(f.inspect(&mut d, Mode::Repair)?.writes, 0);
    }
    Ok(())
}
#[test]
fn reconstruction_du_certificat_et_remontage() -> Result<()> {
    for sector in [512, 4096] {
        let f = Fixture::new(sector)?;
        let mut d = f.base.device();
        let ctl = d.control();
        let good = d.read(f.cert())?;
        flip(&d, f.cert(), 4095)?;
        let before = ctl.snapshot().bytes()?;
        ctl.trace(None, None);
        let report = f.inspect(&mut d, Mode::Full)?;
        assert_eq!(report.status, Status::Damaged);
        assert_eq!(before, ctl.snapshot().bytes()?);
        let report = f.inspect(&mut d, Mode::Repair)?;
        assert_eq!(report.status, Status::Repaired);
        assert_eq!((report.repaired, report.writes, report.flushes), (1, 1, 2));
        assert_eq!(d.read(f.cert())?, good);
        assert_eq!(ctl.snapshot().bytes()?, f.base.bytes()?);
        let repaired = f.inspect(&mut d, Mode::Repair)?;
        assert_eq!(
            (repaired.status, repaired.writes, repaired.flushes),
            (Status::Healthy, 0, 0)
        );
        f.oracle(ctl.snapshot().device())?;
    }
    Ok(())
}
#[test]
fn coupures_et_eio_avec_oracle_de_contenu() -> Result<()> {
    for sector in [512, 4096] {
        let f = Fixture::new(sector)?;
        let mut d = f.base.device();
        flip(&d, f.cert(), 4095)?;
        let ctl = d.control();
        let initial = ctl.snapshot();
        ctl.trace(None, None);
        assert_eq!(f.inspect(&mut d, Mode::Repair)?.status, Status::Repaired);
        let n = ctl.events().len();
        assert_eq!(
            ctl.events()
                .iter()
                .filter(|e| matches!(e, Event::BeforeWrite(_)))
                .count(),
            1
        );
        for cut in 0..n {
            for persist in [
                Persistence::None,
                Persistence::Newest(1),
                Persistence::Prefix {
                    block: f.cert(),
                    bytes: 2037,
                },
            ] {
                let mut d = initial.device();
                let ctl = d.control();
                ctl.trace(Some(cut), None);
                assert_ne!(f.inspect(&mut d, Mode::Repair)?.status, Status::Repaired);
                ctl.power_cut(persist)?;
                f.oracle(ctl.snapshot().device())?;
            }
        }
        for fault in [
            SimFault::Read { block: f.pack + 1 },
            SimFault::Write {
                block: f.cert(),
                prefix: 1500,
            },
            SimFault::Flush {
                ordinal: 1,
                persist_newest: 0,
            },
            SimFault::Flush {
                ordinal: 2,
                persist_newest: 1,
            },
        ] {
            let mut d = initial.device();
            let ctl = d.control();
            ctl.inject(fault)?;
            let r = f.inspect(&mut d, Mode::Repair)?;
            assert_eq!(r.status, Status::Io);
            assert_eq!(r.repaired, 0);
            ctl.power_cut(Persistence::None)?;
            f.oracle(ctl.snapshot().device())?;
        }
        println!("P8_PANNE secteur={sector} coupures={} EIO=4", n * 3);
    }
    Ok(())
}
#[test]
fn campagne_deterministe_et_sessions_separees() -> Result<()> {
    for sector in [512, 4096] {
        let f = Fixture::new(sector)?;
        let mut seed = 0x95a1_28fdu64;
        let mut counts = [0; 4];
        for _ in 0..64 {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            let kind = (seed % 4) as usize;
            counts[kind] += 1;
            let mut d = f.base.device();
            let ctl = d.control();
            flip(&d, f.cert() + (seed >> 5) % 2, (seed >> 8) as usize % 4096)?;
            if kind == 1 {
                flip(&d, f.pack + 1, (seed >> 16) as usize % 4096)?;
            }
            if kind == 2 {
                ctl.trace(Some(1 + (seed >> 20) as usize % 7), None);
            }
            if kind == 3 {
                ctl.inject(SimFault::Read { block: f.pack })?;
            }
            let r = f.inspect(&mut d, Mode::Repair)?;
            if kind == 0 {
                assert_eq!(r.status, Status::Repaired);
                f.oracle(ctl.snapshot().device())?;
            } else {
                assert_ne!(r.status, Status::Repaired);
                if kind != 2 {
                    assert_eq!(r.writes, 0)
                }
            }
            if kind == 2 {
                ctl.power_cut(Persistence::Newest(1))?;
                f.oracle(ctl.snapshot().device())?;
            }
        }
        assert!(counts.iter().all(|n| *n > 0));
        println!("P8_ALEA secteur={sector} classes={counts:?}");
    }
    Ok(())
}

/// Un backend rend une paire défectueuse lisible seulement après le drainage.
struct AfterDrain {
    disk: SimDevice,
    damaged: u64,
    drained: bool,
    writes: u64,
}
impl Device for AfterDrain {
    fn mutation_lock(&self) -> Option<std::sync::Arc<std::sync::Mutex<()>>> {
        self.disk.mutation_lock()
    }
    fn blocks(&self) -> u64 {
        self.disk.blocks()
    }
    fn read(&self, b: u64) -> Result<novafs::Block> {
        let mut page = self.disk.read(b)?;
        if self.drained && b == self.damaged {
            page[105] ^= 1;
        }
        Ok(page)
    }
    fn write(&mut self, b: u64, p: &novafs::Block) -> Result<()> {
        self.writes += 1;
        self.disk.write(b, p)
    }
    fn flush(&mut self) -> Result<()> {
        self.drained = true;
        self.disk.flush()
    }
}
#[test]
fn relecture_apres_drain_et_repliques_de_deux_generations() -> Result<()> {
    for sector in [512, 4096] {
        let f = Fixture::new(sector)?;
        let mut disk = f.base.device();
        let ctl = disk.control();
        flip(&disk, f.cert(), 4095)?;
        flip(&disk, 2 + 2 * ((f.generation - 1) % 4) + 1, 4095)?;
        let before = ctl.snapshot().bytes()?;
        let mut changed = AfterDrain {
            disk: ctl.snapshot().device(),
            damaged: f.pack + 1,
            drained: false,
            writes: 0,
        };
        let admin = Secret::from_bytes([3; 32]);
        let recovery = Secret::from_bytes([23; 32]);
        let domains = [(f.domain, Credential::Recovery(&recovery))];
        let r = scrub(
            &mut changed,
            Mode::Repair,
            Some(&Authority {
                pool: POOL,
                admin: &admin,
                domains: &domains,
            }),
            |_| true,
        )?;
        assert!(changed.drained);
        assert!(
            r.issues
                .iter()
                .any(|i| i.view == Some(f.clone) && i.blocks == Some((f.pack, 2)))
        );
        assert_eq!(
            (r.status, r.repaired, changed.writes),
            (Status::Irreparable, 0, 0)
        );
        assert_eq!(changed.disk.control().snapshot().bytes()?, before);
        let r = f.inspect(&mut disk, Mode::Repair)?;
        assert_eq!((r.status, r.writes, r.repaired), (Status::Repaired, 2, 2));
        assert_eq!(ctl.snapshot().bytes()?, f.base.bytes()?);
        f.oracle(ctl.snapshot().device())?;
        // La disparition d'un emplacement historique établi ne doit pas être masquée.
        let mut old = f.base.device();
        let retired = 2 + 2 * ((f.generation - 2) % 4);
        old.control().replace_stable(retired, [0; 4096])?;
        assert_eq!(
            f.inspect(&mut old, Mode::Repair)?.status,
            Status::Irreparable
        );
    }
    Ok(())
}
