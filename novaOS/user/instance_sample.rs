//! Application de qualification admise explicitement ; ses travailleurs démarrent sans canal.
#![no_std]
#![no_main]
#[allow(dead_code)]
#[path = "../abi/native.rs"]
mod abi;
mod gate;
#[allow(dead_code)]
#[path = "../abi/terminal.rs"]
mod tty;
#[allow(dead_code)]
#[path = "../abi/instances.rs"]
mod wire;
use core::sync::atomic::{AtomicU64, Ordering};

#[cfg(target_arch = "x86_64")]
core::arch::global_asm!(
    ".section .text.demo,\"ax\"",
    ".global _start",
    "_start:",
    "call run",
    "ud2"
);
#[cfg(target_arch = "aarch64")]
core::arch::global_asm!(
    ".section .text.demo,\"ax\"",
    ".global _start",
    "_start:",
    "bl run",
    "brk #0"
);

fn word(index: usize) -> &'static AtomicU64 {
    assert!(index < 8);
    unsafe { &*((0x401000 as *const AtomicU64).add(index)) }
}
fn info(n: u64) -> u64 {
    let r = gate::call(wire::INFO, n, 0, 0);
    assert_eq!(r.status, abi::OK);
    r.value
}
fn exit(code: u64) -> ! {
    gate::call(abi::PROCESS_EXIT, code, 0, 0);
    loop {
        core::hint::spin_loop()
    }
}
extern "C" fn compute(key: u64) -> ! {
    assert_eq!(info(0), key);
    word(4).store(key, Ordering::Release);
    loop {
        word(5).fetch_add(1, Ordering::Relaxed);
    }
}
fn collect(key: u64) -> u64 {
    let mut result = [0u64; 4];
    assert_eq!(
        gate::call(abi::PROCESS_WAIT, key, result.as_mut_ptr() as u64, 32).status,
        abi::OK
    );
    result[3]
}
fn launch_reference() -> abi::Reply {
    let args = [0u64; 5];
    gate::call(
        tty::SPAWN,
        wire::SAMPLE_INDEX as u64,
        args.as_ptr() as u64,
        40,
    )
}
extern "C" fn secondary_launch(_: u64) -> ! {
    let result = launch_reference();
    gate::call(abi::THREAD_EXIT, result.status, 0, 0);
    unreachable!()
}

#[unsafe(no_mangle)]
extern "C" fn run() -> ! {
    let mode = word(0).load(Ordering::Acquire);
    let key = info(0);
    if mode == 8 {
        let thread = gate::call(
            abi::THREAD_CREATE,
            secondary_launch as *const () as u64,
            0,
            2,
        );
        assert_eq!(thread.status, abi::OK);
        let launch = launch_reference();
        assert_eq!(launch.status, abi::OK);
        let joined = gate::call(abi::THREAD_JOIN, thread.value, 0, 0);
        assert_eq!(joined.status, abi::OK);
        assert_eq!(joined.value, abi::DENIED);
        assert_eq!(launch_reference().status, abi::LIMIT);
        assert_eq!(collect(launch.value), 0);
        exit(8);
    }
    if mode == 7 {
        let right = word(1).load(Ordering::Acquire);
        let offer = gate::call(wire::OFFERED, right, 0, 0);
        assert_eq!(offer.status, abi::OK);
        let target = offer.value;
        let from_worker = word(3).load(Ordering::Acquire);
        let from_initial_grant = word(4).load(Ordering::Acquire);
        let independent_root = word(5).load(Ordering::Acquire);
        for cap in [from_worker, from_initial_grant] {
            assert_eq!(gate::call(abi::IPC_NOTIFY, cap, 1, 0).status, abi::OK);
        }
        assert_eq!(
            gate::call(wire::STOP, right, target + 1, 0).status,
            abi::DENIED
        );
        assert_eq!(gate::call(wire::STOP, right, target, 0).status, abi::OK);
        for cap in [from_worker, from_initial_grant] {
            assert_eq!(gate::call(abi::IPC_NOTIFY, cap, 1, 0).status, abi::DENIED);
        }
        assert_eq!(
            gate::call(abi::IPC_NOTIFY, independent_root, 1, 0).status,
            abi::OK
        );
        assert_eq!(collect(right), 125);
        assert_eq!(gate::call(wire::STOP, right, target, 0).status, abi::DENIED);
        exit(7);
    }
    assert_eq!(info(1), wire::SAMPLE);
    assert_eq!(gate::call(abi::IPC_BOOTSTRAP, 0, 0, 0).status, abi::DENIED);
    assert_eq!(gate::call(wire::WORKER, key + 1, 0, 0).status, abi::DENIED);
    let endpoint = gate::call(abi::IPC_CREATE, 0, 0, 0);
    assert_eq!(endpoint.status, abi::OK);
    word(6).store(endpoint.value, Ordering::Release);
    assert_eq!(gate::call(wire::WATCH, 0, 0, 0).status, abi::DENIED);
    if mode == 3 || mode == 4 {
        assert_eq!(gate::call(wire::WORKER, key, 3, 0).status, abi::DENIED);
        if mode == 3 {
            exit(41)
        }
        let created = gate::call(abi::THREAD_CREATE, compute as *const () as u64, key, 2);
        assert_eq!(created.status, abi::OK);
        loop {
            core::hint::spin_loop()
        }
    }
    // Un premier enfant est collecté pendant que le principal reste actif.
    let first = gate::call(wire::WORKER, key, 3, 0);
    assert_eq!(first.status, abi::OK);
    assert_eq!(collect(first.value), 41);
    if mode == 0 {
        exit(0)
    }
    let child = gate::call(wire::WORKER, key, 4, 0);
    assert_eq!(child.status, abi::OK);
    assert_eq!(gate::call(wire::OFFER_STOP, key, 0, 0).status, abi::OK);
    word(2).store(key, Ordering::Release);
    while word(1).load(Ordering::Acquire) == 0 {
        gate::call(abi::THREAD_YIELD, 0, 0, 0);
    }
    if mode == 2 {
        // Instruction illégale au site de qualification, après l’admission du travailleur.
        #[cfg(target_arch = "x86_64")]
        unsafe {
            core::arch::asm!("ud2", options(noreturn));
        }
        #[cfg(target_arch = "aarch64")]
        unsafe {
            core::arch::asm!("brk #0", options(noreturn));
        }
    }
    exit(0)
}
#[panic_handler]
fn panic(_: &core::panic::PanicInfo<'_>) -> ! {
    exit(255)
}
