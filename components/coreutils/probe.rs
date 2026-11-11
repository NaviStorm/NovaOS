#![no_std]
#![no_main]
//! Image accessible uniquement dans le profil de qualification.
use nova_cli::{fileapi::Client, process, protocol as w, runtime as rt, tty};
fn verify() -> rt::Result<()> {
    let args = process::arguments()?;
    match args.as_slice() {
        ["faute"] => {
            // Sûreté : exception synchrone volontaire dans l'image de qualification isolée.
            #[cfg(target_arch = "aarch64")]
            unsafe {
                core::arch::asm!("brk #0", options(noreturn, nostack))
            }
            #[cfg(target_arch = "x86_64")]
            unsafe {
                core::arch::asm!("ud2", options(noreturn, nostack))
            }
        }
        ["stat"] => {
            let mut c = Client::scoped()?;
            let mark = |text: &[u8]| {
                rt::value(rt::call(
                    rt::DEBUG_WRITE,
                    text.as_ptr() as u64,
                    text.len() as u64,
                    0,
                ))
            };
            mark(b"ech debut")?;
            for _ in 0..16 {
                let frame = c.simple(w::STAT, 1)?;
                if w::length(&frame) != 73 || frame[w::PAYLOAD + 24] != 2 {
                    return Err(rt::REFUSED);
                }
            }
            mark(b"ech stat16")?;
            if c.directory(1, b".", w::FILE_WRITE).err() != Some(rt::DENIED) {
                return Err(rt::REFUSED);
            }
            Ok(())
        }
        ["droits"] => {
            if rt::call(tty::SPAWN, 0, 0, 0).status != rt::DENIED
                || rt::call(tty::CONSOLE, tty::INPUT.into(), 0, 0).status != rt::DENIED
            {
                return Err(rt::REFUSED);
            }
            let mut c = Client::scoped()?;
            if c.open(b"/documents/exemple.txt", w::READABLE).err() != Some(rt::DENIED)
                || c.simple(w::UNLINK_AT, 0).err() != Some(rt::DENIED)
            {
                return Err(rt::REFUSED);
            }
            c.cwd_fragment(0, 0)?;
            Ok(())
        }
        ["revocation"] => {
            let mut c = match Client::scoped() {
                Ok(c) => c,
                Err(rt::DENIED | rt::REVOKED) => return Ok(()),
                Err(e) => return Err(e),
            };
            for _ in 0..64 {
                match c.cwd_fragment(0, 0) {
                    Err(rt::DENIED | rt::REVOKED | rt::CLOSED) => return Ok(()),
                    Ok(_) => {}
                    Err(e) => return Err(e),
                }
                rt::call(rt::THREAD_YIELD, 0, 0, 0);
            }
            Err(rt::REFUSED)
        }
        _ => Err(rt::BAD_ARGUMENT),
    }
}
// Sûreté : entrée Rust de l'ELF de qualification, appelée par _start.
#[unsafe(no_mangle)]
extern "C" fn run() -> ! {
    rt::exit(verify().err().unwrap_or(0))
}
