//! Client de qualification. WRITE complet puis DISCARD tient la réservation
//! du seul écrivain public tout en laissant les données publiques intactes.
use super::*;
use core::sync::atomic::{AtomicU64, Ordering};

fn report(index: usize, value: u64) {
    unsafe { (&*(0x401000 as *const AtomicU64).add(index)).store(value, Ordering::Release) }
}
fn wait(phase: u64) {
    while rt::config(17) < phase {
        rt::call(rt::THREAD_YIELD, 0, 0, 0);
    }
}
fn checkpoint(message: &[u8]) -> rt::Result<()> {
    #[allow(dead_code)]
    #[path = "../../abi/terminal.rs"]
    mod tty;
    log(message);
    ensure(rt::value(rt::call(tty::CONSOLE, u64::from(tty::INPUT), 0, 0))? == u64::from(b'!'))
}
fn available(c: &mut Client) -> rt::Result<u64> {
    Ok(w::word(&c.info()?, w::PAYLOAD + 48))
}
pub(super) fn run(mut c: Client) -> rt::Result<()> {
    c.mount()?;
    let public = c.open(b"/documents/exemple.txt", w::READABLE | w::WRITABLE)?;
    wait(1);
    let before = available(&mut c)?;
    c.write(public, 0, b"reservation P9 de qualification")?;
    let held = available(&mut c)?;
    ensure(held < before)?;
    checkpoint(b"TMP snapshot sous reservation")?;
    report(14, before);
    report(15, held);
    report(18, 1);
    wait(2);
    ensure(available(&mut c)? == held)?;
    let mut discard = w::frame(w::CLOSE, c.session, public);
    discard[2] = w::DISCARD;
    c.request(discard)?;
    // La requête publique suivante est servie après le lot de purges.
    let free = available(&mut c)?;
    checkpoint(b"TMP snapshot apres drainage")?;
    report(16, free);
    report(18, 2);
    wait(3);
    c.close(0)
}
