//! Programme autonome NovaOS. Le mode 0 se termine sans intervention du banc.
#![no_std]
#![no_main]
use core::sync::atomic::{AtomicU64, Ordering};
use nova_system::{process, raw, sync, thread};

#[cfg(feature = "qualification")]
mod checks;

static TOTAL: sync::Mutex<usize> = sync::Mutex::new(0);
static START: AtomicU64 = AtomicU64::new(0);
static PROCESS: AtomicU64 = AtomicU64::new(0);
const ROUNDS: usize = 24;

struct Worker;
impl thread::Task for Worker {
    fn run(seed: usize) -> usize {
        assert_eq!(process::id().unwrap(), PROCESS.load(Ordering::Acquire));
        assert_eq!(thread::tls_get(0).unwrap(), 0);
        thread::tls_set(0, seed).unwrap();
        while START.load(Ordering::Acquire) == 0 {
            // Une publication entre la lecture et l'attente interdit de dormir.
            assert!(matches!(
                sync::wait(&START, 0, u64::MAX),
                Ok(()) | Err(nova_system::Error(raw::WOULD_BLOCK))
            ));
        }
        for _ in 0..ROUNDS {
            let mut sum = TOTAL.lock().unwrap();
            *sum += seed;
        }
        thread::tls_get(0).unwrap()
    }
}

fn example() {
    PROCESS.store(process::id().unwrap(), Ordering::Release);
    let first = thread::spawn::<Worker>(7, 4).unwrap();
    let second = thread::spawn::<Worker>(11, 4).unwrap();
    assert_ne!(first.raw(), second.raw());
    START.store(1, Ordering::Release);
    sync::notify(&START, 2).unwrap();
    #[cfg(feature = "qualification")]
    let former = first.raw();
    assert_eq!(first.join().unwrap(), 7);
    assert_eq!(second.join().unwrap(), 11);
    assert_eq!(*TOTAL.lock().unwrap(), (7 + 11) * ROUNDS);
    #[cfg(feature = "qualification")]
    {
        assert_eq!(
            unsafe { raw::call(raw::THREAD_JOIN, former, 0, 0) }.status,
            raw::DENIED
        );
        checks::mark(b"externe somme=432 threads=2 tls=7,11 double-join=refuse");
        checks::lifecycle();
    }
}

// L'entree ELF recoit un sommet de pile, avant tout appel C ou Rust.
#[cfg(target_arch = "x86_64")]
core::arch::global_asm!(".globl _start", "_start:", "call witness_main", "ud2");
#[cfg(target_arch = "aarch64")]
core::arch::global_asm!(".globl _start", "_start:", "bl witness_main", "brk #0");

#[unsafe(no_mangle)]
extern "C" fn witness_main() -> ! {
    #[cfg(not(feature = "qualification"))]
    example();
    #[cfg(feature = "qualification")]
    match checks::word(0).load(Ordering::Acquire) {
        0 => example(),
        mode => checks::scenario(mode),
    }
    process::exit(0)
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo<'_>) -> ! {
    let line = info.location().map_or(0, |point| point.line());
    process::exit(20000 + u64::from(line))
}
