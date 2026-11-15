//! Même client statique construit contre le SDK de phase 1 ou de phase 2.
#![no_std]
#![no_main]

use core::sync::atomic::{AtomicBool, Ordering};
use nova_system::{Error, files, ipc::Endpoint, process, raw, thread};

static RELEASE: AtomicBool = AtomicBool::new(false);

struct Child;
impl thread::Task for Child {
    fn run(value: usize) -> usize {
        while !RELEASE.load(Ordering::Acquire) {
            thread::yield_now();
        }
        value
    }
}

fn delegated_file() {
    // Sûreté : au lancement du client, les deux handles uniques sont transférés
    // par nsh. Le second est le premier mot de la page DATA de son ABI.
    let session = unsafe {
        let tx = raw::call(raw::IPC_BOOTSTRAP, 0, 0, 0);
        assert_eq!(tx.status, raw::OK);
        let rx = (0x401000 as *const u64).read_volatile();
        files::Session::delegated(
            Endpoint::from_raw_owned(tx.value),
            Endpoint::from_raw_owned(rx),
        )
    };
    let mut session = session;
    let mut bytes = [0xaa; 8];
    assert_eq!(session.read_view(1, 0, &mut bytes).unwrap(), 7);
    assert_eq!(&bytes, b"NovaOS\n\xaa");
    assert_eq!(session.read_view(1, 7, &mut bytes).unwrap(), 0);
    assert_eq!(
        session.open(0, b"autre", files::READABLE).err(),
        Some(Error(raw::DENIED))
    );
}

fn observe() -> u64 {
    delegated_file();
    let handle = match thread::spawn::<Child>(77, 4) {
        Ok(handle) => handle,
        Err(Error(raw::UNKNOWN_CALL)) => {
            // Sûreté : le sélecteur prend uniquement des scalaires.
            assert_eq!(
                unsafe { raw::call(raw::THREAD_JOIN, 0, 0, 1) }.status,
                raw::UNKNOWN_CALL
            );
            return 19;
        }
        Err(error) => panic!("création : {}", error.0),
    };
    let former = handle.raw();
    // Sûreté : les champs réservés de JOIN doivent toujours être rejetés.
    assert_eq!(
        unsafe { raw::call(raw::THREAD_JOIN, former, 0, 1) }.status,
        raw::BAD_ARGUMENT
    );
    RELEASE.store(true, Ordering::Release);
    assert_eq!(handle.join().unwrap(), 77);
    #[cfg(feature = "phase2")]
    assert_eq!(handle.join(), Err(Error(raw::DENIED)));
    // Sûreté : le numéro périmé sert uniquement au contrôle de sa génération.
    assert_eq!(
        unsafe { raw::call(raw::THREAD_JOIN, former, 0, 0) }.status,
        raw::DENIED
    );
    0
}

#[cfg(target_arch = "x86_64")]
core::arch::global_asm!(".globl _start", "_start:", "call compat_main", "ud2");
#[cfg(target_arch = "aarch64")]
core::arch::global_asm!(".globl _start", "_start:", "bl compat_main", "brk #0");

#[unsafe(no_mangle)]
extern "C" fn compat_main() -> ! {
    process::exit(observe())
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo<'_>) -> ! {
    process::exit(1000 + u64::from(info.location().map_or(0, |here| here.line())))
}
