//! Sonde de concurrence, insérée uniquement dans l'export QEMU instrumenté.
//! Le membre secondaire possède les mêmes handles locaux que le thread principal.
use super::*;
use core::sync::atomic::{AtomicU32, Ordering};

static RENDEZ_VOUS: AtomicU32 = AtomicU32::new(0);
const ARGV: &[u8] = b"\x01\x0a\x00lignes.tsv";

fn attendre(etape: u32) -> bool {
    for _ in 0..32_768 {
        if RENDEZ_VOUS.load(Ordering::Acquire) >= etape {
            return true;
        }
        rt::call(rt::THREAD_YIELD, 0, 0, 0);
    }
    false
}

extern "C" fn membre(cle: u64) -> ! {
    RENDEZ_VOUS.store(1, Ordering::Release);
    let statut = if attendre(2) {
        rt::call(rt::IPC_NOTIFY, cle, ARGV.as_ptr() as u64, ARGV.len() as u64).status
    } else {
        rt::TIMED_OUT
    };
    rt::call(rt::THREAD_EXIT, statut, 0, 0);
    rt::exit(125)
}

pub(super) fn presenter(cle: u64, argv: &[alloc::string::String]) -> rt::Result<u64> {
    RENDEZ_VOUS.store(0, Ordering::Release);
    let thread = rt::value(rt::call(
        rt::THREAD_CREATE,
        membre as *const () as u64,
        cle,
        2,
    ))?;
    let pret = attendre(1);
    RENDEZ_VOUS.store(2, Ordering::Release);
    let principal = if pret {
        admit(cle, argv)
    } else {
        Err(rt::TIMED_OUT)
    };
    let secondaire = rt::value(rt::call(rt::THREAD_JOIN, thread, 0, 0))?;
    super::super::console::write(
        tty::ERROR,
        &alloc::format!(
            "NXVPIPE concurrence thread={} secondaire={} principal={}\n",
            thread,
            secondaire,
            principal.as_ref().map_or_else(|e| *e, |_| 0)
        ),
    )?;
    if !matches!(secondaire, rt::DENIED | rt::REVOKED) {
        return Err(rt::REFUSED);
    }
    principal
}
