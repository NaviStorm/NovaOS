//! Instance privilégiée unique du DRHD Q35. Les pilotes ne reçoivent ni PA ni registre.
//! Intel VT-d, mode legacy de second niveau, QI 128 bits et fautes primaires.
use super::{
    block::io::{EMPTY, Frame, Region, barrier, wait},
    discovery::Drhd,
    edu_dma::{Device, Drained},
    physical_dma::Direction,
};
use crate::{hal, memory};
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

mod format;
use format::{DID, PAGE, SID, STRIDE};
pub(crate) const IOVA: u64 = format::BASE;
const TE: u32 = 1 << 31;
const SRTP: u32 = 1 << 30;
const QIE: u32 = 1 << 26;
const RUN: u32 = TE | QIE;
const FAULT_MASK: u32 = 1 << 31;
const IQE: u32 = 1 << 4;
const VECTOR: u32 = 0x53;
const RECORD_LIMIT: usize = 32;
const ANCHORS: usize = 8;
const SLOTS: usize = if cfg!(feature = "nvme-lab") { 6 } else { 2 };
const CTX: usize = SID as usize * 16;
const MMIO_SLOT: usize = 328;
const APIC_SLOT: usize = 329;
static CLAIMED: AtomicBool = AtomicBool::new(false);
static IRQ_READY: AtomicBool = AtomicBool::new(false);
static QUARANTINE: crate::sched::sync::Mutex<Option<Engine>> = crate::sched::sync::Mutex::new(None);
static FAULT_IRQS: AtomicU64 = AtomicU64::new(0);
static FAULT_CPU: AtomicU64 = AtomicU64::new(u64::MAX);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Error {
    Denied,
    Feature,
    Geometry,
    Memory,
    Fault,
    Quarantine,
}
type Result<T> = core::result::Result<T, Error>;

/// Capacite interne. Seule attach() sait la construire et elle ne traverse pas l'ABI.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Lease {
    owner: u64,
    epoch: u64,
}
impl Lease {
    pub(crate) fn owner(self) -> u64 {
        self.owner
    }
    pub(crate) fn epoch(self) -> u64 {
        self.epoch
    }
    pub(crate) fn iova(self, slot: u8) -> u64 {
        IOVA + (self.epoch - 1) * STRIDE + u64::from(slot) * PAGE
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub(crate) struct FrameStamp(u64);
impl core::fmt::Debug for FrameStamp {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("FrameStamp")
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Fault {
    pub sid: u16,
    /// Corrélation avec le contexte courant. Le FRCD legacy ne contient pas le DID.
    pub did: Option<u16>,
    pub iova: u64,
    pub reason: u8,
    pub read: bool,
    pub status: u32,
    pub irqs: u64,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    Empty,
    Preparing,
    Published,
    Closing,
    Invalidating,
}
#[derive(Clone, Copy, Eq, PartialEq)]
pub(super) enum Invalidation {
    Global,
    ReceiptDelayed,
    InvalidContext,
}
#[derive(Clone, Copy)]
struct Mapping {
    frame: Frame,
    direction: Direction,
}
impl Mapping {
    fn fill(self, byte: u8) {
        let value = u64::from_ne_bytes([byte; 8]);
        for offset in (0..PAGE as usize).step_by(8) {
            self.frame.mem.w64(offset, value);
        }
        barrier();
    }
}

/// Les fichiers de qualification ne fournissent aucune adresse physique. Une
/// entrée privée peut être épinglée sans PTE pour le témoin de faute DMA.
#[cfg(feature = "nvme-lab")]
#[derive(Clone, Copy)]
pub(super) struct NvmeLayout {
    pub(super) data: bool,
    pub(super) source: Option<bool>,
}
#[cfg(feature = "nvme-lab")]
impl NvmeLayout {
    pub(super) fn pages(self) -> usize {
        5 + usize::from(self.source.is_some())
    }

    pub(super) fn mapped(self, slot: usize) -> bool {
        match slot {
            0..=3 => true,
            4 => self.data,
            5 => self.source == Some(true),
            _ => false,
        }
    }
}

/// Nouvelle possession, par le superviseur, des deux cadres rendus à
/// l'allocateur. Aucun pointeur conservé de l'ancienne allocation n'est lu.
pub(crate) struct RetiredPages {
    frames: [Frame; 2],
    old: Lease,
}
impl RetiredPages {
    fn intact(&self) -> bool {
        self.frames.iter().zip([0xa1, 0xb2]).all(|(f, byte)| {
            let expected = u64::from_ne_bytes([byte; 8]);
            f.pa != EMPTY.pa
                && (0..PAGE as usize)
                    .step_by(8)
                    .all(|off| f.mem.r64(off) == expected)
        })
    }
    pub(crate) fn release(&mut self, device: &Drained<'_>) -> bool {
        if device.lease() != self.old || !device.confirmed() || !self.intact() {
            return false;
        }
        for frame in core::mem::replace(&mut self.frames, [EMPTY; 2]) {
            free(frame);
        }
        true
    }
}

/// Les huit cadres de service restent à la machine pendant toute sa durée de vie.
/// Une fois RTADDR publié aucune erreur, aucun Drop ne les restitue à l'allocateur.
pub(crate) struct Engine {
    regs: Region,
    service: [Frame; ANCHORS], // root, context, PML4, PDP, PD, PT, IQ, status WAIT.
    record: usize,
    cache_line: usize,
    lease: Option<Lease>, // conserve aussi l'identité de la dernière incarnation retirée
    state: State,
    pages: [Option<Mapping>; SLOTS],
    tail: u16,
    cookie: u32,
    in_queue: bool,
    consumed_irqs: u64,
    records: usize,
}

fn free(frame: Frame) {
    // Sûreté : appelants limités à un cadre jamais publié ou un retrait entièrement acquitté.
    unsafe { memory::release_dma(frame.pa) }
}

/// Avant RTADDR aucune table n’est connue du matériel. Les fenêtres temporaires
/// se retirent alors aussi lorsque l’unité ou la plateforme est refusée.
struct Windows {
    pa: u64,
    apic: bool,
    permanent: bool,
}
impl Windows {
    fn new(pa: u64) -> Self {
        Self {
            pa,
            apic: false,
            permanent: false,
        }
    }
}
impl Drop for Windows {
    fn drop(&mut self) {
        if self.permanent {
            return;
        }
        let va = |i: usize| memory::IO_BASE + i as u64 * PAGE;
        let apic_ok = !self.apic || memory::unmap_lab_io(0xfee0_0000, va(APIC_SLOT)).is_ok();
        let unit_ok = memory::unmap_lab_io(self.pa, va(MMIO_SLOT)).is_ok();
        if !apic_ok || !unit_ok {
            hal::log(format_args!(
                "VTD retrait MMIO incomplet apic={apic_ok} unite={unit_ok}"
            ));
        }
    }
}

impl Engine {
    pub(crate) fn initialize(drhd: Drhd) -> Result<Self> {
        if drhd.base() != 0xfed9_0000 || drhd.sid() != SID || crate::smp::index() != 0 {
            return Err(Error::Denied);
        }
        if CLAIMED.swap(true, Ordering::AcqRel) {
            return Err(Error::Denied);
        }
        let regs = Region::map(drhd.base(), MMIO_SLOT, PAGE as usize).map_err(|_| Error::Memory)?;
        let mut windows = Windows::new(drhd.base());
        let version = regs.r32(0);
        let cap = regs.r64(8);
        let ecap = regs.r64(16);
        hal::log(format_args!(
            "VTD registre version={version:#x} CAP={cap:#x} ECAP={ecap:#x} GSTS={:#x}",
            regs.r32(0x1c)
        ));
        let record =
            format::capabilities(version, cap, ecap, drhd.host_bits()).ok_or(Error::Feature)?;
        if regs.r32(0x1c) != 0
            || regs.r64(0x20) != 0
            || regs.r64(0x80) != 0
            || regs.r64(0x88) != 0
            || regs.r64(0x90) != 0
            || regs.r32(0x34) != 0
            || regs.r64(record + 8) != 0
            || regs.r32(0x38) != FAULT_MASK
        {
            return Err(Error::Denied);
        }

        // Q35 avec xAPIC local, BSP destination de la seule MSI de faute de l'unité.
        let (lo, hi): (u32, u32);
        unsafe {
            core::arch::asm!("rdmsr",in("ecx") 0x1bu32,out("eax")lo,out("edx")hi, options(nostack));
        }
        if hi != 0 || lo & !0xfff != 0xfee0_0000 || lo & 0xc00 != 0x800 {
            return Err(Error::Feature);
        }
        let apic = Region::map(0xfee0_0000, APIC_SLOT, PAGE as usize).map_err(|_| Error::Memory)?;
        windows.apic = true;
        if apic.r32(0x20) >> 24 != 0 || crate::arch::cpu_id() != 0 {
            return Err(Error::Denied);
        }
        // ECAP.C=0 sur Q35 11.1.3. Les tables ne sont pas contractuellement
        // snoopées. La file QI et le sémaphore WAIT sont toujours cohérents.
        let cpuid = core::arch::x86_64::__cpuid(1);
        let cache_line = if ecap & 1 != 0 {
            0
        } else {
            let line = ((cpuid.ebx >> 8) & 255) as usize * 8;
            if cpuid.edx & (1 << 19) == 0 || !line.is_power_of_two() || line > PAGE as usize {
                return Err(Error::Feature);
            }
            line
        };
        let mut service = [EMPTY; ANCHORS];
        for i in 0..ANCHORS {
            let frame = match Frame::allocate() {
                Ok(f) if f.pa != 0 && f.pa < 1 << 48 && f.pa & (PAGE - 1) == 0 => f,
                other => {
                    if let Ok(f) = other {
                        free(f);
                    }
                    for &f in &service[..i] {
                        free(f);
                    }
                    return Err(Error::Memory);
                }
            };
            service[i] = frame;
        }
        let mut unit = Self {
            regs,
            service,
            record,
            cache_line,
            state: State::Empty,
            lease: None,
            pages: [None; SLOTS],
            tail: 0,
            cookie: 0,
            in_queue: false,
            consumed_irqs: 0,
            records: 0,
        };
        windows.permanent = true;
        if let Err(err) = unit.activate(apic) {
            hal::log(format_args!(
                "VTD initialisation partielle GSTS={:#x} FSTS={:#x} cadres_service={ANCHORS} quarantaine=true",
                regs.r32(0x1c),
                regs.r32(0x34)
            ));
            *QUARANTINE.lock() = Some(unit);
            return Err(err);
        }
        Ok(unit)
    }
    fn activate(&mut self, apic: Region) -> Result<()> {
        let service = self.service;
        let regs = self.regs;
        let record = self.record;
        service[0].mem.w64(0, service[1].pa | 1); // bus 0, tous les contextes fermés
        self.publish_tables();
        regs.w64(0x20, service[0].pa); // TTM=0 : root-table legacy
        regs.w32(0x18, SRTP);
        wait(|| regs.r32(0x1c) == SRTP).map_err(|_| Error::Quarantine)?;
        regs.w64(0x90, service[6].pa); // file vide 4 Kio, descripteurs de 16 octets
        if regs.r64(0x20) != service[0].pa {
            return Err(Error::Quarantine);
        }
        regs.w32(0x18, QIE);
        wait(|| regs.r32(0x1c) == SRTP | QIE).map_err(|_| Error::Quarantine)?;
        // IQA donne la base active à partir du passage de QIES à 1 dans QEMU.
        if regs.r64(0x90) != service[6].pa {
            return Err(Error::Quarantine);
        }
        self.submit_invalidation(Invalidation::Global)?;
        self.confirm()?;
        regs.w32(0x18, RUN);
        wait(|| regs.r32(0x1c) == SRTP | RUN).map_err(|_| Error::Quarantine)?;
        let spurious = apic.r32(0xf0);
        apic.w32(0xf0, spurious | 0x1ff);
        if apic.r32(0xf0) != spurious | 0x1ff {
            return Err(Error::Quarantine);
        }
        regs.w32(0x40, 0xfee0_0000);
        regs.w32(0x44, 0);
        regs.w32(0x3c, VECTOR);
        if regs.r32(0x40) != 0xfee0_0000 || regs.r32(0x44) != 0 || regs.r32(0x3c) != VECTOR {
            return Err(Error::Quarantine);
        }
        IRQ_READY.store(true, Ordering::Release);
        regs.w32(0x38, 0);
        if regs.r32(0x38) != 0 || !self.parked() {
            return Err(Error::Quarantine);
        }
        hal::log(format_args!(
            "VTD actif mode=legacy niveaux=4 DID={DID} SID={SID} service={ANCHORS} GSTS={:#x} FRCD={record:#x}",
            regs.r32(0x1c)
        ));
        Ok(())
    }
    fn running(&self) -> bool {
        self.regs.r32(0x1c) == SRTP | RUN
            && self.regs.r64(0x20) == self.service[0].pa
            && self.regs.r64(0x90) == self.service[6].pa
    }
    fn context(&self) -> bool {
        self.service[1].mem.r64(CTX) == self.service[2].pa | 1
            && self.service[1].mem.r64(CTX + 8) == (u64::from(DID) << 8) | 2
    }
    pub(crate) fn parked(&self) -> bool {
        self.running()
            && self.state == State::Empty
            && !self.in_queue
            && self.pinned() == 0
            && self.service[1].mem.r64(CTX) == 0
            && self.service[1].mem.r64(CTX + 8) == 0
            && !self.fault_pending()
    }
    pub(crate) fn denies_edu(&self, last: Lease) -> bool {
        self.lease == Some(last) && self.parked()
    }
    fn publish_tables(&self) {
        if self.cache_line != 0 {
            for table in &self.service[..6] {
                for offset in (0..PAGE as usize).step_by(self.cache_line) {
                    // Sûreté : CPUID confirme CLFLUSH, cadre épinglé et HHDM valide.
                    unsafe {
                        core::arch::x86_64::_mm_clflush((table.mem.base() + offset) as *const u8);
                    }
                }
            }
        }
        barrier();
    }
    fn submit_invalidation(&mut self, mode: Invalidation) -> Result<()> {
        if self.in_queue
            || self.regs.r64(0x80) != u64::from(self.tail)
            || self.regs.r32(0x34) & !2 != 0
        {
            return Err(Error::Quarantine);
        }
        self.publish_tables();
        self.cookie = self.cookie.checked_add(1).ok_or(Error::Quarantine)?;
        let receipt = self.service[7];
        receipt.mem.w32(0, 0);
        // Invalidation contexte globale puis IOTLB globale avec DR et DW, puis WAIT avec SW.
        // Dans le seul témoin de faute, G=0 est réservé pour le descripteur de contexte.
        let context = if mode == Invalidation::InvalidContext {
            0x01
        } else {
            0x11
        };
        for (lo, hi) in [
            (context, 0),
            (0xd2, 0),
            ((u64::from(self.cookie) << 32) | 0x25, receipt.pa),
        ] {
            let off = usize::from(self.tail);
            self.service[6].mem.w64(off, lo);
            self.service[6].mem.w64(off + 8, hi);
            self.tail = (self.tail + 16) % 4096;
        }
        barrier();
        self.in_queue = true;
        self.regs.w64(0x88, u64::from(self.tail));
        Ok(())
    }
    fn confirm(&mut self) -> Result<()> {
        if !self.in_queue {
            return Err(Error::Quarantine);
        }
        wait(|| {
            (self.service[7].mem.r32(0) == self.cookie
                && self.regs.r64(0x80) == u64::from(self.tail))
                || self.regs.r32(0x34) & !2 != 0
        })
        .map_err(|_| Error::Quarantine)?;
        if self.regs.r32(0x34) & !2 != 0
            || self.regs.r64(0x80) != u64::from(self.tail)
            || self.service[7].mem.r32(0) != self.cookie
        {
            return Err(Error::Quarantine);
        }
        self.in_queue = false;
        Ok(())
    }
    pub(crate) fn attach(&mut self, owner: u64) -> Result<Lease> {
        if owner == 0 || !self.parked() {
            return Err(Error::Denied);
        }
        let epoch = format::next_epoch(self.lease.map_or(0, |l| l.epoch)).ok_or(Error::Denied)?;
        let lease = Lease { owner, epoch };
        self.lease = Some(lease);
        self.state = State::Preparing;
        // Les pages d’un bail tiennent dans la même feuille de niveau 1.
        let base = lease.iova(0);
        for (i, shift) in [39, 30, 21].into_iter().enumerate() {
            self.service[2 + i].mem.w64(
                ((base >> shift) & 511) as usize * 8,
                self.service[3 + i].pa | 3,
            );
        }
        Ok(lease)
    }
    pub(crate) fn map(&mut self, lease: Lease, slot: u8, dir: Direction) -> Result<()> {
        if self.state != State::Preparing || self.lease != Some(lease) {
            return Err(Error::Denied);
        }
        if !matches!(
            (slot, dir),
            (0, Direction::ReadByDevice) | (1, Direction::WrittenByDevice)
        ) {
            return Err(Error::Geometry);
        }
        self.map_private(lease, slot, dir, true)
    }

    #[cfg(feature = "nvme-lab")]
    pub(super) fn map_nvme(&mut self, lease: Lease, slot: u8, layout: NvmeLayout) -> Result<()> {
        use Direction::*;
        let dir = match slot {
            0 | 2 => ReadByDevice,                        // ASQ, IOSQ
            5 if layout.source.is_some() => ReadByDevice, // source WRITE immuable
            1 | 3 | 4 => WrittenByDevice,                 // ACQ, IOCQ, données
            _ => return Err(Error::Geometry),
        };
        if self.state != State::Preparing || self.lease != Some(lease) {
            return Err(Error::Denied);
        }
        self.map_private(lease, slot, dir, layout.mapped(usize::from(slot)))
    }

    #[cfg(feature = "nvme-lab")]
    pub(super) fn page_address(&self, lease: Lease, slot: u8, allowed: bool) -> Option<usize> {
        if !self.active(lease) || self.mapped_iova(lease, lease.iova(slot))? != allowed {
            return None;
        }
        Some(self.mapping(lease, slot)?.frame.mem.base())
    }

    /// Vérification positive et négative des tables réellement publiées. Le
    /// superviseur ne suit que les huit pages dont il est propriétaire.
    #[cfg(feature = "nvme-lab")]
    pub(super) fn exact_nvme(&self, lease: Lease, layout: NvmeLayout) -> bool {
        if !self.active(lease) || self.pinned() != layout.pages() {
            return false;
        }
        for (level, frame) in self.service[..6].iter().enumerate() {
            for index in 0..512 {
                let expected = match level {
                    0 if index == 0 => self.service[1].pa | 1,
                    1 if index == CTX / 8 => self.service[2].pa | 1,
                    1 if index == CTX / 8 + 1 => (u64::from(DID) << 8) | 2,
                    2..=4
                        if index == ((lease.iova(0) >> [39, 30, 21][level - 2]) & 511) as usize =>
                    {
                        self.service[level + 1].pa | 3
                    }
                    5 => {
                        let first = (lease.iova(0) >> 12) as usize & 511;
                        if let Some(slot) = index
                            .checked_sub(first)
                            .filter(|&slot| slot < layout.pages())
                        {
                            let Some(mapping) = self.pages[slot] else {
                                return false;
                            };
                            if !layout.mapped(slot) {
                                0
                            } else {
                                mapping.frame.pa | if matches!(slot, 0 | 2 | 5) { 1 } else { 2 }
                            }
                        } else {
                            0
                        }
                    }
                    _ => 0,
                };
                if frame.mem.r64(index * 8) != expected {
                    return false;
                }
            }
        }
        true
    }

    #[cfg(feature = "nvme-lab")]
    pub(super) fn nvme_frames(&self, lease: Lease, layout: NvmeLayout) -> Option<[u64; 24]> {
        if !self.active(lease) || self.pinned() != layout.pages() {
            return None;
        }
        let mut frames = [0; 24];
        for (dest, service) in frames.iter_mut().zip(self.service.iter()) {
            *dest = service.pa;
        }
        for (slot, dest) in frames[ANCHORS..ANCHORS + layout.pages()]
            .iter_mut()
            .enumerate()
        {
            *dest = self.mapping(lease, slot as u8)?.frame.pa;
        }
        Some(frames)
    }

    /// Un dépassement de file, un FRCD valide, un masque dû au handler ou
    /// un reçu QI incertain suspendent toutes les livraisons de ce bail.
    /// Aucun de ces registres n'est modifié par l'observation.
    #[cfg(feature = "nvme-lab")]
    pub(super) fn clean_nvme(&self, lease: Lease) -> bool {
        self.active(lease)
            && self.regs.r32(0x38) == 0
            && FAULT_IRQS.load(Ordering::Acquire) == 0
            && self.regs.r64(0x80) == u64::from(self.tail)
            && self.regs.r64(0x88) == u64::from(self.tail)
            && self.service[7].mem.r32(0) == self.cookie
            && !self.fault_pending()
    }

    #[cfg(feature = "nvme-lab")]
    pub(super) fn remove_nvme_data(&mut self, lease: Lease) -> Result<()> {
        if !self.exact_nvme(
            lease,
            NvmeLayout {
                data: true,
                source: None,
            },
        ) || !self.clean_nvme(lease)
        {
            return Err(Error::Denied);
        }
        let offset = ((lease.iova(4) >> 12) as usize & 511) * 8;
        self.service[5].mem.w64(offset, 0);
        self.resync(lease)?;
        if !self.exact_nvme(
            lease,
            NvmeLayout {
                data: false,
                source: None,
            },
        ) || !self.clean_nvme(lease)
        {
            return Err(Error::Quarantine);
        }
        Ok(())
    }

    /// Conserve le défaut original du DRHD et la preuve RAM, sans acquitter
    /// la faute. Le banc QMP relit le FRCD, les tables et le cadre témoin.
    #[cfg(feature = "nvme-lab")]
    pub(super) fn rejected_nvme_data(&self, lease: Lease) -> bool {
        if self.lease != Some(lease)
            || self.state != State::Closing
            || !self.context()
            || self.pinned() != 5
            || !self.running()
        {
            return false;
        }
        let page = match self.mapping(lease, 4) {
            Some(m) => m.frame,
            None => return false,
        };
        let iova = lease.iova(4);
        let pte = self.service[5].mem.r64(((iova >> 12) as usize & 511) * 8);
        let Some(fault) = format::record(
            self.regs.r32(0x34),
            self.regs.r64(self.record),
            self.regs.r64(self.record + 8),
        ) else {
            return false;
        };
        let intact = self.matches(lease, 4, 0);
        hal::log(format_args!(
            "NVME refus DMA SID={} faute={} lecture={} iova={:#x} cadre_temoin={:#x} pte={pte} zero={intact}",
            fault.sid, fault.reason, fault.read, fault.iova, page.pa
        ));
        pte == 0
            && fault.sid == SID
            && fault.iova == iova
            && !fault.read
            && fault.reason == 5
            && intact
    }

    #[cfg(feature = "nvme-lab")]
    pub(super) fn rejected_write_source(&self, lease: Lease, expected: &[u8]) -> bool {
        if self.lease != Some(lease)
            || self.state != State::Closing
            || !self.context()
            || self.pinned() != 6
            || !self.running()
            || expected.is_empty()
            || expected.len() > 4096
        {
            return false;
        }
        let Some(mapping) = self.mapping(lease, 5) else {
            return false;
        };
        let page = mapping.frame;
        let iova = lease.iova(5);
        let pte = self.service[5].mem.r64(((iova >> 12) as usize & 511) * 8);
        let Some(fault) = format::record(
            self.regs.r32(0x34),
            self.regs.r64(self.record),
            self.regs.r64(self.record + 8),
        ) else {
            return false;
        };
        barrier();
        let intact = (0..4096).all(|i| page.mem.r8(i) == expected.get(i).copied().unwrap_or(0));
        hal::log(format_args!(
            "NVME refus DMA WRITE SID={} faute={} lecture={} iova={:#x} cadre_temoin={:#x} pte={pte} source_intacte={intact}",
            fault.sid, fault.reason, fault.read, fault.iova, page.pa
        ));
        pte == 0
            && fault.sid == SID
            && fault.iova == iova
            && fault.read
            && fault.reason == 6
            && intact
    }

    fn map_private(&mut self, lease: Lease, slot: u8, dir: Direction, install: bool) -> Result<()> {
        let entry = self
            .pages
            .get_mut(usize::from(slot))
            .ok_or(Error::Geometry)?;
        if entry.is_some() {
            return Err(Error::Denied);
        }
        let frame = Frame::allocate().map_err(|_| Error::Memory)?;
        if frame.pa == 0 || frame.pa >= 1 << 48 || frame.pa & (PAGE - 1) != 0 {
            free(frame);
            return Err(Error::Geometry);
        }
        let access = match dir {
            Direction::ReadByDevice => 1,
            Direction::WrittenByDevice => 2,
        };
        *entry = Some(Mapping {
            frame,
            direction: dir,
        });
        let offset = ((lease.iova(slot) >> 12) & 511) as usize * 8;
        if install {
            self.service[5].mem.w64(offset, frame.pa | access);
        }
        Ok(())
    }
    pub(crate) fn publish(&mut self, lease: Lease) -> Result<()> {
        if self.state != State::Preparing
            || self.lease != Some(lease)
            || self.pinned() == 0
            || !self.running()
            || self.fault_pending()
        {
            return Err(Error::Denied);
        }
        // Garder la responsabilité même après une publication partielle.
        self.state = State::Closing;
        self.service[1].mem.w64(CTX + 8, (u64::from(DID) << 8) | 2);
        barrier();
        self.service[1].mem.w64(CTX, self.service[2].pa | 1);
        self.submit_invalidation(Invalidation::Global)?;
        self.confirm()?;
        if !self.context() || !self.running() || self.fault_pending() {
            return Err(Error::Quarantine);
        }
        self.state = State::Published;
        Ok(())
    }
    pub(crate) fn active(&self, lease: Lease) -> bool {
        self.lease == Some(lease)
            && self.state == State::Published
            && !self.in_queue
            && self.running()
            && self.context()
            && !self.fault_pending()
    }
    /// Témoin du banc à cache froid : invalider les traductions en conservant
    /// le même domaine, les mêmes feuilles et leurs permissions exactes.
    pub(crate) fn resync(&mut self, lease: Lease) -> Result<()> {
        if self.lease != Some(lease) || self.state != State::Published {
            return Err(Error::Denied);
        }
        self.state = State::Closing;
        if !self.running() || !self.context() || self.fault_pending() {
            return Err(Error::Quarantine);
        }
        self.submit_invalidation(Invalidation::Global)?;
        self.confirm()?;
        if !self.running() || !self.context() || self.fault_pending() {
            return Err(Error::Quarantine);
        }
        self.state = State::Published;
        Ok(())
    }
    /// Marche de table confinée aux quatre pages de service connues. Une entrée
    /// inattendue donne un diagnostic indéterminé. Aucune adresse lue dans les
    /// descripteurs n'est utilisée comme pointeur CPU.
    pub(crate) fn mapped_iova(&self, lease: Lease, iova: u64) -> Option<bool> {
        if !self.active(lease) || iova >= (1 << 28) || !iova.is_multiple_of(PAGE) {
            return None;
        }
        for (index, shift) in [39, 30, 21].into_iter().enumerate() {
            let offset = ((iova >> shift) & 511) as usize * 8;
            let entry = self.service[2 + index].mem.r64(offset);
            if entry == 0 {
                return Some(false);
            }
            if entry != self.service[3 + index].pa | 3 {
                return None;
            }
        }
        let entry = self.service[5].mem.r64(((iova >> 12) & 511) as usize * 8);
        if entry == 0 {
            return Some(false);
        }
        for (slot, mapping) in self.pages.iter().enumerate() {
            if lease.iova(slot as u8) == iova {
                let page = mapping.as_ref()?;
                let rights = match page.direction {
                    Direction::ReadByDevice => 1,
                    Direction::WrittenByDevice => 2,
                };
                return (entry == page.frame.pa | rights).then_some(true);
            }
        }
        None
    }
    pub(crate) fn guard_reused(&self, old: Lease, stamps: [FrameStamp; 2]) -> Result<RetiredPages> {
        if !self.denies_edu(old) || stamps[0] == stamps[1] {
            return Err(Error::Denied);
        }
        let first = Frame::allocate().map_err(|_| Error::Memory)?;
        let second = Frame::allocate().map_err(|_| {
            free(first);
            Error::Memory
        })?;
        // Si l'allocateur contredit l'exclusivité, garder le cadre épinglé.
        if first.pa == second.pa {
            return Err(Error::Quarantine);
        }
        let frames = [first, second];
        if !frames.iter().all(|f| {
            stamps.contains(&FrameStamp(f.pa))
                && (0..PAGE as usize).step_by(8).all(|off| f.mem.r64(off) == 0)
        }) {
            frames.into_iter().for_each(free);
            return Err(Error::Denied);
        }
        for (f, byte) in frames.iter().zip([0xa1, 0xb2]) {
            let value = u64::from_ne_bytes([byte; 8]);
            for offset in (0..PAGE as usize).step_by(8) {
                f.mem.w64(offset, value);
            }
        }
        barrier();
        Ok(RetiredPages { frames, old })
    }
    pub(crate) fn permits(&self, lease: Lease, slot: u8, dir: Direction) -> bool {
        self.active(lease)
            && self
                .mapping(lease, slot)
                .is_some_and(|p| p.direction == dir)
    }
    fn mapping(&self, lease: Lease, slot: u8) -> Option<Mapping> {
        if self.lease != Some(lease) {
            return None;
        }
        self.pages.get(usize::from(slot)).copied().flatten()
    }
    pub(crate) fn fill(&mut self, lease: Lease, slot: u8, byte: u8) -> Result<()> {
        if self.state != State::Preparing {
            return Err(Error::Denied);
        }
        self.mapping(lease, slot).ok_or(Error::Denied)?.fill(byte);
        Ok(())
    }
    /// Le transfert READ a été acquitté : changer le motif RAM doit rendre
    /// observable une éventuelle écriture à tort sur la même page RO.
    pub(crate) fn replace_read_source(&mut self, lease: Lease, byte: u8, device: &Device) -> bool {
        if !self.permits(lease, 0, Direction::ReadByDevice) || !device.can_reseed(lease) {
            return false;
        }
        let Some(source) = self.mapping(lease, 0) else {
            return false;
        };
        source.fill(byte);
        true
    }
    pub(crate) fn matches(&self, lease: Lease, slot: u8, byte: u8) -> bool {
        let Some(mapping) = self.mapping(lease, slot) else {
            return false;
        };
        let value = u64::from_ne_bytes([byte; 8]);
        barrier();
        (0..4096)
            .step_by(8)
            .all(|i| mapping.frame.mem.r64(i) == value)
    }
    pub(crate) fn frame_stamp(&self, lease: Lease, slot: u8) -> Option<FrameStamp> {
        self.mapping(lease, slot).map(|p| FrameStamp(p.frame.pa))
    }
    pub(crate) fn pinned(&self) -> usize {
        self.pages.iter().flatten().count()
    }
    pub(crate) fn close(&mut self, lease: Lease) -> Result<()> {
        if self.lease != Some(lease) || self.state == State::Empty {
            return Err(Error::Denied);
        }
        if self.state != State::Invalidating {
            self.state = State::Closing;
        }
        Ok(())
    }
    /// Annule une préparation faillie avant que le SID ait jamais pu utiliser
    /// ces pages. Dès la première publication, retire() exige une preuve EDU.
    pub(crate) fn cancel_preparation(&mut self, lease: Lease) -> Result<()> {
        if self.lease != Some(lease) || self.state != State::Preparing {
            return Err(Error::Denied);
        }
        if !self.running()
            || self.in_queue
            || self.fault_pending()
            || self.service[1].mem.r64(CTX) != 0
            || self.service[1].mem.r64(CTX + 8) != 0
        {
            self.state = State::Closing;
            return Err(Error::Quarantine);
        }
        self.clear_tables();
        self.release_pages();
        self.state = State::Empty;
        Ok(())
    }
    fn clear_tables(&self) {
        for frame in &self.service[2..=5] {
            for offset in (0..4096).step_by(8) {
                frame.mem.w64(offset, 0);
            }
        }
    }
    fn release_pages(&mut self) {
        for page in &mut self.pages {
            if let Some(mapping) = page.take() {
                free(mapping.frame);
            }
        }
    }
    pub(crate) fn retire(&mut self, lease: Lease, device: &Drained<'_>) -> Result<()> {
        self.retirement(lease, device, Invalidation::Global)
    }
    /// Injection de perte d'observation du WAIT. La commande matérielle est réelle.
    pub(crate) fn retire_delayed(&mut self, lease: Lease, device: &Drained<'_>) -> Result<()> {
        self.retirement(lease, device, Invalidation::ReceiptDelayed)
    }
    /// Essai terminal après drainage : faire refuser une commande par le DRHD.
    pub(crate) fn retire_invalid(&mut self, lease: Lease, device: &Drained<'_>) -> Result<()> {
        if self.state != State::Closing || self.in_queue || self.pinned() != 2 {
            return Err(Error::Denied);
        }
        self.retirement(lease, device, Invalidation::InvalidContext)
    }
    fn retirement(&mut self, lease: Lease, device: &Drained<'_>, mode: Invalidation) -> Result<()> {
        if device.lease() != lease {
            return Err(Error::Denied);
        }
        self.retire_confirmed(lease, mode, || device.confirmed())
    }

    #[cfg(feature = "nvme-lab")]
    pub(super) fn retire_nvme(
        &mut self,
        lease: Lease,
        drained: &super::nvme::Drained<'_>,
        memory: &super::nvme::Memory,
        mode: Invalidation,
        pci_fenced: impl Fn() -> bool,
    ) -> Result<()> {
        self.retire_confirmed(lease, mode, || pci_fenced() && drained.confirmed(memory))
    }

    #[cfg(feature = "nvme-lab")]
    pub(super) fn retire_unstarted_nvme(
        &mut self,
        lease: Lease,
        never: &super::nvme_domain::Unstarted<'_>,
    ) -> Result<()> {
        self.retire_confirmed(lease, Invalidation::Global, || never.confirmed(lease))
    }

    fn retire_confirmed(
        &mut self,
        lease: Lease,
        mode: Invalidation,
        confirmed: impl Fn() -> bool,
    ) -> Result<()> {
        if self.lease != Some(lease) || !matches!(self.state, State::Closing | State::Invalidating)
        {
            return Err(Error::Denied);
        }
        if !confirmed() || !self.running() || self.fault_pending() {
            return Err(Error::Quarantine);
        }
        if self.state == State::Closing {
            if self.in_queue {
                self.confirm()?;
            } // publication préalable interrompue
            self.service[1].mem.w64(CTX, 0);
            barrier();
            self.service[1].mem.w64(CTX + 8, 0);
            // Les quatre niveaux sont privés de l'ancienne incarnation. Éliminer aussi
            // les pointeurs des chemins intermédiaires avant la prochaine fenêtre IOVA.
            self.clear_tables();
            self.submit_invalidation(mode)?;
            self.state = State::Invalidating;
        }
        if mode == Invalidation::ReceiptDelayed {
            return Err(Error::Quarantine);
        }
        self.confirm()?;
        // Défense du banc si le DRHD acceptait la commande réservée : conserver
        // les cadres. L'oracle constatera l'absence d'IQE et le reçu inattendu.
        if mode == Invalidation::InvalidContext {
            return Err(Error::Quarantine);
        }
        if !self.running()
            || self.fault_pending()
            || !confirmed()
            || self.service[1].mem.r64(CTX) != 0
            || self.service[1].mem.r64(CTX + 8) != 0
        {
            self.state = State::Closing; // une reprise doit obtenir une nouvelle clôture d'invalidation
            return Err(Error::Quarantine);
        }
        self.release_pages();
        self.state = State::Empty;
        Ok(())
    }
    #[cfg(feature = "nvme-lab")]
    pub(super) fn failed_nvme_invalidation(&self, lease: Lease, mode: Invalidation) -> bool {
        if !self.invalidation_holds(lease, 6) {
            return false;
        }
        // Dans le témoin WAIT, le DRHD peut déjà avoir écrit le cookie.
        // L'injection a sauté l'observation, les cadres restent à l'ancien bail.
        match mode {
            Invalidation::InvalidContext => self.queue_error(),
            Invalidation::ReceiptDelayed => {
                self.regs.r32(0x34) == 0 && self.regs.r64(0x88) == u64::from(self.tail)
            }
            _ => false,
        }
    }
    pub(crate) fn queue_error_holds(&self, lease: Lease) -> bool {
        self.invalidation_holds(lease, 2) && self.queue_error()
    }
    fn invalidation_holds(&self, lease: Lease, pages: usize) -> bool {
        self.lease == Some(lease)
            && self.state == State::Invalidating
            && self.in_queue
            && self.running()
            && self.pinned() == pages
            && self.service[1].mem.r64(CTX) == 0
            && self.service[1].mem.r64(CTX + 8) == 0
    }
    fn queue_error(&self) -> bool {
        let start = (self.tail + 4096 - 48) % 4096;
        self.regs.r32(0x34) == IQE
            && self.regs.r64(self.record + 8) >> 63 == 0
            && self.regs.r64(0x80) == u64::from(start)
            && self.regs.r64(0x88) == u64::from(self.tail)
            && self.service[7].mem.r32(0) == 0
    }
    /// Instantané en lecture seule pour la console du superviseur. Les bits du
    /// FRCD n'ont de sens que si F est positionné. Un IRQ peut arriver entre
    /// deux lectures avant la collecte. Le masque et les acquittements restent
    /// exclusivement gérés par fault_irq() et take_fault().
    pub(crate) fn trace_fault(&self, lease: Lease, stage: &str) {
        let status = self.regs.r32(0x34);
        let high = self.regs.r64(self.record + 8);
        let low = self.regs.r64(self.record);
        let control = self.regs.r32(0x38);
        let count = FAULT_IRQS.load(Ordering::Acquire);
        let irq_cpu = FAULT_CPU.load(Ordering::Relaxed) as i64; // -1 avant la première IRQ
        let cpu = crate::smp::index();
        let head = self.regs.r64(0x80);
        let tail = self.regs.r64(0x88);
        let cookie = self.service[7].mem.r32(0);
        hal::log(format_args!(
            "VTD diagnostic epoque={} etape={stage} fsts={status:#x} fr_hi={high:#x} fr_lo={low:#x} fectl={control:#x} irq={count} cpu_irq={irq_cpu} cpu={cpu} iqh={head:#x} iqt={tail:#x} wait={cookie}",
            lease.epoch()
        ));
    }
    pub(crate) fn fault_pending(&self) -> bool {
        self.regs.r32(0x34) != 0 || self.regs.r64(self.record + 8) >> 63 != 0
    }
    pub(crate) fn take_fault(&mut self, lease: Lease) -> Result<Option<Fault>> {
        if self.lease != Some(lease) {
            return Err(Error::Denied);
        }
        let status = self.regs.r32(0x34);
        let hi = self.regs.r64(self.record + 8);
        if status == 0 && hi >> 63 == 0 {
            return Ok(None);
        }
        let installed = self.context();
        if self.state == State::Published {
            self.state = State::Closing;
        }
        let irqs = FAULT_IRQS.load(Ordering::Acquire);
        let record = format::record(status, self.regs.r64(self.record), hi).ok_or(Error::Fault)?;
        if record.sid != SID || !matches!(record.reason, 2 | 5 | 6) || self.records == RECORD_LIMIT
        {
            return Err(Error::Fault); // conserver le FRCD et toute ressource encore épinglée
        }
        if irqs == self.consumed_irqs {
            return Ok(None);
        }
        if irqs != self.consumed_irqs + 1 || self.regs.r32(0x38) & FAULT_MASK == 0 {
            return Err(Error::Fault);
        }
        self.trace_fault(lease, "avant-ack");
        self.consumed_irqs = irqs;
        self.records += 1;
        let fault = Fault {
            sid: record.sid,
            did: installed.then_some(DID),
            iova: record.iova,
            read: record.read,
            reason: record.reason,
            status,
            irqs,
        };
        // F, RW1C. Une erreur de file ou un deuxième FRCD n'est jamais effacé ici.
        self.regs.w32(self.record + 12, 1 << 31);
        if self.fault_pending() {
            return Err(Error::Quarantine);
        }
        self.regs.w32(0x38, 0);
        if self.regs.r32(0x38) != 0 {
            return Err(Error::Quarantine);
        }
        self.trace_fault(lease, "apres-ack");
        Ok(Some(fault))
    }
}

/// Handler indépendant de la garde SMP d'un appelant. Retient le FRCD dans l'unité
/// et masque la source jusqu'à collecte privilégiée. Aucun diagnostic livré au pilote.
pub(crate) extern "C" fn fault_irq() {
    if !IRQ_READY.load(Ordering::Acquire) {
        return;
    }
    let regs = (memory::IO_BASE + MMIO_SLOT as u64 * PAGE) as usize;
    let lapic = (memory::IO_BASE + APIC_SLOT as u64 * PAGE) as usize;
    // Sûreté : les deux fenêtres UC sont permanentes avant publication de IRQ_READY.
    unsafe {
        let control = (regs + 0x38) as *mut u32;
        if ((regs + 0x34) as *const u32).read_volatile() != 0
            && control.read_volatile() & FAULT_MASK == 0
        {
            control.write_volatile(FAULT_MASK);
            let _ = control.read_volatile();
            FAULT_CPU.store(crate::smp::index() as u64, Ordering::Relaxed);
            FAULT_IRQS.fetch_add(1, Ordering::Release);
        }
        ((lapic + 0xb0) as *mut u32).write_volatile(0);
        let _ = ((lapic + 0x20) as *const u32).read_volatile();
    }
}
