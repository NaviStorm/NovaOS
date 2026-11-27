//! Préparation sans BME. Les cadres non retirés restent épinglés en quarantaine.
use super::io::EMPTY;
#[cfg(feature="gestionnaire-4e1")]
use crate::sched::process::net_preparation::dynamique::reprise;
use super::*;
use crate::{memory, sched::sync::Mutex};
#[cfg(feature="ethernet-v1-port")]
mod reset;
static QUARANTAINE: Mutex<Option<Preparation>> = Mutex::new(None);
pub(super) struct Preparation {
    pub(super) domaine: domain::Domain,
    cadres: [Frame; 22],
    #[cfg(feature="gestionnaire-4d")]
    decision: Option<crate::sched::process::net_preparation::dynamique::Decision>,
    pub(super) nic: Device,
    #[cfg(feature="ethernet-v1-port")]
    retrait_differe: Option<u64>,
    #[cfg(feature="ethernet-v1-port")]
    reset_negocie: bool,
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
    fn inventaire() -> Result<pci::Config> {
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
        Ok(cfg)
    }
    #[cfg(feature="gestionnaire-4d")]
    pub(super) fn correspond(id:u64,sid:u64)->bool {
        sid==56 && Self::inventaire().is_ok_and(|cfg|u64::from(cfg.get(0,4))==id)
    }
    #[cfg(feature="ethernet-v1-port")]
    fn negocier(nic:&Device)->Result<bool>{
        if reset::profil(){super::net_event::negocier_reset(nic)}else{
            super::net_event::negocier(nic)?;
            Ok(false)
        }
    }
    pub(super) fn ouvrir() -> Result<Self> {
        if QUARANTAINE.lock().is_some() {
            return Err(Error::Quarantine);
        }
        #[cfg(feature="gestionnaire-4d")]
        let debut=hal::timer::counter();
        let cfg=Self::inventaire()?;
        #[cfg(feature="gestionnaire-4d")]
        hal::log(format_args!("D4 decouverte ticks={}",hal::timer::counter()-debut));
        #[cfg(feature="gestionnaire-4d")]
        let decision = crate::sched::process::net_preparation::dynamique::selection(cfg.get(0,4) as u64, 56)
            .ok_or(Error::State)?;
        let nic = Device::discover(56, 0x1041, 368)?;
        if nic.msix.is_some() {
            return Err(Error::Route);
        }
        #[cfg(all(feature="ethernet-v1-port",not(feature="gestionnaire-4d")))]
        let reset_negocie = Self::negocier(&nic)?;
        #[cfg(feature="gestionnaire-4d")]
        let reset_negocie = false;
        #[cfg(all(feature = "net-event-idx", not(feature="ethernet-v1-port")))]
        super::net_event::negocier(&nic)?;
        #[cfg(not(feature = "net-event-idx"))]
        nic.start((1 << 32) | (1 << 33) | (1 << 5))?;
        let domaine = domain::Domain::reseau()?;
        let mut lease = Self {
            domaine,
            cadres: [EMPTY; 22],
            #[cfg(feature="gestionnaire-4d")]
            decision: Some(decision),
            nic,
            #[cfg(feature="ethernet-v1-port")]
            retrait_differe: None,
            #[cfg(feature="ethernet-v1-port")]
            reset_negocie,
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
            #[cfg(feature="gestionnaire-4e1")]
            if !reprise::map(lease.domaine.generation(),n,lease.cadres[n].pa){
                *QUARANTAINE.lock()=Some(lease);
                return Err(Error::Quarantine)
            }
            #[cfg(feature="gestionnaire-4d")]
            if n==2 && option_env!("NOVAOS_4D_REFUS")==Some("reservation"){
                hal::log(format_args!("D4 reservation interrompue mappings=3"));
                lease.fermer()?;
                return Err(Error::State)
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
    pub(super) fn fermer(#[allow(unused_mut)] mut self) -> Result<()> {
        // Une quarantaine matérielle possède seulement le bail DMA et ses preuves.
        #[cfg(feature="gestionnaire-4d")]
        drop(self.decision.take());
        #[cfg(feature="gestionnaire-4e1")]
        let autorise=reprise::liberation_permises(self.domaine.generation(),self.cadres.map(|f|f.pa));
        #[cfg(not(feature="gestionnaire-4e1"))]
        let autorise=true;
        if !autorise || self.retirer().is_err() {
            let mut q = QUARANTAINE.lock();
            assert!(q.is_none());
            *q = Some(self);
            return Err(Error::Quarantine);
        }
        #[cfg(feature="gestionnaire-4e1")]
        let generation=self.domaine.generation();
        for f in self.cadres {
            if f.pa != 0 {
                // release_dma efface le cadre avant restitution à l’allocateur.
                unsafe { memory::release_dma(f.pa) }
            }
        }
        #[cfg(feature="gestionnaire-4e1")]
        reprise::rendu(generation,[0;22]);
        Ok(())
    }
    fn purger_irq(&self) -> Result<()> {
        if ISR_PREPARATION.load(Ordering::Acquire) != 0 {
            if ISR_PREPARATION.load(Ordering::Acquire) != self.nic.isr.base() as u64 {
                return Err(Error::Quarantine);
            }
            hal::timer::purger_spi_reseau()?;
            ISR_PREPARATION.store(0, Ordering::Release);
        }
        Ok(())
    }
    fn retirer(&self) -> Result<()> {
        self.purger_irq()?;
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
    pub(super) fn trames(#[allow(unused_mut)] mut self, pendant:impl FnOnce()->bool) -> Result<()> {
        // Les 22 mappings sont acquittés avant publication d'un premier descripteur.
        #[cfg(feature="ethernet-v1-port")]
        let mut preuve = None;
        #[cfg(feature="ethernet-v1-port")]
        let mut files_confirmees = false;
        #[cfg(feature="gestionnaire-4d")]
        let acteur = {
            let decision=self.decision.take().filter(|d|Self::inventaire().is_ok_and(|c|d.actuelle(c.get(0,4) as u64,56)));
            match crate::sched::process::net_preparation::port_v1::EthernetActor::reserve(self.domaine.generation(),decision) {
                Some(a)=>Some(a),
                None=>{
                    let generation=self.domaine.generation();
                    self.fermer()?;
                    hal::log(format_args!("D4 lancement refuse avant files generation={generation} rendus=22"));
                    return Err(Error::State);
                }
            }
        };
        let resultat = (|| {
            #[cfg(feature="gestionnaire-4d")]
            {
                self.reset_negocie=Self::negocier(&self.nic)?;
                hal::log(format_args!("D4 initialisation generation={} bme_avant_files={}",self.domaine.generation(),self.nic.cfg.get(4,2)&4!=0));
            }
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
            #[cfg(all(feature="ethernet-v1-port",not(feature="gestionnaire-4d")))]
            let acteur = if reset::profil() {
                Some(crate::sched::process::net_preparation::port_v1::EthernetActor::reserve(self.domaine.generation()).ok_or(Error::State)?)
            } else {None};
            #[cfg(feature="ethernet-v1-port")]
            let titulaire = acteur.as_ref().map(|a| a.titulaire_dma());
            let operation = |tag, len, copie: &mut [u8]| {
                    if tag == 5 {
                        #[cfg(feature="gestionnaire-4d")]
                        crate::sched::process::net_preparation::dynamique::debut_retrait();
                        #[cfg(feature="ethernet-v1-port")]
                        {
                            if let Some(id) = titulaire && self.reset_negocie {
                                let recu = reset::retirer(&self.nic, id);
                                files_confirmees = recu.is_some_and(|r| r.confirme(id, self.domaine.generation()));
                                hal::log(format_args!("E3E files pilote={} incarnation={} generation={} confirmees={files_confirmees}", id.0, id.1, id.2));
                            }
                            preuve = port.sceller(self.domaine.generation()).ok();
                            return preuve.map(|_| 0).ok_or(crate::abi::REFUSED);
                        }
                        #[cfg(not(feature="ethernet-v1-port"))]
                        return port.stopper().map(|()| 0).map_err(|_| crate::abi::REFUSED);
                    }
                    if tag==6 {
                        let _garde=hal::interrupts::Guard::new();
                        if ETAT_IRQ.swap(0,Ordering::AcqRel)!=0x101 {return Err(crate::abi::REFUSED);}
                        let _=self.nic.isr.r8(0);
                        hal::timer::purger_spi_reseau().and_then(|()|hal::timer::armer_spi_reseau(false)).map_err(|_|crate::abi::REFUSED)?;
                    }
                    port.demande(tag, len, copie, ETAT_IRQ.load(Ordering::Acquire) == 0x101)
                };
            #[cfg(feature="ethernet-v1-port")]
            let ok = if let Some(a) = acteur {
                crate::sched::process::net_preparation::port_v1::ethernet_actor(a, pendant, operation, false)
            } else {
                crate::sched::process::net_preparation::ethernet(self.domaine.generation(), pendant, operation)
            };
            #[cfg(not(feature="ethernet-v1-port"))]
            let ok = crate::sched::process::net_preparation::ethernet(self.domaine.generation(), pendant, operation);
            let irq = ETAT_IRQ.load(Ordering::Acquire);
            let cycles = hal::timer::counter() - debut;
            hal::log(format_args!(
                "NET ARM L2 INTID=38 isr={irq:#x} cycles={cycles} hz={} dma={}",
                hal::timer::counter_hz(),
                memory::pinned_dma_frames()
            ));
            #[cfg(not(feature="ethernet-v1-port"))]
            {
                port.stopper()?;
                if !ok || irq != 0x101 { return Err(Error::Fault); }
            }
            #[cfg(feature="ethernet-v1-port")]
            {
                hal::log(format_args!("E3C session generation={} succes={ok}", self.domaine.generation()));
                preuve.ok_or(Error::Quarantine)?;
                self.purger_irq()?;
            }
            self.domaine.faults_preparation()
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
        #[cfg(feature="ethernet-v1-port")]
        return if files_confirmees && preuve.is_some() {
            self.retrait_complet()
        } else {
            self.recuperer(preuve.ok_or(Error::Quarantine)?)
        };
        #[cfg(not(feature="ethernet-v1-port"))]
        self.fermer()
    }

    #[cfg(feature="ethernet-v1-port")]
    fn retrait_complet(self) -> Result<()> {
        #[cfg(feature="gestionnaire-4d")]
        let _temps=crate::sched::process::net_preparation::dynamique::Duree::new("retrait");
        let generation = self.domaine.generation();
        let numero = {
            let mut place = QUARANTAINE.lock();
            if place.is_some() { return Err(Error::Quarantine); }
            let bail = place.insert(self);
            // La fermeture interdit toute nouvelle MAP. Les cadres restent épinglés,
            // y compris celui déjà retiré, jusqu'au dernier reçu et au DETACH.
            bail.domaine.retirer_nic(21)?;
            let n = bail.domaine.differer_nic()?;
            bail.retrait_differe = Some(n);
            n
        };
        if matches!(option_env!("NOVAOS_ETH3_CASE"), Some("recu-arm-differe" | "recu-arm-absent" | "reset-iommu-differe" | "reset-iommu-absent")) {
            // Le contrôleur peut déjà avoir enregistré le succès. Le propriétaire du
            // bail n'en reçoit aucune notification lors de ce premier passage.
            let q = QUARANTAINE.lock();
            let bail=q.as_ref().ok_or(Error::State)?;
            bail.domaine.reconcilier_nic(numero)?;
            #[cfg(feature="gestionnaire-4e1")]
            {
                let frames=bail.cadres.map(|f|f.pa);
                if !reprise::ressources(generation,frames,bail.domaine.carte_nic(frames).inspect_err(|_|reprise::geler_carte(generation))?,0,"attente"){return Err(Error::Quarantine)}
            }
            return Err(Error::Quarantine);
        }
        Self::reprendre_retrait(generation, numero)
    }

    #[cfg(feature="ethernet-v1-port")]
    pub(super) fn attente_retrait(generation: u64) -> Option<(u64, usize)> {
        let q = QUARANTAINE.lock();
        let bail = q.as_ref()?;
        (bail.domaine.generation() == generation).then_some((
            bail.retrait_differe?, bail.cadres.iter().filter(|f| f.pa != 0).count()
        ))
    }

    #[cfg(feature="ethernet-v1-port")]
    pub(super) fn reprendre_retrait(generation: u64, numero: u64) -> Result<()> {
        #[cfg(feature="gestionnaire-4d")]
        let _temps=crate::sched::process::net_preparation::dynamique::Duree::new("reconciliation");
        let bail = {
            let mut place = QUARANTAINE.lock();
            let p = place.as_ref().ok_or(Error::State)?;
            if p.domaine.generation()!=generation || p.retrait_differe!=Some(numero) {
                return Err(Error::State);
            }
            p.domaine.reconcilier_nic(numero)?;
            #[cfg(feature="gestionnaire-4e1")]
            {
                let frames=p.cadres.map(|f|f.pa);
                let mapped=p.domaine.carte_nic(frames).inspect_err(|_|reprise::geler_carte(generation))?;
                if !reprise::ressources(generation,frames,mapped,numero,"reconciliation"){return Err(Error::Quarantine)}
            }
            let mut bail = place.take().ok_or(Error::State)?;
            bail.retrait_differe = None;
            bail
        };
        bail.fermer()?;
        hal::log(format_args!("E3C VirtIO-IOMMU generation={generation} rendus=22 quarantaine=0 detache=true"));
        #[cfg(feature="gestionnaire-4d")]
        crate::sched::process::net_preparation::dynamique::fin_retrait();
        Ok(())
    }

    #[cfg(feature="ethernet-v1-port")]
    fn recuperer(mut self, preuve: super::net_trames::Arret) -> Result<()> {
        let generation = self.domaine.generation();
        let pages: [bool; 22] = core::array::from_fn(|i| preuve.page_restituable(i));
        let rendues = pages.iter().filter(|v| **v).count();
        if rendues == pages.len() {
            return self.retrait_complet();
        }
        // Conserver le domaine en fermeture. Le disque dispose de sa propre
        // attribution. Chaque unmap synchrone vérifie incarnation, ID, used et statut.
        let retrait = (|| {
            for (i, &libre) in pages.iter().enumerate() {
                if libre { self.domaine.retirer_nic(i)?; }
            }
            self.domaine.faults_preparation()
        })();
        let mut liberes = 0;
        #[cfg(feature="gestionnaire-4e1")]
        let autorise={let frames=self.cadres.map(|f|f.pa);self.domaine.carte_nic(frames).inspect_err(|_|reprise::geler_carte(generation)).is_ok_and(|m|reprise::ressources(generation,frames,m,0,"retrait-partiel")) && reprise::liberation_permises(generation,frames)};
        #[cfg(not(feature="gestionnaire-4e1"))]
        let autorise=true;
        if autorise && retrait.is_ok() {
            for (i, &libre) in pages.iter().enumerate() {
                if libre {
                    // Sûreté : propriété du couple de descripteurs démontrée,
                    // reset et purge IRQ confirmés, traduction de cette page retirée.
                    unsafe { memory::release_dma(self.cadres[i].pa) }
                    self.cadres[i] = EMPTY;
                    liberes += 1;
                }
            }
        }
        #[cfg(feature="gestionnaire-4e1")]
        {
            let frames=self.cadres.map(|f|f.pa);
            reprise::rendu(generation,frames);
            if let Ok(mapped)=self.domaine.carte_nic(frames).inspect_err(|_|reprise::geler_carte(generation)){
                reprise::ressources(generation,frames,mapped,0,"quarantaine");
            }
        }
        hal::log(format_args!("E3C VirtIO-IOMMU generation={generation} rendus={liberes} quarantaine={} detache=false", 22-liberes));
        let mut q = QUARANTAINE.lock();
        assert!(q.is_none());
        *q = Some(self);
        Err(Error::Quarantine)
    }
}
