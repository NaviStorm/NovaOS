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

// Ces assertions sont exécutées dans l’image témoin d’une page. Un refus
// inattendu est transmis par le statut de sortie du processus de qualification.
#[inline(never)]
fn call(op: u64, a: u64, b: u64, c: u64) -> abi::Reply {
    gate::call(op, a, b, c)
}

macro_rules! check_eq {
    ($actual:expr, $expected:expr $(,)?) => {
        if $actual != $expected {
            exit(255)
        }
    };
}

fn word(index: usize) -> &'static AtomicU64 {
    assert!(index < 8);
    unsafe { &*((0x401000 as *const AtomicU64).add(index)) }
}
fn info(n: u64) -> u64 {
    let r = call(wire::INFO, n, 0, 0);
    check_eq!(r.status, abi::OK);
    r.value
}
type Frame = [u8; wire::Identity::SIZE as usize];

fn identity(op: u64, subject: u64) -> Frame {
    let mut bytes = [0; wire::Identity::SIZE as usize];
    let reply = call(op, subject, bytes.as_mut_ptr() as u64, wire::Identity::SIZE);
    check_eq!(reply.status, abi::OK);
    check_eq!(reply.value, wire::Identity::SIZE);
    bytes
}

fn worker(bytes: &Frame, arg: u64) -> abi::Reply {
    call(
        wire::WORKER,
        bytes.as_ptr() as u64,
        arg,
        wire::Identity::SIZE,
    )
}

fn stop(right: u64, bytes: &Frame) -> abi::Reply {
    call(
        wire::STOP,
        right,
        bytes.as_ptr() as u64,
        wire::Identity::SIZE,
    )
}

fn exit(code: u64) -> ! {
    call(abi::PROCESS_EXIT, code, 0, 0);
    loop {
        core::hint::spin_loop()
    }
}
extern "C" fn compute(expected: u64) -> ! {
    // Le thread principal conserve ce tableau immuable sur sa pile jusqu’à l’arrêt collectif.
    let frame = unsafe { &*(expected as *const Frame) };
    check_eq!(identity(wire::INFO, 2), *frame);
    let key = wire::Identity::decode(*frame).instance;
    word(4).store(key, Ordering::Release);
    loop {
        word(5).fetch_add(1, Ordering::Relaxed);
    }
}
fn collect(key: u64) -> u64 {
    let mut result = [0u64; 4];
    check_eq!(
        call(abi::PROCESS_WAIT, key, result.as_mut_ptr() as u64, 32).status,
        abi::OK
    );
    result[3]
}
fn launch_reference() -> abi::Reply {
    let args = [0u64; 5];
    call(
        tty::SPAWN,
        wire::SAMPLE_INDEX as u64,
        args.as_ptr() as u64,
        40,
    )
}
extern "C" fn secondary_launch(_: u64) -> ! {
    let result = launch_reference();
    call(abi::THREAD_EXIT, result.status, 0, 0);
    unreachable!()
}

#[unsafe(no_mangle)]
extern "C" fn run() -> ! {
    let mode = word(0).load(Ordering::Acquire);
    let key = info(0);
    if mode == 14 {
        let pid = call(abi::PROCESS_ID, 0, 0, 0);
        check_eq!(pid.status, abi::OK);
        assert!(pid.value < key);
        let endpoint = call(abi::IPC_CREATE, 0, 0, 0);
        check_eq!(endpoint.status, abi::OK);
        check_eq!(call(abi::IPC_NOTIFY, endpoint.value, 1, 0).status, abi::OK);
        let mut buffer = [0u8; wire::Identity::SIZE as usize];
        check_eq!(
            call(
                wire::INFO,
                2,
                buffer.as_mut_ptr() as u64,
                wire::Identity::SIZE
            )
            .status,
            abi::REFUSED
        );
        check_eq!(worker(&buffer, 3).status, abi::REFUSED);
        exit(14);
    }
    let own = identity(wire::INFO, 2);
    check_eq!(key, wire::Identity::decode(own).instance);
    if mode == 12 || mode == 13 {
        if mode == 13 {
            let mut old = [0; wire::Identity::SIZE as usize];
            for (i, chunk) in old.chunks_exact_mut(8).enumerate() {
                chunk.copy_from_slice(&word(i + 1).load(Ordering::Acquire).to_le_bytes());
            }
            check_eq!(old[16..], own[16..]);
            assert_ne!(old[..16], own[..16]);
            check_eq!(worker(&old, 3).status, abi::DENIED);
        }
        let child = worker(&own, 3);
        check_eq!(child.status, abi::OK);
        check_eq!(collect(child.value), 41);
        check_eq!(identity(wire::INFO, 2), own);
        for (i, chunk) in own.chunks_exact(8).enumerate() {
            word(i + 1).store(
                u64::from_le_bytes(chunk.try_into().unwrap()),
                Ordering::Release,
            );
        }
        word(7).store(1, Ordering::Release);
        while word(6).load(Ordering::Acquire) == 0 {
            call(abi::THREAD_YIELD, 0, 0, 0);
        }
        exit(0);
    }
    if mode == 8 {
        let thread = call(
            abi::THREAD_CREATE,
            secondary_launch as *const () as u64,
            0,
            2,
        );
        check_eq!(thread.status, abi::OK);
        let launch = launch_reference();
        check_eq!(launch.status, abi::OK);
        let joined = call(abi::THREAD_JOIN, thread.value, 0, 0);
        check_eq!(joined.status, abi::OK);
        check_eq!(joined.value, abi::DENIED);
        check_eq!(launch_reference().status, abi::LIMIT);
        check_eq!(collect(launch.value), 0);
        exit(8);
    }
    if mode == 7 {
        let right = word(1).load(Ordering::Acquire);
        let target = identity(wire::OFFERED, right);
        check_eq!(stop(right.wrapping_add(1), &target).status, abi::DENIED);
        let from_worker = word(3).load(Ordering::Acquire);
        let from_initial_grant = word(4).load(Ordering::Acquire);
        let independent_root = word(5).load(Ordering::Acquire);
        for cap in [from_worker, from_initial_grant] {
            check_eq!(call(abi::IPC_NOTIFY, cap, 1, 0).status, abi::OK);
        }
        for offset in [0, 8, 16, 24] {
            let mut wrong = target;
            wrong[offset] ^= 1;
            check_eq!(stop(right, &wrong).status, abi::DENIED);
        }
        check_eq!(stop(right, &target).status, abi::OK);
        for cap in [from_worker, from_initial_grant] {
            check_eq!(call(abi::IPC_NOTIFY, cap, 1, 0).status, abi::DENIED);
        }
        check_eq!(
            call(abi::IPC_NOTIFY, independent_root, 1, 0).status,
            abi::OK
        );
        check_eq!(collect(right), 125);
        check_eq!(stop(right, &target).status, abi::DENIED);
        exit(7);
    }
    check_eq!(info(1), wire::SAMPLE);
    check_eq!(call(abi::IPC_BOOTSTRAP, 0, 0, 0).status, abi::DENIED);
    let mut wrong = own;
    wrong[24] ^= 1;
    check_eq!(worker(&wrong, 0).status, abi::DENIED);
    let endpoint = call(abi::IPC_CREATE, 0, 0, 0);
    check_eq!(endpoint.status, abi::OK);
    word(6).store(endpoint.value, Ordering::Release);
    check_eq!(call(wire::WATCH, 0, 0, 0).status, abi::DENIED);
    if mode == 3 || mode == 4 {
        check_eq!(worker(&own, 3).status, abi::DENIED);
        if mode == 3 {
            exit(41)
        }
        let created = call(
            abi::THREAD_CREATE,
            compute as *const () as u64,
            &own as *const Frame as u64,
            2,
        );
        check_eq!(created.status, abi::OK);
        loop {
            core::hint::spin_loop()
        }
    }
    // Un premier enfant est collecté pendant que le principal reste actif.
    let first = worker(&own, 3);
    check_eq!(first.status, abi::OK);
    check_eq!(collect(first.value), 41);
    if mode == 0 {
        exit(0)
    }
    let child = worker(&own, 4);
    check_eq!(child.status, abi::OK);
    check_eq!(
        call(
            wire::OFFER_STOP,
            own.as_ptr() as u64,
            0,
            wire::Identity::SIZE
        )
        .status,
        abi::OK
    );
    word(2).store(key, Ordering::Release);
    while word(1).load(Ordering::Acquire) == 0 {
        call(abi::THREAD_YIELD, 0, 0, 0);
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
