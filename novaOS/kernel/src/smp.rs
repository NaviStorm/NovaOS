//! Topologie logique dense. L'identité matérielle peut être discontinue.
use crate::{arch, boot, hal};
use core::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

pub const MAX_CPUS: usize = 16;
static HARDWARE: [AtomicUsize; MAX_CPUS] = [const { AtomicUsize::new(usize::MAX) }; MAX_CPUS];
static COUNT: AtomicUsize = AtomicUsize::new(1);
pub static ONLINE: AtomicU64 = AtomicU64::new(1);

pub fn count() -> usize {
    COUNT.load(Ordering::Acquire)
}
pub fn index() -> usize {
    if count() == 1 {
        return 0;
    }
    let hardware = arch::cpu_id();
    HARDWARE[..count()]
        .iter()
        .position(|id| id.load(Ordering::Relaxed) == hardware)
        .expect("CPU étranger à la topologie")
}
pub fn hardware(cpu: usize) -> usize {
    HARDWARE[cpu].load(Ordering::Acquire)
}

fn identity(cpu: &limine::mp::Cpu) -> usize {
    #[cfg(target_arch = "x86_64")]
    {
        cpu.lapic_id as usize
    }
    #[cfg(target_arch = "aarch64")]
    {
        cpu.mpidr as usize & 0xff_00ff_ffff
    }
}

// Tous les AP restent dans Limine jusqu'à la publication de leurs structures.
pub fn discover() {
    let bsp = arch::cpu_id();
    HARDWARE[0].store(bsp, Ordering::Relaxed);
    let Some(info) = boot::processors() else {
        return;
    };
    let cpus = info.cpus();
    assert!(
        !cpus.is_empty() && cpus.len() <= MAX_CPUS,
        "topologie hors capacité"
    );
    assert_eq!(
        cpus.iter().filter(|cpu| identity(cpu) == bsp).count(),
        1,
        "BSP absent ou dupliqué"
    );
    let mut next = 1;
    for cpu in cpus {
        let id = identity(cpu);
        if id == bsp {
            continue;
        }
        assert!(
            !HARDWARE[..next]
                .iter()
                .any(|v| v.load(Ordering::Relaxed) == id)
        );
        HARDWARE[next].store(id, Ordering::Relaxed);
        cpu.extra.store(next as u64, Ordering::Relaxed);
        next += 1;
    }
    assert_eq!(next, cpus.len(), "BSP absent ou dupliqué");
    COUNT.store(next, Ordering::Release);
}

pub fn launch() {
    if count() == 1 {
        return;
    }
    arch::prepare_secondaries();
    IPI_READY.store(true, Ordering::Release);
    for cpu in boot::processors().unwrap().cpus() {
        if boot::scenario() == b"smp-ap-absent"
            && cpu.extra.load(Ordering::Relaxed) as usize == count() - 1
        {
            continue;
        }
        if identity(cpu) != hardware(0) {
            cpu.goto_address.write(secondary);
        }
    }
}

unsafe extern "C" fn secondary(info: &limine::mp::Cpu) -> ! {
    hal::interrupts::mask();
    let cpu = info.extra.load(Ordering::Acquire) as usize;
    assert!(cpu > 0 && cpu < count() && hardware(cpu) == arch::cpu_id());
    check_stop();
    arch::init_secondary(cpu);
    crate::sched::secondary(cpu)
}

static PARK: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);
pub fn stop_requested() -> bool {
    PARK.load(Ordering::Acquire)
}
pub fn request_park() {
    assert_eq!(index(), 0);
    PARK.store(true, Ordering::Release);
    if count() > 1 {
        arch::kick();
    }
}

// Arrêt terminal de la machine. Les piles, tables, baux et TCB restent possédés.
// Le coordinateur n'emprunte aucun verrou qui pourrait appartenir à un CPU arrêté.
static COORDINATOR: AtomicUsize = AtomicUsize::new(usize::MAX);
static ACK: AtomicU64 = AtomicU64::new(0);
static STOP_SP: [AtomicUsize; MAX_CPUS] = [const { AtomicUsize::new(0) }; MAX_CPUS];
static IPI_READY: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);

pub(crate) fn record_park(cpu: usize) {
    STOP_SP[cpu].store(hal::context::sp(), Ordering::Relaxed);
    ACK.fetch_or(1 << cpu, Ordering::Release);
}

pub(crate) fn check_stop() {
    let owner = COORDINATOR.load(Ordering::Acquire);
    if owner != usize::MAX && owner != index() {
        hal::interrupts::mask();
        let cpu = index();
        record_park(cpu);
        hal::cpu::park_local();
    }
}

pub(crate) fn shutdown(reason: &str, details: core::fmt::Arguments<'_>) -> ! {
    hal::interrupts::mask();
    let cpu = index();
    if COORDINATOR
        .compare_exchange(usize::MAX, cpu, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        check_stop();
        // Double faute du coordinateur lui-même.
        hal::cpu::park_local();
    }
    let expected = (1u64 << count()) - 1;
    let online = ONLINE.load(Ordering::Acquire);
    STOP_SP[cpu].store(hal::context::sp(), Ordering::Release);
    ACK.fetch_or(1 << cpu, Ordering::Release);
    if online & !(1 << cpu) != 0 && IPI_READY.load(Ordering::Acquire) {
        arch::stop_ipi();
    }
    if let Some(write) = arch::DIAGNOSTIC {
        if details.as_str() != Some("") {
            write(details);
        }
    }
    let start = hal::timer::counter();
    let budget = arch::stop_budget();
    while ACK.load(Ordering::Acquire) & online != online {
        if hal::timer::counter().wrapping_sub(start) >= budget {
            break;
        }
        core::hint::spin_loop();
    }
    let ack = ACK.load(Ordering::Acquire);
    if let Some(write) = arch::DIAGNOSTIC {
        for (id, saved) in STOP_SP[..count()].iter().enumerate() {
            if ack & (1 << id) != 0 {
                write(format_args!(
                    "SMP diagnostic cpu={id} sp={:#x}",
                    saved.load(Ordering::Acquire)
                ));
            }
        }
        write(format_args!(
            "SMP arret motif={reason} coordinateur={cpu} attendus={expected:#x} en_ligne={online:#x} ack={ack:#x} absents={:#x} muets={:#x}",
            expected & !(online | ack),
            online & !ack
        ));
    }
    hal::cpu::park_local();
}
