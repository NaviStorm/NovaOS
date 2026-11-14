//! Mesures activées seulement dans les scénarios de qualification. Les durées
//! sont en unités du compteur architectural. Chaque CPU écrit dans sa ligne.
use crate::{hal, smp};
use core::{
    panic::Location,
    sync::atomic::{AtomicBool, AtomicPtr, AtomicU64, Ordering::*},
};

pub(super) static ACTIVE: AtomicBool = AtomicBool::new(false);
#[repr(align(64))]
struct Cpu {
    acquisitions: AtomicU64,
    conflicts: AtomicU64,
    wait: AtomicU64,
    held: AtomicU64,
    longest: AtomicU64,
    since: AtomicU64,
    site: AtomicPtr<Location<'static>>,
    worst: AtomicPtr<Location<'static>>,
    switches: AtomicU64,
    migrations: AtomicU64,
    kicks: AtomicU64,
    tlb: AtomicU64,
    user_ticks: AtomicU64,
    kernel_ticks: AtomicU64,
    ipc_wait: AtomicU64,
    idle_ticks: AtomicU64,
}
impl Cpu {
    const fn new() -> Self {
        Self {
            acquisitions: zero(),
            conflicts: zero(),
            wait: zero(),
            held: zero(),
            longest: zero(),
            since: zero(),
            site: AtomicPtr::new(core::ptr::null_mut()),
            worst: AtomicPtr::new(core::ptr::null_mut()),
            switches: zero(),
            migrations: zero(),
            kicks: zero(),
            tlb: zero(),
            user_ticks: zero(),
            kernel_ticks: zero(),
            ipc_wait: zero(),
            idle_ticks: zero(),
        }
    }
}
const fn zero() -> AtomicU64 {
    AtomicU64::new(0)
}
static CPU: [Cpu; smp::MAX_CPUS] = [const { Cpu::new() }; smp::MAX_CPUS];
pub(super) fn stamp() -> u64 {
    if ACTIVE.load(Relaxed) {
        hal::timer::counter()
    } else {
        0
    }
}
pub(super) fn acquired(cpu: usize, start: u64, conflict: bool, site: &'static Location<'static>) {
    if start == 0 {
        return;
    }
    let p = &CPU[cpu];
    let end = hal::timer::counter();
    p.acquisitions.fetch_add(1, Relaxed);
    if conflict {
        p.conflicts.fetch_add(1, Relaxed);
        p.wait.fetch_add(end - start, Relaxed);
    }
    p.site.store(core::ptr::from_ref(site).cast_mut(), Relaxed);
    p.since.store(end, Relaxed);
}
pub(super) fn released(cpu: usize) {
    let p = &CPU[cpu];
    let start = p.since.swap(0, Relaxed);
    if start == 0 {
        return;
    }
    let duration = hal::timer::counter() - start;
    p.held.fetch_add(duration, Relaxed);
    if duration > p.longest.load(Relaxed) {
        p.longest.store(duration, Relaxed);
        p.worst.store(p.site.load(Relaxed), Relaxed);
    }
}
pub(super) fn switched(cpu: usize, migrated: bool) {
    if ACTIVE.load(Relaxed) {
        CPU[cpu].switches.fetch_add(1, Relaxed);
        CPU[cpu].migrations.fetch_add(u64::from(migrated), Relaxed);
    }
}
pub(super) fn tick(cpu: usize, user: bool, idle: bool) {
    if ACTIVE.load(Relaxed) {
        let p = &CPU[cpu];
        (if user {
            &p.user_ticks
        } else if idle {
            &p.idle_ticks
        } else {
            &p.kernel_ticks
        })
        .fetch_add(1, Relaxed);
    }
}
pub(super) fn kick(cpu: usize) {
    if ACTIVE.load(Relaxed) {
        CPU[cpu].kicks.fetch_add(1, Relaxed);
    }
}
pub(crate) fn tlb() {
    if ACTIVE.load(Relaxed) {
        CPU[smp::index()].tlb.fetch_add(1, Relaxed);
    }
}
pub(super) fn ipc_woke(cpu: usize, since: u64) {
    if since != 0 {
        CPU[cpu]
            .ipc_wait
            .fetch_add(hal::timer::counter().saturating_sub(since), Relaxed);
    }
}
pub(super) fn report() {
    ACTIVE.store(false, Release);
    for (cpu, p) in CPU[..smp::count()].iter().enumerate() {
        // Les pointeurs publiés désignent exclusivement des Location statiques.
        let site = unsafe { p.worst.load(Acquire).as_ref() };
        hal::log(format_args!(
            "SMP profil cpu={cpu} acquisitions={} conflits={} attente={} detention={} max={} site={} switch={} migrations={} ipi={} tlb={} ticks_usr={} ticks_sys={} ticks_idle={} attente_ipc={}",
            p.acquisitions.load(Relaxed),
            p.conflicts.load(Relaxed),
            p.wait.load(Relaxed),
            p.held.load(Relaxed),
            p.longest.load(Relaxed),
            site.map_or("", |l| l.file()),
            p.switches.load(Relaxed),
            p.migrations.load(Relaxed),
            p.kicks.load(Relaxed),
            p.tlb.load(Relaxed),
            p.user_ticks.load(Relaxed),
            p.kernel_ticks.load(Relaxed),
            p.idle_ticks.load(Relaxed),
            p.ipc_wait.load(Relaxed)
        ));
        if let Some(site) = site {
            hal::log(format_args!("SMP section cpu={cpu} site={site}"));
        }
    }
}
