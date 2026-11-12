//! Transport du disque de données dédié. Toutes les adresses matérielles sont privées.
mod io;
mod lease;
pub(crate) mod metrics;
mod pci;
#[cfg(all(target_arch = "aarch64", feature = "dma-sequentiel"))]
pub(crate) mod qualification;
mod queue;
pub(crate) use lease::publication_unique;
pub(crate) use lease::{
    PORTAL, admit, attach, begin_session, command, died, inflight, private_frames, retire,
};
#[cfg(target_arch = "x86_64")]
#[path = "vtd.rs"]
mod domain;
#[cfg(target_arch = "aarch64")]
#[path = "viommu.rs"]
mod domain;

use crate::hal;
use core::sync::atomic::{AtomicU64, Ordering};
use io::{Frame, PAGE, Pages, barrier, wait};
use pci::Device;
use queue::{Queue, Segment};

const IOVA: u64 = 0x0800_0000;
const SID: u16 = 6 << 3;
const VIRTIO: u64 = (1 << 32) | (1 << 33) | (1 << 6) | (1 << 9);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Error {
    Absent,
    Memory,
    Geometry,
    Feature,
    Timeout,
    Queue,
    Route,
    Fault,
    Device,
    State,
    Quarantine,
}
pub(crate) type Result<T> = core::result::Result<T, Error>;
static USED_ADDR: AtomicU64 = AtomicU64::new(0);
static EXPECT: AtomicU64 = AtomicU64::new(0);
static STRAYS: AtomicU64 = AtomicU64::new(0);
static IRQS: AtomicU64 = AtomicU64::new(0);
static KICKS: AtomicU64 = AtomicU64::new(0);
static ISR: AtomicU64 = AtomicU64::new(0);
/// Exécuté sur le BSP. L'IRQ ne prend jamais le verrou du composant DMA.
pub(crate) fn interrupt() -> bool {
    let addr = ISR.load(Ordering::Acquire);
    if addr != 0 {
        // Sûreté : la porte est installée sous masque. La page ISR reste
        // mappée superviseur jusqu'après la purge, y compris pendant un reset.
        let flags = unsafe { (addr as *const u8).read_volatile() };
        if cfg!(target_arch = "x86_64") || flags & 1 != 0 {
            let expected = EXPECT.load(Ordering::Acquire);
            let used = USED_ADDR.load(Ordering::Acquire);
            // Sûreté : indice aligné de la file privée encore épinglée. L'ancien
            // propriétaire et l'adresse sont conservés jusqu'à la purge matérielle.
            if expected != 0
                && used != 0
                && unsafe { (used as *const u16).read_volatile() } == expected as u16
            {
                EXPECT.store(0, Ordering::Release);
                IRQS.fetch_add(1, Ordering::Release);
            } else {
                STRAYS.fetch_add(1, Ordering::Relaxed);
            }
            return true;
        }
    }
    false
}
fn frame(p: &Pages, i: usize) -> Frame {
    p.0[i]
}
fn addr(n: usize) -> u64 {
    IOVA + (n * PAGE) as u64
}

#[cfg(target_arch = "x86_64")]
static LAPIC: AtomicU64 = AtomicU64::new(0);
#[cfg(target_arch = "x86_64")]
pub(crate) fn eoi() {
    let addr = LAPIC.load(Ordering::Acquire);
    if addr != 0 {
        // Sûreté : fenêtre UC du LAPIC, une écriture alignée du BSP après une IRQ 0x51.
        unsafe { ((addr + 0xb0) as *mut u32).write_volatile(0) }
    }
}

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
enum State {
    Initial,
    Granted,
    Domain,
    Route,
    Queues,
    Active,
    Running,
    Closed,
    Quiet,
    IrqRevoked,
    Unmapped,
    Released,
}
struct Lifecycle(State);
impl Lifecycle {
    fn next(&mut self, from: State, to: State) -> Result<()> {
        use State::*;
        if self.0 != from
            || !matches!(
                (from, to),
                (Initial, Granted)
                    | (Granted, Domain)
                    | (Domain, Route)
                    | (Route, Queues)
                    | (Queues, Active)
                    | (Active, Running)
                    | (Running, Closed)
                    | (Closed, Quiet)
                    | (Quiet, IrqRevoked)
                    | (IrqRevoked, Unmapped)
                    | (Unmapped, Released)
            )
        {
            return Err(Error::State);
        }
        self.0 = to;
        Ok(())
    }
}
struct Transport {
    p: Pages,
    d: Device,
    unit: domain::Domain,
    q: Queue,
    state: Lifecycle,
    sectors: u64,
    size: usize,
    pending: Option<u64>,
    ro: bool,
    // La page de données OUT n'est traduite qu'après l'admission en écriture.
    output_mapped: bool,
    started: u64,
    kind: usize,
}
impl Transport {
    fn open() -> Result<Self> {
        Self::open_disk(SID as u8, true)
    }
    fn open_disk(sid: u8, output_mapped: bool) -> Result<Self> {
        let mut state = Lifecycle(State::Initial);
        let d = Device::discover(sid, 0x1042, 288)?;
        let p = Pages::new()?;
        state.next(State::Initial, State::Granted)?;
        let mut unit = domain::Domain::start(&p, sid).map_err(|e| {
            if e == Error::Absent {
                Error::Feature
            } else {
                e
            }
        })?;
        state.next(State::Granted, State::Domain)?;
        // QEMU lit used.idx à l’activation. La seule page mixte est celle de
        // l’anneau utilisé ; aucune donnée client n’y réside.
        let rights = [1, 1, 3, 1, 2, 2, 1];
        let mapped = if output_mapped { 7 } else { 6 };
        for (n, &r) in rights[..mapped].iter().enumerate() {
            unit.map(&p, n, frame(&p, 16 + n).pa, r)?
        }
        let ro = d.read_only();
        d.start(VIRTIO | if ro { 1 << 5 } else { 0 })?;
        let (sectors, size) = d.geometry()?;
        ISR.store(d.isr.base() as u64, Ordering::Release);
        unit.route(&p, d, true)?;
        state.next(State::Domain, State::Route)?;
        let q = Queue::new(
            d,
            0,
            [p.0[16], p.0[17], p.0[18]],
            [addr(0), addr(1), addr(2)],
        )?;
        state.next(State::Route, State::Queues)?;
        USED_ADDR.store((p.0[18].mem.base() + 2) as u64, Ordering::Release);
        d.ready()?;
        state.next(State::Queues, State::Active)?;
        unit.faults(&p)?;
        state.next(State::Active, State::Running)?;
        Ok(Self {
            p,
            d,
            unit,
            q,
            state,
            sectors,
            size,
            pending: None,
            ro,
            output_mapped,
            started: 0,
            kind: 0,
        })
    }
    fn begin_read(&mut self, lba: u64) -> Result<()> {
        self.begin_input(lba, 0, self.size)
    }
    // Seules les opérations IN (0) et GET_ID (8) ont une chaîne de réception.
    fn begin_input(&mut self, lba: u64, opcode: u32, bytes: usize) -> Result<()> {
        if self.state.0 != State::Running {
            return Err(Error::State);
        }
        if self.pending.is_some() {
            return Err(Error::State);
        }
        if bytes == 0
            || bytes > PAGE
            || lba >= self.sectors
            || (opcode == 0
                && (bytes % self.size != 0 || (bytes / self.size) as u64 > self.sectors - lba))
            || (opcode == 8 && (lba != 0 || bytes != 20))
            || !matches!(opcode, 0 | 8)
        {
            return Err(Error::Geometry);
        }
        let sector = lba
            .checked_mul((self.size / 512) as u64)
            .ok_or(Error::Geometry)?;
        self.unit.faults(&self.p)?;
        let h = self.p.0[19].mem;
        h.w32(0, opcode);
        h.w32(4, 0);
        h.w64(8, sector);
        self.p.0[21].mem.w8(0, 0xff);
        self.started = metrics::stamp();
        self.kind = 0;
        let n = IRQS.load(Ordering::Acquire);
        EXPECT.store(
            0x10000 | u64::from(self.q.index.wrapping_add(1)),
            Ordering::Release,
        );
        self.q.submit(&[
            Segment::new(addr(3), 16, false),
            Segment::new(addr(4), bytes as u32, true),
            Segment::new(addr(5), 1, true),
        ])?;
        KICKS.fetch_add(1, Ordering::Release);
        self.pending = Some(n);
        Ok(())
    }
    fn finish_read(&mut self) -> Result<()> {
        self.finish(1 + self.size as u32)
    }
    fn finish(&mut self, expected: u32) -> Result<()> {
        let n = self.pending.ok_or(Error::State)?;
        let used = self.q.completed()?;
        wait(|| IRQS.load(Ordering::Acquire) != n)?;
        self.unit.faults(&self.p)?;
        let status = self.p.0[21].mem.r8(0);
        if used != expected || status != 0 {
            hal::log(format_args!(
                "BLOC erreur peripherique op={} statut={status} longueur={used}",
                self.kind
            ));
            return Err(Error::Device);
        }
        metrics::record(self.kind, self.started);
        self.pending = None;
        Ok(())
    }
    fn read(&mut self, lba: u64, out: &mut [u8]) -> Result<()> {
        if out.len() != self.size {
            return Err(Error::Geometry);
        }
        self.begin_read(lba)?;
        self.finish_read()?;
        self.p.0[20].mem.bytes(0, out);
        Ok(())
    }
    fn output(&mut self, lba: u64, flush: bool) -> Result<()> {
        if !self.output_mapped {
            return Err(Error::State);
        }
        if self.ro && !flush {
            return Err(Error::Device);
        }
        if self.state.0 != State::Running || self.pending.is_some() {
            return Err(Error::State);
        }
        if lba >= self.sectors {
            return Err(Error::Geometry);
        }
        let sector = lba
            .checked_mul((self.size / 512) as u64)
            .ok_or(Error::Geometry)?;
        self.unit.faults(&self.p)?;
        let h = self.p.0[19].mem;
        h.w32(0, if flush { 4 } else { 1 });
        h.w32(4, 0);
        h.w64(8, sector);
        self.p.0[21].mem.w8(0, 0xff);
        let head = Segment::new(addr(3), 16, false);
        let status = Segment::new(addr(5), 1, true);
        self.started = metrics::stamp();
        self.kind = if flush { 2 } else { 1 };
        let seen = IRQS.load(Ordering::Acquire);
        EXPECT.store(
            0x10000 | u64::from(self.q.index.wrapping_add(1)),
            Ordering::Release,
        );
        if flush {
            self.q.submit(&[head, status])?
        } else {
            self.q
                .submit(&[head, Segment::new(addr(6), self.size as u32, false), status])?
        }
        KICKS.fetch_add(1, Ordering::Release);
        self.pending = Some(seen);
        self.finish(1)
    }
    fn close(mut self) -> Result<()> {
        self.state.next(State::Running, State::Closed)?;
        self.d.reset()?;
        self.d.cfg.master(false);
        if self.d.cfg.get(4, 2) & 4 != 0 {
            return Err(Error::Quarantine);
        }
        self.state.next(State::Closed, State::Quiet)?;
        self.unit.route(&self.p, self.d, false)?;
        self.unit.clear_pending(self.d)?;
        ISR.store(0, Ordering::Release);
        USED_ADDR.store(0, Ordering::Release);
        EXPECT.store(0, Ordering::Release);
        self.state.next(State::Quiet, State::IrqRevoked)?;
        let mapped = if self.output_mapped { 7 } else { 6 };
        for n in 0..mapped {
            self.unit.map(&self.p, n, 0, 0)?
        }
        self.unit.stop(&self.p)?;
        self.state.next(State::IrqRevoked, State::Unmapped)?;
        barrier();
        // Sûreté : queue réinitialisée, interruptions purgées, fonction arrêtée,
        // tables retirées et confirmation matérielle reçue avant restitution.
        unsafe { self.p.release() };
        self.state.next(State::Unmapped, State::Released)
    }
}
pub(crate) fn probe() {
    hal::log(format_args!("DMA qualification native debut"));
    let res = (|| {
        let mut t = Transport::open()?;
        hal::log(format_args!(
            "DMA domaine actif taille={} blocs={}",
            t.size, t.sectors
        ));
        let mut data = [0u8; PAGE];
        let lba = 8192 / t.size;
        t.read(lba as u64, &mut data[..t.size])?;
        if !data[..t.size]
            .iter()
            .enumerate()
            .all(|(i, &v)| v == 0x71 + (i % 83) as u8)
        {
            return Err(Error::Device);
        }
        hal::log(format_args!(
            "DMA motif correct IRQ={} octets={}",
            IRQS.load(Ordering::Acquire),
            t.size
        ));
        t.close()?;
        hal::log(format_args!("DMA retrait acquitte"));
        Ok::<_, Error>(())
    })();
    hal::log(format_args!("DMA bilan {res:?}"));
    hal::cpu::halt()
}

/// Sonde défensive sur l'unique tampon privé du banc. Aucun accès en dehors
/// de l'image jetable, aucune commande d'écriture disque ni émission MSI forgée.
pub(crate) fn probe_protection() -> Result<()> {
    let mut illegal = Lifecycle(State::Released);
    if illegal.next(State::Released, State::Running) != Err(Error::State) {
        return Err(Error::State);
    }
    let before = KICKS.load(Ordering::Acquire);
    for rights in [1, 0] {
        let mut t = Transport::open()?;
        let count = t.size;
        let bounce = t.p.0[20];
        let before_bytes = [0x5au8; PAGE];
        bounce.mem.put(0, &before_bytes);
        t.unit.map(&t.p, 4, 0, 0)?;
        if rights != 0 {
            t.unit.map(&t.p, 4, bounce.pa, rights)?
        }
        t.begin_read((8192 / count) as u64)?;
        let result = t.finish_read();
        let status = t.p.0[21].mem.r8(0);
        let retained = (0..PAGE).all(|i| bounce.mem.r8(i) == 0x5a);
        let fault = t.unit.faults(&t.p);
        hal::log(format_args!(
            "DMA protection droits={rights} IRQ={} statut={status} erreur={result:?} faute={fault:?} temoin={retained}",
            IRQS.load(Ordering::Acquire)
        ));
        if result != Err(Error::Fault) || fault != Err(Error::Fault) || !retained {
            return Err(Error::Fault);
        }
        // Le retrait de la page déjà absente n'est pas une nouvelle opération UNMAP.
        // Restaurer sa traduction privée après reset uniquement pour employer le
        // même chemin de destruction que la production.
        t.d.reset()?;
        t.d.cfg.master(false);
        if rights != 0 {
            t.unit.map(&t.p, 4, 0, 0)?
        }
        t.unit.map(&t.p, 4, bounce.pa, 2)?;
        t.close()?;
    }
    let mut t = Transport::open()?;
    let seen = IRQS.load(Ordering::Acquire);
    t.unit.hold(t.d)?;
    t.begin_read((8192 / t.size) as u64)?;
    let used = t.q.completed()?;
    wait(|| t.unit.pending_on_mask(t.d))?;
    if used != t.size as u32 + 1 || IRQS.load(Ordering::Acquire) != seen {
        return Err(Error::Route);
    }
    t.close()?;
    let mut fresh = Transport::open()?;
    if fresh.unit.pending_on_mask(fresh.d) || IRQS.load(Ordering::Acquire) != seen {
        return Err(Error::Route);
    }
    let mut out = [0; PAGE];
    let n = fresh.size;
    fresh.read((8192 / n) as u64, &mut out[..n])?;
    if IRQS.load(Ordering::Acquire) != seen + 1
        || !out[..n]
            .iter()
            .enumerate()
            .all(|(i, &x)| x == 0x71 + (i % 83) as u8)
    {
        return Err(Error::Route);
    }
    fresh.close()?;
    hal::log(format_args!(
        "DMA interruption masquee en attente retiree et nouveau proprietaire confirme"
    ));
    // Une vraie interruption de l'ancienne demande est déjà pendante. Fermer
    // son destinataire, puis livrer la route initiale : aucun nouveau réveil.
    let mut stale = Transport::open()?;
    let strays = STRAYS.load(Ordering::Acquire);
    let accepted = IRQS.load(Ordering::Acquire);
    stale.unit.hold(stale.d)?;
    stale.begin_read((8192 / stale.size) as u64)?;
    stale.q.completed()?;
    wait(|| stale.unit.pending_on_mask(stale.d))?;
    EXPECT.store(0, Ordering::Release);
    stale.unit.resume_line(stale.d)?;
    wait(|| STRAYS.load(Ordering::Acquire) > strays)?;
    if IRQS.load(Ordering::Acquire) != accepted {
        return Err(Error::Route);
    }
    stale.close()?;
    hal::log(format_args!("DMA IRQ tardive acquittee sans livraison"));
    if KICKS.load(Ordering::Acquire) - before != 5 {
        return Err(Error::Queue);
    }
    Ok(())
}
pub(crate) fn kicks() -> u64 {
    KICKS.load(Ordering::Acquire)
}

pub(crate) fn stray() {
    STRAYS.fetch_add(1, Ordering::Relaxed);
}

// Réattribution réelle d'une ancienne page de données au seul banc de qualification.
pub(crate) struct Witness(Frame);
impl Witness {
    pub(crate) fn claim(pa: u64) -> Result<Self> {
        let mut held = [io::EMPTY; 32];
        let mut found = None;
        for item in &mut held {
            let f = Frame::allocate()?;
            if f.pa == pa {
                found = Some(f);
                break;
            }
            *item = f;
        }
        for f in held.into_iter().filter(|f| f.pa != 0) {
            // Sûreté : cadre temporaire du banc, jamais publié à un périphérique.
            unsafe { crate::memory::release_dma(f.pa) }
        }
        let f = found.ok_or(Error::Memory)?;
        if !(0..PAGE).all(|i| f.mem.r8(i) == 0) {
            return Err(Error::Fault);
        }
        for i in 0..PAGE {
            f.mem.w8(i, 0xbd)
        }
        Ok(Self(f))
    }
    pub(crate) fn verify(self) -> Result<()> {
        if !(0..PAGE).all(|i| self.0.mem.r8(i) == 0xbd) {
            return Err(Error::Fault);
        }
        // Sûreté : le banc possède ce cadre, jamais accordé au nouveau domaine.
        unsafe { crate::memory::release_dma(self.0.pa) };
        Ok(())
    }
}
