use nova_cli::{console, fileapi::Client, options::Options, protocol as w, runtime as rt, tty};
pub fn exists(c: &mut Client, key: u64) -> rt::Result<Option<w::Frame>> {
    match c.simple(w::STAT, key) {
        Ok(st) => Ok(Some(st)),
        Err(w::NOT_FOUND) => Ok(None),
        Err(e) => Err(e),
    }
}
pub fn replacement(o: &Options, name: &str) -> rt::Result<()> {
    if o.has(b'n') {
        return Ok(());
    }
    if o.has(b'i') {
        console::printable(tty::ERROR, &alloc::format!("Remplacer {name} ? [o/N] "))?;
        let mut answer = None;
        loop {
            let b = rt::value(rt::call(tty::CONSOLE, tty::INPUT.into(), 0, 0))?;
            if matches!(b, 10 | 13) {
                break;
            }
            if answer.is_none() {
                answer = Some(b)
            }
        }
        if !matches!(answer, Some(79 | 111 | 89 | 121)) {
            return Ok(());
        }
    }
    // P9 sait renommer vers une entrée libre. Un remplacement par effacement
    // suivi d'un renommage perdrait la destination en cas d'interruption.
    Err(w::EXISTS)
}
pub fn verbose(o: &Options) -> rt::Result<()> {
    if o.has(b'v') {
        console::printable(
            tty::OUTPUT,
            &alloc::format!("{} -> {}\n", o.files[0], o.files[1]),
        )?
    }
    Ok(())
}
