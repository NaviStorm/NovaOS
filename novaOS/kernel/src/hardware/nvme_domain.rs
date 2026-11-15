//! Admission du seul NVMe Q35 sur 00:06.0, avant restitution des pages firmware.
//! Les adresses physiques, pages de tables, descripteurs et fenêtres UC restent au superviseur.
mod reuse;
pub(crate) use super::vtd_lab::Error;
use super::{
    discovery::{Config, Drhd, vtd},
    nvme::{Controller, Drained, Memory},
    pci_inventory::{self as pci, resources::lab},
    vtd_lab::{Engine, Invalidation, Lease, NvmeLayout},
};
use crate::{hal::log, memory, sched::Guard};
use alloc::vec::Vec;
use core::{
    mem::ManuallyDrop,
    sync::atomic::{AtomicBool, Ordering},
};

type Result<T> = core::result::Result<T, Error>;
const SID: u16 = 6 << 3;
const ID: u32 = 0x0010_1b36;
const PAGE: usize = 4096;
// Les slots UC du banc sont exclusifs entre les profils. 323 sert à l'AHCI.
const BAR_SLOT: usize = 324;

static DISCOVERED: AtomicBool = AtomicBool::new(false);

fn cfg(offset: usize, width: usize) -> u32 {
    super::pci_ports::port(SID, offset, width, None)
}
fn write_command(command: u16) {
    super::pci_ports::port(SID, 4, 2, Some(u32::from(command)));
}

/// Seuls les registres et les doorbells ont un alias UC. L'aire MSI-X
/// (0x2000) et son PBA (0x3000) restent hors de cet alias.
struct Bar {
    physical: u64,
    mapped: usize,
    command: u16,
}
impl Bar {
    fn va(i: usize) -> u64 {
        memory::IO_BASE + (BAR_SLOT + i) as u64 * memory::PAGE
    }
    fn snapshot(&self, command: u16) -> bool {
        // Ces offsets sont propres au modèle QEMU 11.1.3 figé. Vérifier aussi le
        // chaînage avant de se servir des registres encodés à ces positions.
        cfg(0,4) == ID
            && cfg(4,2) == u32::from(command)
            && cfg(0x0e,1) == 0
            && cfg(0x34,1) == 0x40
            && cfg(0x40,4) & !0x4000_0000 == 0x0001_8011 // MSI-X désactivé, Function Mask libre
            && cfg(0x44,4) == 0x2000
            && cfg(0x48,4) == 0x3000
            && cfg(0x80,2) == 0x6010
            && cfg(0x60,2) == 0x0001
            && cfg(0x10,4) == (self.physical as u32 | 4)
            && cfg(0x14,4) == (self.physical >> 32) as u32
    }
    fn map(&mut self) -> Result<()> {
        for i in 0..2 {
            memory::map_io(self.physical + i * memory::PAGE, Self::va(i as usize))
                .map_err(|_| Error::Memory)?;
            self.mapped += 1;
        }
        Ok(())
    }
    fn unmap(&mut self) -> Result<()> {
        while self.mapped != 0 {
            let i = self.mapped - 1;
            memory::unmap_lab_io(self.physical + i as u64 * memory::PAGE, Self::va(i))
                .map_err(|_| Error::Quarantine)?;
            self.mapped = i;
        }
        Ok(())
    }
    fn prepared_queues(&self, memory: &Memory) -> bool {
        if self.mapped != 2 {
            return false;
        }
        // Sûreté : relecture avant ouverture de BME des registres de la
        // paire admin du courtier ; les valeurs sont comparées au bail actuel.
        let base = Self::va(0);
        let r32 = |o: u64| unsafe { ((base + o) as *const u32).read_volatile() };
        let r64 = |o: u64| unsafe { ((base + o) as *const u64).read_volatile() };
        r32(0x14) & 1 == 0
            && r32(0x1c) == 0
            && r32(0x24) == 0x000f_000f
            && r64(0x28) == memory.iova[0]
            && r64(0x30) == memory.iova[1]
    }
    fn clear_prepared(&self, memory: &Memory) -> bool {
        if !self.fenced() || self.mapped != 2 {
            return false;
        }
        let base = Self::va(0);
        // Sûreté : aucune tentative d'ouverture BME depuis prepare ; seul le
        // courtier a pu préparer ces registres. Refuser tout autre pointeur.
        unsafe {
            let cc = (base + 0x14) as *const u32;
            let csts = (base + 0x1c) as *const u32;
            let aqa = (base + 0x24) as *mut u32;
            let asq = (base + 0x28) as *mut u64;
            let acq = (base + 0x30) as *mut u64;
            if cc.read_volatile() != 0
                || csts.read_volatile() != 0
                || ![0, 0x000f_000f].contains(&aqa.read_volatile())
                || ![0, memory.iova[0]].contains(&asq.read_volatile())
                || ![0, memory.iova[1]].contains(&acq.read_volatile())
            {
                return false;
            }
            aqa.write_volatile(0);
            asq.write_volatile(0);
            acq.write_volatile(0);
        }
        self.fenced() && self.virgin()
    }
    fn virgin(&self) -> bool {
        if self.mapped != 2 {
            return false;
        }
        // Sûreté : les deux feuilles UC sont réservées à ce BAR identifié ; la
        // lecture ci-dessous ne change aucun registre, même en cas de refus.
        let base = Self::va(0);
        let r32 = |o: u64| unsafe { ((base + o) as *const u32).read_volatile() };
        let r64 = |o: u64| unsafe { ((base + o) as *const u64).read_volatile() };
        let cc = r32(0x14);
        let csts = r32(0x1c);
        let aqa = r32(0x24);
        let asq = r64(0x28);
        let acq = r64(0x30);
        let clean = (cc, csts, aqa, asq, acq) == (0, 0, 0, 0, 0);
        log(format_args!(
            "NVME etat initial CC={cc:#x} CSTS={csts:#x} AQA={aqa:#x} ASQ={asq:#x} ACQ={acq:#x} vierge={clean}"
        ));
        clean
    }
    fn fenced(&self) -> bool {
        self.snapshot(self.command)
    }
    fn master(&self, open: bool) -> bool {
        let (old, new) = if open {
            (self.command, self.command | 4)
        } else {
            (self.command | 4, self.command)
        };
        if !self.snapshot(old) {
            return false;
        }
        write_command(new);
        self.snapshot(new)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DataPage {
    Mapped,
    UntilRead,
    Withheld,
}
impl DataPage {
    fn mapped(self) -> bool {
        self != Self::Withheld
    }
}

pub(crate) struct Admission {
    source: Option<bool>,
    phase2_media: bool,
    retirement: Invalidation,
    data_page: DataPage,
    bar: Bar,
    drhd: Drhd,
}

impl Admission {
    pub(crate) fn identity(&self) -> [u64; 4] {
        [
            u64::from(self.drhd.sid()),
            u64::from(ID),
            0x010802,
            self.bar.physical,
        ]
    }
}

/// Exécuté sur le BSP avant les AP. Le chemin de refus conserve les pages
/// firmware. Il n'ouvre jamais le décodage mémoire pour inspecter une fonction.
pub(crate) fn discover() -> Option<Admission> {
    if crate::smp::index() != 0 || DISCOVERED.swap(true, Ordering::AcqRel) {
        return None;
    }
    let mut functions = Vec::new();
    functions.try_reserve_exact(5).ok()?;
    let mut complete = true;
    pci::enumerate(
        &mut Config,
        pci::BusRange {
            segment: 0,
            first: 0,
            last: 0,
        },
        |a, f| match f {
            Ok(f) if functions.len() < 5 => functions.push((a, f)),
            _ => complete = false,
        },
    )
    .ok()?;
    if !complete {
        return None;
    }
    let (physical, old) = lab::qualify_nvme(&functions)
        .map_err(|e| log(format_args!("NVME admission PCI refusee {e:?}")))
        .ok()?;
    let drhd = vtd::q35_drhd(lab::Q35_LAB_SLOT)?;
    if drhd.sid() != SID || !memory::firmware::unlisted_extent(physical, lab::NVME_SIZE) {
        return None;
    }
    let ahci = functions
        .iter()
        .find(|(a, _)| (a.device, a.function) == (31, 2))?
        .1;
    let abar = ahci.bars[5]?.base;
    if ahci.command & 2 == 0
        || super::pci_ports::port(0xfa, 0x24, 4, None) as u64 != abar
        || !vtd::ahci_ports(abar, false)
    {
        return None;
    }
    let mut bar = Bar {
        physical,
        mapped: 0,
        command: (old & !4) | 0x400,
    };
    if !bar.snapshot(old) {
        return None;
    }
    // Si le BAR ou le domaine est incertain, aucune autre adoption dans ce boot.
    let ok = bar.map().is_ok() && bar.virgin() && bar.snapshot(old);
    if !ok {
        let _ = bar.unmap();
        return None;
    }
    write_command(bar.command);
    if !bar.fenced() || !vtd::quiesce_peers(&functions, lab::Q35_LAB_SLOT) {
        let _ = bar.unmap();
        return None;
    }
    log(format_args!(
        "NVME PCI 0000:00:06.0 1b36:0010 classe=010802 BAR0={physical:#x} taille=16384 PCIe=true MSI=false MSI-X=2 desactivee INTx=masquee SID={SID} BME=0"
    ));
    // L'admission ne possède aucun alias MMIO. Un refus du gestionnaire Ring 3
    // avant la création du domaine peut la laisser tomber sans ressource à rendre.
    bar.unmap().ok()?;
    let scenario = crate::boot::scenario();
    Some(Admission {
        bar,
        drhd,
        phase2_media: scenario.starts_with(b"nvme-rw-"),
        retirement: match scenario {
            b"nvme-rw-qi-error" => Invalidation::InvalidContext,
            b"nvme-rw-qi-delay" => Invalidation::ReceiptDelayed,
            _ => Invalidation::Global,
        },
        source: match scenario {
            b"nvme-rw-dma-fault" => Some(false),
            b"nvme-rw-recover" | b"nvme-rw-rights" => None,
            name if name.starts_with(b"nvme-rw-") => Some(true),
            _ => None,
        },
        data_page: match scenario {
            b"nvme-permission" => DataPage::Withheld,
            b"nvme-permission-read" => DataPage::UntilRead,
            _ => DataPage::Mapped,
        },
    })
}

enum State {
    Parked,
    Prepared,
    Active,
    Quarantine,
    Finished,
}
/// Une instance physique encore fermée depuis sa préparation vérifiée.
pub(super) struct Unstarted<'a> {
    bar: &'a Bar,
    lease: Lease,
}
impl Unstarted<'_> {
    pub(super) fn confirmed(&self, lease: Lease) -> bool {
        self.lease == lease && self.bar.fenced() && self.bar.virgin()
    }
}
pub(crate) struct Domain {
    source: Option<bool>,
    phase2_media: bool,
    retirement: Invalidation,
    data_page: DataPage,
    bar: Bar,
    engine: ManuallyDrop<Engine>,
    lease: Option<Lease>,
    memory: Option<Memory>,
    state: State,
    reuse: Option<reuse::Probe>,
}
impl Domain {
    pub(crate) fn new(mut admission: Admission) -> Result<Self> {
        if !admission.bar.fenced() {
            return Err(Error::Denied);
        }
        if let Err(e) = admission.bar.map() {
            admission.bar.unmap()?;
            return Err(e);
        }
        if !admission.bar.virgin() || !admission.bar.fenced() {
            admission.bar.unmap()?;
            return Err(Error::Denied);
        }
        let engine = Engine::initialize(admission.drhd).inspect_err(|_| {
            let _ = admission.bar.unmap();
        })?;
        Ok(Self {
            data_page: admission.data_page,
            source: admission.source,
            phase2_media: admission.phase2_media,
            retirement: admission.retirement,
            bar: admission.bar,
            engine: ManuallyDrop::new(engine),
            lease: None,
            memory: None,
            state: State::Parked,
            reuse: None,
        })
    }

    pub(crate) fn arm_reuse_probe(&mut self) -> Result<()> {
        if self.reuse.is_some()
            || !matches!(self.state, State::Parked)
            || self.source != Some(true)
            || !self.engine.parked()
            || !self.bar.fenced()
        {
            return Err(Error::Denied);
        }
        self.reuse = Some(reuse::Probe::new());
        Ok(())
    }

    pub(crate) fn trace_owners(&self) -> Result<()> {
        let lease = self.lease.ok_or(Error::Denied)?;
        if !matches!(self.state, State::Prepared) || !self.engine.exact_nvme(lease, self.layout()) {
            return Err(Error::Denied);
        }
        for slot in 0..self.layout().pages() as u8 {
            let pa = self.engine.nvme_page_pa(lease, slot).ok_or(Error::Denied)?;
            log(format_args!(
                "NVME P3 bail epoque={} pid={} slot={slot} iova={:#x} gpa={pa:#x}",
                lease.epoch(),
                lease.owner(),
                lease.iova(slot)
            ));
        }
        Ok(())
    }

    pub(crate) fn witness_pages(&self) -> usize {
        self.reuse.as_ref().map_or(0, reuse::Probe::pages)
    }

    pub(crate) fn observe_parked_witnesses(&self) -> Result<()> {
        if !matches!(self.state, State::Parked)
            || self.lease.is_some()
            || self.memory.is_some()
            || !self.bar.fenced()
            || !self.engine.parked()
        {
            return Err(Error::Quarantine);
        }
        self.reuse.as_ref().ok_or(Error::Denied)?.observe(false)
    }

    pub(crate) fn observe_readback(&self, proof: &Drained<'_>) -> Result<()> {
        let lease = self.lease.ok_or(Error::Denied)?;
        let memory = self.memory.as_ref().ok_or(Error::Denied)?;
        if !matches!(self.state, State::Active)
            || self.source.is_some()
            || !self.bar.snapshot(self.bar.command | 4)
            || !self.dma_clean()
            || !self.engine.exact_nvme(lease, self.layout())
            || !proof.confirmed(memory)
        {
            return Err(Error::Quarantine);
        }
        self.reuse.as_ref().ok_or(Error::Denied)?.observe(true)
    }

    fn layout(&self) -> NvmeLayout {
        NvmeLayout {
            data: self.data_page.mapped(),
            source: self.source,
        }
    }

    pub(crate) fn prepare(&mut self, owner: u64) -> Result<Memory> {
        if !matches!(self.state, State::Parked) || !self.bar.fenced() || !self.bar.virgin() {
            return Err(Error::Denied);
        }
        let lease = self.engine.attach(owner)?;
        self.lease = Some(lease);
        self.state = State::Quarantine;
        let layout = self.layout();
        for slot in 0..layout.pages() as u8 {
            if let Err(e) = self.engine.map_nvme(lease, slot, layout) {
                self.engine.cancel_preparation(lease)?;
                self.lease = None;
                self.state = State::Parked;
                return Err(e);
            }
        }
        self.engine.publish(lease)?;
        if !self.engine.exact_nvme(lease, self.layout()) {
            return Err(Error::Quarantine);
        }
        let mut cpu = [0; 6];
        let mut iova = [0; 6];
        for (slot, address) in cpu[..layout.pages()].iter_mut().enumerate() {
            iova[slot] = lease.iova(slot as u8);
            *address = self
                .engine
                .page_address(lease, slot as u8, layout.mapped(slot))
                .ok_or(Error::Quarantine)?;
        }
        let memory = Memory {
            phase2_media: self.phase2_media,
            bar: Bar::va(0) as usize,
            len: 2 * PAGE,
            cpu,
            iova,
            epoch: lease.epoch(),
        };
        self.memory = Some(memory);
        self.state = State::Prepared;
        let (leaves, rights) = match (self.data_page.mapped(), self.source) {
            (true, Some(true)) => (6, "R,W,R,W,W,R"),
            (true, _) => (5, "R,W,R,W,W"),
            (false, _) => (4, "R,W,R,W"),
        };
        log(format_args!(
            "NVME VT-d SID={SID} DID=1 epoque={} feuilles={leaves} droits={rights} autres=0 service=8 BME=0",
            lease.epoch()
        ));
        if self.reuse.as_ref().is_some_and(|p| !p.first()) {
            self.trace_owners()?;
            self.reuse
                .as_mut()
                .unwrap()
                .reopen(&self.engine, lease, &memory)?;
        }
        Ok(memory)
    }
    pub(crate) fn cancel_prepared(&mut self) -> Result<()> {
        let _guard = Guard::new();
        if !matches!(self.state, State::Prepared) {
            return Err(Error::Denied);
        }
        self.state = State::Quarantine;
        let lease = self.lease.ok_or(Error::Denied)?;
        let memory = self.memory.as_ref().ok_or(Error::Denied)?;
        self.engine.close(lease)?;
        if !self.bar.clear_prepared(memory) {
            return Err(Error::Quarantine);
        }
        self.engine.retire_unstarted_nvme(
            lease,
            &Unstarted {
                bar: &self.bar,
                lease,
            },
        )?;
        if !self.engine.parked() {
            return Err(Error::Quarantine);
        }
        self.lease = None;
        self.memory = None;
        self.state = State::Parked;
        log(format_args!(
            "NVME preparation annulee epoque={} BME=0 WAIT=confirme pages_restituees={}",
            lease.epoch(),
            self.layout().pages()
        ));
        Ok(())
    }
    pub(crate) fn enable(&mut self) -> Result<()> {
        let _guard = Guard::new();
        let lease = self.lease.ok_or(Error::Denied)?;
        if !matches!(self.state, State::Prepared)
            || !self.engine.exact_nvme(lease, self.layout())
            || !self
                .memory
                .as_ref()
                .is_some_and(|m| self.bar.prepared_queues(m))
        {
            return Err(Error::Denied);
        }
        self.state = State::Quarantine;
        if !self.bar.master(true) {
            return Err(Error::Quarantine);
        }
        self.state = State::Active;
        Ok(())
    }
    /// Le canal de faute n’est jamais acquitté pendant un bail NVMe.
    /// Le superviseur sonde après la complétion et avant livraison au client.
    pub(crate) fn dma_clean(&self) -> bool {
        matches!(self.state, State::Active) && self.lease.is_some_and(|l| self.engine.clean_nvme(l))
    }

    /// Témoin en lecture seule. Aucune adresse, commande ou donnée ne vient
    /// du client. Le seul changement est de retirer la permission DATA. Le
    /// cadre témoin reste épinglé, comme les quatre pages des files.
    pub(crate) fn deny_read_data(&mut self, controller: &Controller) -> Result<()> {
        let _guard = Guard::new();
        let lease = self.lease.ok_or(Error::Denied)?;
        let memory = self.memory.as_ref().ok_or(Error::Denied)?;
        if self.data_page != DataPage::UntilRead
            || !self.dma_clean()
            || !controller.idle_before_read(memory)
        {
            return Err(Error::Denied);
        }
        // Aucune admission concurrente sous la garde. Une commande reçue par
        // IPC mais pas encore soumise reste privée pendant cette transition.
        self.state = State::Quarantine;
        if !self.bar.master(false) || !controller.idle_before_read(memory) {
            return Err(Error::Quarantine);
        }
        self.data_page = DataPage::Withheld;
        self.engine.remove_nvme_data(lease)?;
        if !controller.idle_before_read(memory) || !self.bar.master(true) {
            return Err(Error::Quarantine);
        }
        self.state = State::Active;
        log(format_args!(
            "NVME avant READ controleur_sans_commandes=true feuilles=4 DATA=absente contexte=confirme IOTLB=drain WAIT=confirme cadres_epingles=5"
        ));
        Ok(())
    }

    pub(crate) fn retire(&mut self, proof: &Drained<'_>) -> Result<()> {
        let _guard = Guard::new();
        let lease = self.lease.ok_or(Error::Denied)?;
        let memory = self.memory.as_ref().ok_or(Error::Denied)?;
        if !matches!(self.state, State::Active) {
            return Err(Error::Denied);
        }
        let first = self.reuse.as_ref().is_some_and(reuse::Probe::first);
        if let Some(probe) = self.reuse.as_mut().filter(|_| first) {
            probe.capture(&self.engine, lease, memory)?;
        }
        self.state = State::Quarantine;
        self.engine.close(lease)?;
        if !proof.confirmed(memory) || !self.bar.master(false) {
            return Err(Error::Quarantine);
        }
        self.engine
            .retire_nvme(lease, proof, memory, self.retirement, || self.bar.fenced())?;
        if !self.engine.parked() {
            return Err(Error::Quarantine);
        }
        self.lease = None;
        self.memory = None;
        self.state = State::Parked;
        log(format_args!(
            "NVME retrait epoque={} controleur=confirme BME=0 contexte=0 IOTLB=drain WAIT=confirme pages_restituees={}",
            lease.epoch(),
            self.layout().pages()
        ));
        if let Some(probe) = self.reuse.as_mut().filter(|_| first) {
            probe.reallocate()?;
            self.source = None; // La seconde incarnation utilise le profil RECOVER.
        }
        Ok(())
    }
    /// Issue terminale : garde les propriétaires et interdit toute réouverture.
    pub(crate) fn quarantine(&mut self) {
        if matches!(self.state, State::Finished) {
            return;
        }
        if let Some(lease) = self.lease {
            let _ = self.engine.close(lease);
        }
        self.state = State::Quarantine;
        let _guard = Guard::new();
        // Fermer le robinet PCI limite les nouvelles transactions. Même
        // confirmé, cet arrêt ne prouve pas que les transactions en vol sont finies.
        let fenced = self.bar.fenced() || self.bar.master(false);
        log(format_args!(
            "NVME quarantaine cadres={} BME={} fermeture_pci={fenced} readmission=false",
            self.engine.pinned(),
            cfg(4, 2) & 4 != 0
        ));
    }
    pub(crate) fn private_frames(&self) -> Option<[u64; 24]> {
        if !matches!(self.state, State::Prepared) {
            return None;
        }
        let mut frames = self.engine.nvme_frames(self.lease?, self.layout())?;
        if let Some(probe) = &self.reuse {
            probe.deny_cpu(&mut frames)?;
        }
        Some(frames)
    }
    pub(crate) fn refused_data_intact(&self) -> bool {
        !self.data_page.mapped()
            && matches!(self.state, State::Quarantine)
            && self.bar.fenced()
            && self
                .lease
                .is_some_and(|l| self.engine.rejected_nvme_data(l))
    }
    pub(crate) fn refused_write_source(&self, expected: &[u8]) -> bool {
        self.source == Some(false)
            && matches!(self.state, State::Quarantine)
            && self.bar.fenced()
            && self
                .lease
                .is_some_and(|l| self.engine.rejected_write_source(l, expected))
    }

    pub(crate) fn failed_invalidation(&self) -> bool {
        matches!(self.state, State::Quarantine)
            && self.bar.fenced()
            && self
                .lease
                .is_some_and(|l| self.engine.failed_nvme_invalidation(l, self.retirement))
    }

    pub(crate) fn pinned(&self) -> usize {
        self.engine.pinned()
    }
    pub(crate) fn finish(&mut self) -> Result<()> {
        if !matches!(self.state, State::Parked) || !self.engine.parked() || !self.bar.fenced() {
            return Err(Error::Quarantine);
        }
        if let Some(probe) = self.reuse.as_mut() {
            probe.finish()?;
        }
        self.bar.unmap()?;
        self.state = State::Finished;
        Ok(())
    }
}

/// Les huit cadres de service demeurent publiés dans le remappeur même sans
/// bail. Une sortie du banc conserve le propriétaire des tables et de tous
/// les cadres de quarantaine. CLAIMED ne permet qu'un seul Engine par machine.
static RETAINED: crate::sched::sync::Mutex<Option<(Engine, Option<reuse::Probe>)>> =
    crate::sched::sync::Mutex::new(None);
impl Drop for Domain {
    fn drop(&mut self) {
        if !matches!(
            self.state,
            State::Parked | State::Finished | State::Quarantine
        ) {
            self.quarantine();
        }
        if matches!(self.state, State::Parked) {
            let _ = self.finish();
        }
        let mut retained = RETAINED.lock();
        debug_assert!(retained.is_none());
        // Sûreté : unique déplacement dans Drop, aucun Drop implicite d'Engine.
        *retained = Some((
            unsafe { ManuallyDrop::take(&mut self.engine) },
            self.reuse.take(),
        ));
    }
}
