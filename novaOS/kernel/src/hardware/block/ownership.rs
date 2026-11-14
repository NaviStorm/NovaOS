//! Un emplacement matériel, y compris pendant la construction et les reprises.
use super::*;
use crate::sched::sync::Mutex;
use core::{
    ops::{Deref, DerefMut},
    sync::atomic::AtomicBool,
};

static OCCUPE: AtomicBool = AtomicBool::new(false);
static QUARANTAINE: Mutex<Option<Retrait>> = Mutex::new(None);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Etape {
    Drainage,
    Arret,
    Route,
    Purge,
    Traductions,
    Domaine,
    Rendu,
}

pub(super) struct Retrait {
    d: Device,
    p: Pages,
    unit: Option<domain::Domain>,
    en_vol: Option<Queue>,
    route: bool,
    traductions: usize,
    etape: Etape,
}

impl Retrait {
    pub(super) fn ouvrir(sid: u8, sortie: bool) -> Result<Bail> {
        OCCUPE
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| Error::Quarantine)?;
        let ressources = (|| {
            let d = Device::discover(sid, 0x1042, 288)?;
            let p = Pages::new()?;
            Ok(Self {
                d,
                p,
                unit: None,
                en_vol: None,
                route: false,
                traductions: 0,
                etape: Etape::Drainage,
            })
        })();
        let mut r = match ressources {
            Ok(r) => r,
            Err(e) => {
                OCCUPE.store(false, Ordering::Release);
                return Err(e);
            }
        };
        let ouverture = (|| {
            r.unit = Some(domain::Domain::preparer(sid).map_err(|e| {
                if e == Error::Absent {
                    Error::Feature
                } else {
                    e
                }
            })?);
            let unit = r.unit.as_mut().ok_or(Error::State)?;
            unit.start(&r.p)?;
            let droits = [1, 1, 3, 1, 2, 2, 1];
            for (n, droit) in droits[..if sortie { 7 } else { 6 }].iter().enumerate() {
                r.traductions = n + 1; // inclut un MAP soumis dont l'acquittement est incertain
                unit.map(&r.p, n, r.p.0[16 + n].pa, *droit)?;
            }
            let d = r.d;
            let ro = d.read_only();
            d.start(VIRTIO | if ro { 1 << 5 } else { 0 })?;
            let (sectors, size) = d.geometry()?;
            r.route = true;
            #[cfg(target_arch = "aarch64")]
            ISR_SANS_FILE.store(0, Ordering::Relaxed);
            ISR.store(d.isr.base() as u64, Ordering::Release);
            unit.route(&r.p, d, true)?;
            let q = Queue::new(
                d,
                0,
                [r.p.0[16], r.p.0[17], r.p.0[18]],
                [addr(0), addr(1), addr(2)],
            )?;
            USED_ADDR.store((r.p.0[18].mem.base() + 2) as u64, Ordering::Release);
            d.ready()?;
            unit.faults(&r.p)?;
            Ok((q, sectors, size, ro))
        })();
        let (q, sectors, size, ro) = match ouverture {
            Ok(verifie) => verifie,
            Err(cause) => {
                r.terminer()?;
                return Err(cause);
            }
        };
        Ok(Bail(Some(Transport {
            p: r.p,
            d: r.d,
            unit: r.unit.take().unwrap(),
            q,
            sectors,
            first: 0,
            count: sectors,
            size,
            pending: None,
            ro,
            output_mapped: sortie,
            traductions: r.traductions,
            started: 0,
            kind: 0,
        })))
    }

    fn du_transport(t: Transport) -> Self {
        Self {
            d: t.d,
            p: t.p,
            unit: Some(t.unit),
            en_vol: t.pending.map(|_| t.q),
            route: true,
            traductions: t.traductions,
            etape: Etape::Drainage,
        }
    }

    fn retirer(&mut self) -> Result<()> {
        // Un échec avant la construction du domaine précède toute publication des pages.
        let Some(unit) = self.unit.as_mut() else {
            return Ok(());
        };
        if self.etape == Etape::Drainage {
            if let Some(q) = &self.en_vol {
                q.completed()?;
            }
            self.en_vol = None;
            self.etape = Etape::Arret;
        }
        if self.etape == Etape::Arret {
            self.d.reset()?;
            self.d.cfg.master(false);
            wait(|| self.d.cfg.get(4, 2) & 4 == 0)?;
            self.etape = Etape::Route;
        }
        if self.etape == Etape::Route {
            if self.route {
                unit.route(&self.p, self.d, false)?;
            }
            self.etape = Etape::Purge;
        }
        if self.etape == Etape::Purge {
            if self.route {
                unit.clear_pending(self.d)?;
                ISR.store(0, Ordering::Release);
                USED_ADDR.store(0, Ordering::Release);
                EXPECT.store(0, Ordering::Release);
            }
            self.etape = Etape::Traductions;
        }
        if self.etape == Etape::Traductions {
            while self.traductions != 0 {
                let n = self.traductions - 1;
                unit.retirer_slot(&self.p, n)?;
                self.traductions = n;
            }
            self.etape = Etape::Domaine;
        }
        if self.etape == Etape::Domaine {
            unit.stop(&self.p)?;
            self.etape = Etape::Rendu;
        }
        Ok(())
    }

    fn terminer(mut self) -> Result<()> {
        if let Err(e) = self.retirer() {
            hal::log(format_args!(
                "DMA quarantaine etape={:?} cause={e:?} session={} traductions_restantes={}",
                self.etape,
                self.p.len(),
                self.traductions
            ));
            let mut attente = QUARANTAINE.lock();
            assert!(attente.is_none());
            *attente = Some(self);
            return Err(Error::Quarantine);
        }
        if let Some(unit) = self.unit.take() {
            unit.garer();
        }
        barrier();
        // Sûreté : lot jamais publié ou retrait acquitté et porte IRQ purgée.
        unsafe { self.p.release() };
        OCCUPE.store(false, Ordering::Release);
        Ok(())
    }
}

pub(super) fn reprendre() -> Result<()> {
    // Extraire sous verrou, puis le rendre avant toute attente matérielle.
    let r = QUARANTAINE.lock().take();
    if let Some(r) = r {
        r.terminer()
    } else if OCCUPE.load(Ordering::Acquire) {
        Err(Error::Quarantine)
    } else {
        Ok(())
    }
}

pub(super) struct Bail(Option<Transport>);
impl Deref for Bail {
    type Target = Transport;
    fn deref(&self) -> &Transport {
        self.0.as_ref().unwrap()
    }
}
impl DerefMut for Bail {
    fn deref_mut(&mut self) -> &mut Transport {
        self.0.as_mut().unwrap()
    }
}
impl Bail {
    pub(super) fn close(mut self) -> Result<()> {
        Retrait::du_transport(self.0.take().unwrap()).terminer()
    }
}
impl Drop for Bail {
    fn drop(&mut self) {
        if let Some(t) = self.0.take() {
            let _ = Retrait::du_transport(t).terminer();
        }
    }
}
