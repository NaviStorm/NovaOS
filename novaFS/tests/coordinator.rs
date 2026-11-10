use novafs::device::{FileImage, Persistence, SimDevice, SimFault};
use novafs::pool::{GRAIN, Pool, PoolCoordinator, Type, Uid};
use novafs::{Block, Device, Engine, Error, Ids, Result, VolumePolicy};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

const A: Uid = [1; 16];
const B: Uid = [2; 16];
const BLOCKS: u64 = 3 * GRAIN;
const FILE: &str = "/record";

fn fixture<D: Device>(disk: D) -> Result<Pool<D>> {
    let mut pool = Pool::format(disk)?;
    for lv in [A, B] {
        let fs = [lv[0] + 100; 16];
        pool.create(lv, Some(fs), 1)?;
        {
            let mut engine = Engine::format(pool.view(lv, Type::Preparing, fs)?, Ids { lv, fs })?;
            engine.create(FILE)?;
            engine.write(FILE, 0, &[lv[0]; 4096])?;
        }
        pool.mark_ready(lv, fs)?;
    }
    Ok(pool)
}

#[test]
fn fifo_lecture_preparation_annulation_et_limites() -> Result<()> {
    for sector in [512, 4096] {
        let mut c = PoolCoordinator::mount(fixture(SimDevice::new(BLOCKS, sector)?)?)?;
        let reads: Vec<_> = (0..16).map(|_| c.request_read(A)).collect::<Result<_>>()?;
        assert!(matches!(c.request_write(B), Err(Error::Busy)));
        drop(reads);
        c.set_volume_policy(
            B,
            VolumePolicy {
                logical_quota_bytes: Some(4097),
                ..VolumePolicy::default()
            },
        )?;
        let mut first = c.request_read(A)?;
        let mut a = c.request_write(A)?;
        let mut b = c.request_write(B)?;
        let mut next = c.request_write(A)?;
        let mut reader = c.request_read(B)?;
        b.prepare(FILE, 0, b"perdu")?;
        a.prepare(FILE, 0, b"valide")?;
        assert!(matches!(next.prepare(FILE, 0, b"x"), Err(Error::Busy)));
        assert!(matches!(reader.read(FILE, 0, 5), Err(Error::Busy)));
        assert!(matches!(b.commit(), Err(Error::Busy)));
        assert!(matches!(a.commit(), Err(Error::Busy)));
        assert_eq!(first.read(FILE, 0, 6)?, [1; 6]);
        assert_eq!(a.commit()?.birth, 4);
        assert!(matches!(a.commit(), Err(Error::Invalid(_))));
        next.prepare(FILE, 0, b"rendu")?;
        next.cancel();
        drop(b);
        assert_eq!(reader.read(FILE, 0, 5)?, [2; 5]);
        assert_eq!(c.request_read(A)?.read(FILE, 0, 6)?, b"valide");
        let mut rejected = c.request_write(B)?;
        assert!(matches!(
            rejected.prepare(FILE, 4097, b"x"),
            Err(Error::QuotaExceeded)
        ));
        let mut ok = c.request_write(B)?;
        ok.prepare(FILE, 4096, b"x")?;
        ok.commit()?;
        assert_eq!(c.request_read(B)?.read(FILE, 4096, 1)?, b"x");
    }
    Ok(())
}

// Deux appels réels de lecture doivent se rencontrer à la frontière du
// périphérique. Le timeout permet au test d'échouer s'ils sont sérialisés.
#[derive(Default)]
struct RendezVous {
    arrived: Mutex<Option<usize>>,
    changed: Condvar,
}

impl RendezVous {
    fn arm(&self) {
        *self.arrived.lock().unwrap() = Some(0);
    }
    fn enter(&self) -> Result<()> {
        let mut state = self.arrived.lock().unwrap();
        match *state {
            None => return Ok(()),
            Some(0) => *state = Some(1),
            Some(_) => {
                *state = None;
                self.changed.notify_all();
                return Ok(());
            }
        }
        let (_guard, timeout) = self
            .changed
            .wait_timeout_while(state, Duration::from_secs(5), |s| s.is_some())
            .unwrap();
        if timeout.timed_out() {
            return Err(Error::Invalid("absence de chevauchement des appels"));
        }
        Ok(())
    }
}

struct Probed {
    image: FileImage,
    probes: Arc<RendezVous>,
}
impl Device for Probed {
    fn blocks(&self) -> u64 {
        self.image.blocks()
    }
    fn read(&self, block: u64) -> Result<Block> {
        self.probes.enter()?;
        self.image.read(block)
    }
    fn write(&mut self, block: u64, bytes: &Block) -> Result<()> {
        self.image.write(block, bytes)
    }
    fn flush(&mut self) -> Result<()> {
        self.image.flush()
    }
}

#[test]
fn lectures_et_preparations_effectivement_chevauchees() -> Result<()> {
    for sector in [512, 4096] {
        let probes = Arc::new(RendezVous::default());
        let dev = Probed {
            image: FileImage::create(BLOCKS, sector)?,
            probes: Arc::clone(&probes),
        };
        let c = PoolCoordinator::mount_with_cache_budget(fixture(dev)?, 0)?;
        let mut r1 = c.request_read(A)?;
        let mut r2 = c.request_read(A)?;
        probes.arm();
        std::thread::scope(|s| -> Result<()> {
            let joined = s.spawn(|| r1.read(FILE, 0, 16));
            assert_eq!(r2.read(FILE, 0, 16)?, [1; 16]);
            assert_eq!(joined.join().unwrap()?, [1; 16]);
            Ok(())
        })?;
        let mut w1 = c.request_write(A)?;
        let mut w2 = c.request_write(B)?;
        probes.arm();
        std::thread::scope(|s| -> Result<()> {
            let joined = s.spawn(|| w2.prepare(FILE, 2, b"abc"));
            w1.prepare(FILE, 2, b"xyz")?;
            joined.join().unwrap()
        })?;
        assert!(matches!(w2.commit(), Err(Error::Busy)));
        w1.commit()?;
        w2.commit()?;
        assert_eq!(c.request_read(A)?.read(FILE, 2, 3)?, b"xyz");
        assert_eq!(c.request_read(B)?.read(FILE, 2, 3)?, b"abc");
    }
    Ok(())
}

fn retry<T>(mut op: impl FnMut() -> Result<T>) -> Result<T> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match op() {
            Err(Error::Busy) if Instant::now() < deadline => std::thread::yield_now(),
            result => return result,
        }
    }
}

#[test]
fn huit_clients_et_deux_lv_par_ordre_admission() -> Result<()> {
    for sector in [512, 4096] {
        let disk = SimDevice::new(BLOCKS, sector)?;
        let control = disk.control();
        let c = PoolCoordinator::mount(fixture(disk)?)?;
        for round in 0..8u8 {
            let mut batch = Vec::new();
            for i in 0..8u8 {
                let lv = if i % 2 == 0 { A } else { B };
                batch.push((c.request_write(lv)?, i));
            }
            control.trace(None, None);
            std::thread::scope(|s| -> Result<()> {
                let mut threads = Vec::new();
                for (mut req, i) in batch.into_iter().rev() {
                    threads.push(s.spawn(move || -> Result<()> {
                        retry(|| req.prepare(FILE, 0, &[8 * round + i]))?;
                        let next = retry(|| req.commit())?;
                        assert_eq!(next.birth, 4 + u64::from(round) * 4 + u64::from(i / 2));
                        Ok(())
                    }));
                }
                for child in threads {
                    child.join().unwrap()?;
                }
                Ok(())
            })?;
            let events = control.events();
            assert_eq!(
                events
                    .iter()
                    .filter(|e| matches!(e, novafs::device::Event::FlushSubmitted(_)))
                    .count(),
                16
            );
            for (lv, i) in [(A, 6), (B, 7)] {
                assert_eq!(c.request_read(lv)?.read(FILE, 0, 1)?, [8 * round + i]);
                c.request_read(lv)?.verify()?;
            }
        }
        let reopened = PoolCoordinator::mount(Pool::import(c.into_device()?)?)?;
        assert_eq!(reopened.request_read(A)?.read(FILE, 0, 1)?, [62]);
        assert_eq!(reopened.request_read(B)?.read(FILE, 0, 1)?, [63]);
    }
    Ok(())
}

#[test]
fn faute_de_barriere_gele_les_tickets_et_reimport() -> Result<()> {
    for sector in [512, 4096] {
        for (barrier, retained, fresh) in [(1, 0, false), (2, 0, false), (2, 1, true)] {
            let disk = SimDevice::new(BLOCKS, sector)?;
            let control = disk.control();
            let c = PoolCoordinator::mount(fixture(disk)?)?;
            {
                let mut a = c.request_write(A)?;
                let mut b = c.request_write(B)?;
                a.prepare(FILE, 0, b"a")?;
                b.prepare(FILE, 0, b"b")?;
                control.inject(SimFault::Flush {
                    ordinal: barrier,
                    persist_newest: retained,
                })?;
                let failure = a.commit().unwrap_err();
                if barrier == 1 {
                    assert!(matches!(failure, Error::Io(_)));
                    assert!(matches!(b.commit(), Err(Error::Unavailable)));
                } else {
                    assert!(matches!(failure, Error::Publication { phase: "F2", .. }));
                    assert!(matches!(b.commit(), Err(Error::UnknownOutcome)));
                }
                assert!(c.request_read(B).is_err());
            }
            let disk = c.into_device()?;
            control.power_cut(Persistence::None)?;
            let c = PoolCoordinator::mount(Pool::import(disk)?)?;
            assert_eq!(
                c.request_read(A)?.read(FILE, 0, 1)?,
                [if fresh { b'a' } else { 1 }]
            );
            assert_eq!(c.request_read(B)?.read(FILE, 0, 1)?, [2]);
            c.request_read(A)?.verify()?;
        }
    }
    Ok(())
}

#[test]
fn enveloppe_cache_partagee_et_restitution_pendant_preparation() -> Result<()> {
    for n in [0, 1, 16] {
        let mut pool = Pool::format(SimDevice::new((n + 1).max(2) * GRAIN, 512)?)?;
        for i in 1..=n {
            let lv = [i as u8; 16];
            let fs = [i as u8 + 48; 16];
            pool.create_for([i as u8 + 16; 16], lv, Some(fs), 1)?;
            {
                let mut fs = Engine::format(pool.view(lv, Type::Preparing, fs)?, Ids { lv, fs })?;
                fs.create(FILE)?;
                fs.write(FILE, 0, &[i as u8; 4096])?;
            }
            pool.mark_ready(lv, fs)?;
        }
        let budget = 16 * 8192 + 3;
        let c = PoolCoordinator::mount_with_cache_budget(pool, budget)?;
        let mut charged = 0;
        let mut pages = 0;
        for i in 1..=n {
            let lv = [i as u8; 16];
            assert_eq!(c.request_read(lv)?.read(FILE, 0, 512)?, [i as u8; 512]);
            let stats = c.cache_stats(lv)?;
            assert_eq!(stats.budget_bytes, budget / n as usize);
            assert!(stats.charged_bytes <= stats.budget_bytes);
            charged += stats.charged_bytes;
            pages += stats.pages;
        }
        assert!(charged <= budget);
        let mut candidate = if n > 0 {
            Some(c.request_write(A)?)
        } else {
            None
        };
        if let Some(w) = &mut candidate {
            w.prepare(FILE, 0, b"apres pression")?;
        }
        // La préparation peut lire des pages supplémentaires ; reprendre le
        // nombre occupé à l'instant de la pression.
        let mut active = 0;
        for i in 1..=n {
            active += c.cache_stats([i as u8; 16])?.pages;
        }
        assert!(pages > 0 || n == 0);
        assert_eq!(c.reclaim_caches()?, active);
        for i in 1..=n {
            assert_eq!(c.cache_stats([i as u8; 16])?.charged_bytes, 0);
        }
        if let Some(mut w) = candidate {
            w.commit()?;
            assert_eq!(c.request_read(A)?.read(FILE, 0, 14)?, b"apres pression");
        }
    }
    let empty = Pool::format(SimDevice::new(512, 512)?)?;
    assert!(matches!(
        PoolCoordinator::mount_with_cache_budget(empty, usize::MAX),
        Err(Error::Limit(_))
    ));
    Ok(())
}

#[test]
fn corruption_connue_interdit_les_caches_et_le_plan_en_attente() -> Result<()> {
    for sector in [512, 4096] {
        let disk = SimDevice::new(BLOCKS, sector)?;
        let control = disk.control();
        let c = PoolCoordinator::mount(fixture(disk)?)?;
        c.request_read(A)?.read(FILE, 0, 4096)?;
        c.request_read(B)?.read(FILE, 0, 4096)?;
        let original = control.snapshot().bytes()?;
        let block = original
            .chunks_exact(4096)
            .position(|chunk| chunk == [1; 4096])
            .unwrap();
        let mut audit = c.request_read(A)?;
        let mut pending = c.request_write(B)?;
        pending.prepare(FILE, 2, b"refuse")?;
        control.replace_stable(block as u64, [7; 4096])?;
        control.trace(None, None);
        assert!(matches!(audit.verify(), Err(Error::Corrupt(_))));
        assert!(matches!(pending.commit(), Err(Error::Unavailable)));
        assert!(matches!(c.request_read(B), Err(Error::Unavailable)));
        assert!(control.events().is_empty());
    }
    Ok(())
}

#[test]
fn echec_non_io_apres_soumission_gele_aussi_le_pool() -> Result<()> {
    use std::sync::atomic::{AtomicBool, Ordering};
    struct Backend {
        disk: SimDevice,
        fail: Arc<AtomicBool>,
    }
    impl Device for Backend {
        fn blocks(&self) -> u64 {
            self.disk.blocks()
        }
        fn read(&self, i: u64) -> Result<Block> {
            self.disk.read(i)
        }
        fn write(&mut self, i: u64, bytes: &Block) -> Result<()> {
            self.disk.write(i, bytes)?;
            if self.fail.swap(false, Ordering::AcqRel) {
                Err(Error::NoSpace)
            } else {
                Ok(())
            }
        }
        fn flush(&mut self) -> Result<()> {
            self.disk.flush()
        }
    }
    for sector in [512, 4096] {
        let disk = SimDevice::new(BLOCKS, sector)?;
        let control = disk.control();
        let fail = Arc::new(AtomicBool::new(false));
        let c = PoolCoordinator::mount(fixture(Backend {
            disk,
            fail: Arc::clone(&fail),
        })?)?;
        {
            let mut a = c.request_write(A)?;
            let mut b = c.request_write(B)?;
            a.prepare(FILE, 0, b"jamais confirme")?;
            b.prepare(FILE, 0, b"interdit")?;
            fail.store(true, Ordering::Release);
            assert!(matches!(a.commit(), Err(Error::NoSpace)));
            control.trace(None, None);
            assert!(matches!(b.commit(), Err(Error::Unavailable)));
            assert!(matches!(c.request_read(B), Err(Error::Unavailable)));
            assert!(control.events().is_empty());
        }
        let backend = c.into_device()?;
        control.power_cut(Persistence::None)?;
        let c = PoolCoordinator::mount(Pool::import(backend)?)?;
        for lv in [A, B] {
            c.request_read(lv)?.verify()?;
            assert_eq!(c.request_read(lv)?.read(FILE, 0, 4)?, [lv[0]; 4]);
        }
    }
    Ok(())
}
