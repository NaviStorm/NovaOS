//! Préparation sans BME. Les cadres non retirés restent épinglés en quarantaine.
use super::io::EMPTY;
use super::*;
use crate::{memory, sched::sync::Mutex};
static QUARANTAINE: Mutex<Option<Preparation>> = Mutex::new(None);
pub(super) struct Preparation {
    pub(super) domaine: domain::Domain,
    cadres: [Frame; 22],
    pub(super) nic: Device,
}
static ISR_PREPARATION: AtomicU64 = AtomicU64::new(0);
static ETAT_IRQ: AtomicU64 = AtomicU64::new(0);

pub(crate) fn interruption() -> bool {
    let adresse = ISR_PREPARATION.load(Ordering::Acquire);
    if adresse != 0 {
        // Sûreté : BAR vérifiée, exclusivement superviseur, publiée sous masque sur le BSP.
        let raison = unsafe { (adresse as *const u8).read_volatile() };
        ETAT_IRQ.store(0x100 | u64::from(raison), Ordering::Release);
        return true;
    }
    false
}

impl Preparation {
    pub(super) fn verifier_route_interruptions(&self) -> Result<()> {
        if self.nic.cfg.get(4, 2) & 0x404 != 0x400 || self.nic.isr.r8(0) != 0 {
            return Err(Error::Route);
        }
        {
            let _garde = crate::hal::interrupts::Guard::new();
            if ISR_PREPARATION
                .compare_exchange(
                    0,
                    self.nic.isr.base() as u64,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                )
                .is_err()
            {
                return Err(Error::Route);
            }
            ETAT_IRQ.store(0, Ordering::Release);
        }
        let resultat = hal::timer::preparer_spi_reseau()
            .and_then(|()| wait(|| ETAT_IRQ.load(Ordering::Acquire) != 0));
        // Garder le pointeur valide tant que le GIC ne confirme pas la purge.
        hal::timer::purger_spi_reseau()?;
        ISR_PREPARATION.store(0, Ordering::Release);
        resultat?;
        if ETAT_IRQ.load(Ordering::Acquire) != 0x100 {
            return Err(Error::Fault);
        }
        hal::log(format_args!(
            "NET ARM GIC INTID=38 injecte par superviseur, ISR NIC=0, purge confirmee"
        ));
        Ok(())
    }
    pub(super) fn ouvrir() -> Result<Self> {
        if QUARANTAINE.lock().is_some() {
            return Err(Error::Quarantine);
        }
        // L'ECAM du profil virt-10.0 ne contient que le pont hôte et trois
        // fonctions simples. Aucun pont secondaire ni fonction soeur admis.
        for slot in 0..32 {
            let f = pci::Config::open(slot * 8)?;
            let identite = f.get(0, 4);
            let attendue = match slot {
                0 => 0x0008_1b36,
                5 => 0x1057_1af4,
                6 => 0x1042_1af4,
                7 => 0x1041_1af4,
                _ => u32::MAX,
            };
            if identite != attendue || (identite != u32::MAX && f.get(0x0e, 1) != 0) {
                return Err(Error::Route);
            }
        }
        let cfg = pci::Config::open(56)?;
        // La fixture utilise une fonction conventionnelle, sans ATS, PASID ni SR-IOV.
        if !matches!(cfg.get(0x100, 4), 0 | u32::MAX) {
            return Err(Error::Route);
        }
        if cfg.get(0, 4) != 0x10411af4
            || cfg.get(8, 4) >> 16 != 0x0200
            || cfg.get(0x0e, 1) != 0
            || cfg.get(4, 2) & 4 != 0
            || pci::intx_id(56)? != 38
            || pci::intx_id(48)? != 37
        {
            return Err(Error::Route);
        }
        let nic = Device::discover(56, 0x1041, 368)?;
        if nic.msix.is_some() {
            return Err(Error::Route);
        }
        #[cfg(feature = "net-event-idx")]
        super::net_event::negocier(&nic)?;
        #[cfg(not(feature = "net-event-idx"))]
        nic.start((1 << 32) | (1 << 33) | (1 << 5))?;
        let domaine = domain::Domain::reseau()?;
        let mut lease = Self {
            domaine,
            cadres: [EMPTY; 22],
            nic,
        };
        for n in 0..22 {
            let r = Frame::allocate().and_then(|f| {
                for off in (0..PAGE).step_by(8) {
                    f.mem.w64(off, 0);
                }
                lease.cadres[n] = f;
                lease.domaine.mapper_nic(n, f)
            });
            if let Err(e) = r {
                lease.fermer()?;
                return Err(e);
            }
        }
        if nic.cfg.get(4, 2) & 4 != 0 {
            lease.fermer()?;
            return Err(Error::Quarantine);
        }
        Ok(lease)
    }
    pub(super) fn preparer_files(&self, installer: bool) -> Result<()> {
        super::net_files::preparer(&self.nic, &self.cadres, installer)
    }
    pub(super) fn retablir_premiere_page(&self) -> Result<()> {
        self.domaine.mapper_nic(0, self.cadres[0])
    }
    pub(super) fn fermer(self) -> Result<()> {
        if self.retirer().is_err() {
            let mut q = QUARANTAINE.lock();
            assert!(q.is_none());
            *q = Some(self);
            return Err(Error::Quarantine);
        }
        for f in self.cadres {
            if f.pa != 0 {
                // release_dma efface le cadre avant restitution à l’allocateur.
                unsafe { memory::release_dma(f.pa) }
            }
        }
        Ok(())
    }
    fn retirer(&self) -> Result<()> {
        if ISR_PREPARATION.load(Ordering::Acquire) != 0 {
            if ISR_PREPARATION.load(Ordering::Acquire) != self.nic.isr.base() as u64 {
                return Err(Error::Quarantine);
            }
            hal::timer::purger_spi_reseau()?;
            ISR_PREPARATION.store(0, Ordering::Release);
        }
        if self.nic.cfg.get(4, 2) & 4 != 0 {
            return Err(Error::Quarantine);
        }
        self.nic.reset()?;
        self.nic.cfg.intx(false);
        for n in (0..22).rev() {
            self.domaine.retirer_nic(n)?;
        }
        self.domaine.detacher_nic()?;

        Ok(())
    }
}

#[cfg(feature = "net-ethernet-lab")]
impl Preparation {
    pub(super) fn trames(self) -> Result<()> {
        // Les 22 mappings sont acquittés avant publication d'un premier descripteur.
        let resultat = (|| {
            self.domaine.faults_preparation()?;
            let mut port = super::net_trames::Port::nouveau(&self.nic, &self.cadres)?;
            {
                let _garde = hal::interrupts::Guard::new();
                if ISR_PREPARATION
                    .compare_exchange(
                        0,
                        self.nic.isr.base() as u64,
                        Ordering::AcqRel,
                        Ordering::Acquire,
                    )
                    .is_err()
                {
                    return Err(Error::Quarantine);
                }
                ETAT_IRQ.store(0, Ordering::Release);
                hal::timer::armer_spi_reseau(false)?;
            }
            let debut = hal::timer::counter();
            let ok = crate::sched::process::net_preparation::ethernet(
                self.domaine.generation(),
                |tag, len, copie| {
                    if tag == 5 {
                        return port.stopper().map(|()| 0).map_err(|_| crate::abi::REFUSED);
                    }
                    port.demande(tag, len, copie, ETAT_IRQ.load(Ordering::Acquire) == 0x101)
                },
            );
            let irq = ETAT_IRQ.load(Ordering::Acquire);
            let cycles = hal::timer::counter() - debut;
            hal::log(format_args!(
                "NET ARM L2 INTID=38 isr={irq:#x} cycles={cycles} hz={} dma={}",
                hal::timer::counter_hz(),
                memory::pinned_dma_frames()
            ));
            port.stopper()?;
            self.domaine.faults_preparation()?;
            if !ok || irq != 0x101 {
                return Err(Error::Fault);
            }
            Ok(())
        })();
        if resultat.is_err() {
            self.nic.cfg.intx(false);
            self.nic.cfg.master(false);
            if ISR_PREPARATION.load(Ordering::Acquire) == self.nic.isr.base() as u64
                && hal::timer::purger_spi_reseau().is_ok()
            {
                ISR_PREPARATION.store(0, Ordering::Release);
            }
            let mut q = QUARANTAINE.lock();
            assert!(q.is_none());
            *q = Some(self);
            return resultat;
        }
        self.fermer()
    }
}
