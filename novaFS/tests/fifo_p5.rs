//! Publication FIFO, préparations concurrentes et revalidation des branches.
use novafs::device::SimDevice;
use novafs::shared::{SharedCoordinator, SharedPool};
use novafs::{Block, Device, Error, Ids, Result};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

const FILE: &str = "/contenu";

fn fixture<D: Device>(dev: D) -> Result<(SharedPool<D>, u64, u64)> {
    let mut pool = SharedPool::format(dev, [10; 16])?;
    let mut views = [0; 2];
    for (i, slot) in views.iter_mut().enumerate() {
        let id = i as u8 + 1;
        *slot = pool.create(
            [id; 16],
            Ids {
                fs: [id + 10; 16],
                lv: [id + 50; 16],
            },
            256,
        )?;
        pool.create_file(*slot, FILE)?;
        pool.write_file(*slot, FILE, 0, &[id; 4096])?;
    }
    Ok((pool, views[0], views[1]))
}

#[test]
fn fifo_deux_plans_et_annulations_restituent_les_tickets() -> Result<()> {
    for sector in [512, 4096] {
        let (pool, a, b) = fixture(SimDevice::new(1536, sector)?)?;
        let c = SharedCoordinator::mount(pool)?;
        let reads = (0..16)
            .map(|_| c.request_read(a))
            .collect::<Result<Vec<_>>>()?;
        assert!(matches!(c.request_admin(), Err(Error::Busy)));
        drop(reads);
        let mut r = c.request_read(a)?;
        let mut w1 = c.request_write(a)?;
        let mut w2 = c.request_write(b)?;
        let mut w3 = c.request_write(a)?;
        let mut next = c.request_read(b)?;
        w2.prepare(FILE, 0, b"annule")?;
        w1.prepare(FILE, 0, b"publie")?;
        assert!(matches!(w3.prepare(FILE, 0, b"z"), Err(Error::Busy)));
        assert!(matches!(w2.commit(), Err(Error::Busy)));
        assert!(matches!(w1.commit(), Err(Error::Busy)));
        assert!(matches!(next.read(FILE, 0, 2), Err(Error::Busy)));
        assert_eq!(r.read(FILE, 0, 2)?, [1; 2]);
        w1.commit()?;
        assert!(matches!(w1.commit(), Err(Error::Invalid(_))));
        w3.prepare(FILE, 0, b"rendu")?;
        w3.cancel();
        w2.cancel();
        assert_eq!(next.read(FILE, 0, 6)?, [2; 6]);
        assert_eq!(c.request_read(a)?.read(FILE, 0, 6)?, b"publie");
    }
    Ok(())
}

#[test]
fn administration_intercalee_racine_supprimee_modifiee_ou_partagee() -> Result<()> {
    for sector in [512, 4096] {
        let (pool, a, b) = fixture(SimDevice::new(1536, sector)?)?;
        let c = SharedCoordinator::mount(pool)?;
        let mut creation = c.request_admin()?;
        let mut edit = c.request_write(a)?;
        edit.prepare(FILE, 0, b"apres")?;
        assert!(matches!(edit.commit(), Err(Error::Busy)));
        let snapshot = creation.publish(|pool| pool.snapshot(a))?;
        edit.commit()?;
        assert_eq!(c.request_read(snapshot)?.read(FILE, 0, 5)?, [1; 5]);
        assert_eq!(c.request_read(a)?.read(FILE, 0, 5)?, b"apres");
        let mut mutation = c.request_admin()?;
        let mut stale = c.request_write(a)?;
        stale.prepare(FILE, 0, b"caduc")?;
        mutation.publish(|pool| pool.write_file(a, FILE, 0, b"indep"))?;
        assert!(matches!(stale.commit(), Err(Error::Invalid(_))));
        assert_eq!(c.request_read(a)?.read(FILE, 0, 5)?, b"indep");
        let mut suppression = c.request_admin()?;
        let mut orphan = c.request_write(a)?;
        orphan.prepare(FILE, 0, b"perdu")?;
        suppression.publish(|pool| pool.remove(a))?;
        assert!(matches!(orphan.commit(), Err(Error::NotFound)));
        assert_eq!(c.request_read(snapshot)?.read(FILE, 0, 5)?, [1; 5]);
        assert_eq!(c.request_read(b)?.read(FILE, 0, 5)?, [2; 5]);
        let mut ro = c.request_write(snapshot)?;
        assert!(matches!(ro.prepare(FILE, 0, b"x"), Err(Error::ReadOnly)));
        assert_eq!(c.request_read(snapshot)?.read(FILE, 0, 1)?, [1]);
    }
    Ok(())
}

// Une barrière au premier read du support prouve que deux préparations sont
// réellement entrées dans le pilote. À défaut, le test échoue au délai.
#[derive(Default)]
struct Rencontre {
    attente: Mutex<Option<bool>>,
    signal: Condvar,
}
impl Rencontre {
    fn armer(&self) {
        *self.attente.lock().unwrap() = Some(false);
    }
    fn lire(&self) -> Result<()> {
        let mut attente = self.attente.lock().unwrap();
        match *attente {
            None => Ok(()),
            Some(true) => {
                *attente = None;
                self.signal.notify_all();
                Ok(())
            }
            Some(false) => {
                *attente = Some(true);
                let (_lock, deadline) = self
                    .signal
                    .wait_timeout_while(attente, Duration::from_secs(4), |v| v.is_some())
                    .unwrap();
                if deadline.timed_out() {
                    return Err(Error::Invalid("absence de lectures simultanées"));
                }
                Ok(())
            }
        }
    }
}

struct Observed {
    inner: SimDevice,
    probe: Arc<Rencontre>,
}
impl Device for Observed {
    fn blocks(&self) -> u64 {
        self.inner.blocks()
    }
    fn read(&self, b: u64) -> Result<Block> {
        self.probe.lire()?;
        self.inner.read(b)
    }
    fn write(&mut self, b: u64, bytes: &Block) -> Result<()> {
        self.inner.write(b, bytes)
    }
    fn flush(&mut self) -> Result<()> {
        self.inner.flush()
    }
}

#[test]
fn lectures_et_preparations_simultanees_p5() -> Result<()> {
    for sector in [512, 4096] {
        let probe = Arc::new(Rencontre::default());
        let (pool, a, b) = fixture(Observed {
            inner: SimDevice::new(1536, sector)?,
            probe: Arc::clone(&probe),
        })?;
        let c = SharedCoordinator::mount(pool)?;
        probe.armer();
        let mut r1 = c.request_read(a)?;
        let mut r2 = c.request_read(b)?;
        std::thread::scope(|s| -> Result<()> {
            let second = s.spawn(|| r2.read(FILE, 0, 20));
            assert_eq!(r1.read(FILE, 0, 5)?, [1; 5]);
            assert_eq!(second.join().unwrap()?, [2; 20]);
            Ok(())
        })?;
        let mut w1 = c.request_write(a)?;
        let mut w2 = c.request_write(b)?;
        probe.armer();
        std::thread::scope(|s| -> Result<()> {
            let second = s.spawn(|| w2.prepare(FILE, 0, b"222"));
            w1.prepare(FILE, 0, b"111")?;
            second.join().unwrap()?;
            Ok(())
        })?;
        assert!(matches!(w2.commit(), Err(Error::Busy)));
        w1.commit()?;
        w2.commit()?;
        assert_eq!(c.request_read(a)?.read(FILE, 0, 3)?, b"111");
        assert_eq!(c.request_read(b)?.read(FILE, 0, 3)?, b"222");
    }
    Ok(())
}
