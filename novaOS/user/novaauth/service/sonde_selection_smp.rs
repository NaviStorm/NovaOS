//! Sonde interprocessus du profil --selection-smp. Le shell et NovaFS
//! présentent le raccord sur les portes normales pendant le même échange.
use super::*;
use nova_cli::protocol as w;

extern "C" fn fin_temoin(_: u64) -> ! {
    rt::call(rt::THREAD_EXIT, 0, 0, 0);
    rt::exit(125)
}

pub(super) fn presenter(
    fs: &mut nova_cli::fileapi::Client,
    cle: u64,
    argv: &[alloc::string::String],
) -> rt::Result<u64> {
    // nsh est un titulaire unique. Aucun droit THREAD_CREATE n'est ajouté à la sonde.
    let thread = rt::call(rt::THREAD_CREATE, fin_temoin as *const () as u64, 0, 2);
    if thread.status != rt::DENIED {
        if thread.status == rt::OK {
            rt::call(rt::THREAD_JOIN, thread.value, 0, 0);
        }
        return Err(rt::REFUSED);
    }
    if admit(cle, argv) != Err(rt::REFUSED) {
        return Err(rt::REFUSED);
    }
    fs.simple(w::STAGE, 0)?;
    let path = super::super::paths::resolve(fs, "lignes.tsv")?;
    fs.at(w::STAGE, 0, path.as_bytes(), w::SELECT_FILE)?;
    let (tx, rx, tag) = fs.transaction_sonde()?;
    let frame = w::frame(0xe8, fs.session, cle);
    rt::send_for(tx, tag, &frame, 2500)?;
    // Le service est libre de valider avant ou après ce premier appel.
    let essai = admit(cle, argv);
    let ack = rt::receive_for(rx, 2500)?;
    if ack.kind != 1
        || ack.tag != tag
        || ack.len != 128
        || ack.payload[..2] != frame[..2]
        || ack.payload[3] != 0
    {
        return Err(rt::REFUSED);
    }
    let wait = match essai {
        Ok(wait) => wait,
        Err(rt::REFUSED) => admit(cle, argv)?,
        Err(e) => return Err(e),
    };
    super::super::console::write(
        tty::ERROR,
        &alloc::format!(
            "NXVPIPE concurrence instance_unique={} premier={} pid={}\n",
            thread.status,
            essai.map_or_else(|e| e, |_| 0),
            wait & !(1u64 << 63)
        ),
    )?;
    Ok(wait)
}
