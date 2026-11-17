use super::*;
use nova_cli::{
    command::{Program, operand},
    process, runtime as rt, tty,
};

fn admit(key: u64, argv: &[alloc::string::String]) -> rt::Result<u64> {
    if argv.len() > tty::ARG_COUNT {
        return Err(rt::LIMIT);
    }
    let mut bytes = alloc::vec![argv.len() as u8];
    for s in argv {
        let n = u16::try_from(s.len()).map_err(|_| rt::LIMIT)?;
        if bytes.len() + 2 + s.len() > tty::ARG_MAX {
            return Err(rt::LIMIT);
        }
        bytes.extend_from_slice(&n.to_le_bytes());
        bytes.extend_from_slice(s.as_bytes());
    }
    rt::value(rt::call(
        rt::IPC_NOTIFY,
        key,
        bytes.as_ptr() as u64,
        bytes.len() as u64,
    ))
}
pub(super) fn run(
    login: &mut super::session_auth_lab::Login,
    fs: &mut nova_cli::fileapi::Client,
    writer: Program,
    argv: &[alloc::string::String],
    consumer: super::executor::PipeReader,
) -> rt::Result<u64> {
    login.authenticate()?;
    let args: alloc::vec::Vec<_> = argv.iter().map(|s| s.as_str()).collect();
    let selection = operand(writer, &args)?;
    let control = rt::value(rt::call(tty::SPAWN, tty::PIPE_OPEN, 0, 0))?;
    let mut reservation = 0;
    let mut produced = None;
    let mut read = None;
    let run: rt::Result<()> = (|| {
        let key = rt::value(rt::call(
            rt::IPC_NOTIFY,
            control,
            0x4e5856505245,
            writer as u64,
        ))?;
        reservation = key;
        let mut grant = super::capabilities::Grant::new();
        grant.select_private(fs, &selection, key)?;
        produced = Some(admit(key, argv)?);
        let (reader, _, options) = consumer.spec();
        let words = options
            .iter()
            .copied()
            .map(alloc::string::String::from)
            .collect::<alloc::vec::Vec<_>>();
        let key = rt::value(rt::call(
            rt::IPC_NOTIFY,
            control,
            0x4e5856524452,
            reader as u64,
        ))?;
        login.confirm_live()?;
        let wait = admit(key, &words)?;
        read = Some(wait);
        login.drain_catalog(wait & !(1u64 << 63))?;
        Ok(())
    })();
    if run.is_err() && reservation != 0 {
        rt::call(rt::IPC_NOTIFY, control, 0x4e585643414e, reservation);
    }
    let writer_status = produced.map(process::wait).transpose();
    let reader_status = read.map(process::wait).transpose();
    rt::call(rt::IPC_NOTIFY, control, 0x4e585643414e, reservation);
    let closed = rt::value(rt::call(rt::IPC_CLOSE, control, 0, 0));
    run?;
    closed?;
    let (wf, ws) = writer_status?.ok_or(rt::REFUSED)?;
    let (rf, rs) = reader_status?.ok_or(rt::REFUSED)?;
    if option_env!("NOVA_DISPLAY_TRACE") == Some("1") {
        super::console::write(
            tty::ERROR,
            &alloc::format!(
                "NXVPIPE producteur={} statut={} lecteur={} statut={}\n",
                produced.unwrap() & !(1u64 << 63),
                if wf { 128 } else { ws },
                read.unwrap() & !(1u64 << 63),
                if rf { 128 } else { rs }
            ),
        )?;
    }
    Ok(if rf { 128 } else { rs })
}

/// Témoin exécuté dans le vrai shell de qualification. Les erreurs proviennent
/// des portes IPC et des décisions VFS habituelles.
pub(super) fn probe(
    login: &mut super::session_auth_lab::Login,
    fs: &mut nova_cli::fileapi::Client,
    args: &[&str],
) -> rt::Result<u64> {
    let [mode] = args else {
        return Err(rt::BAD_ARGUMENT);
    };
    login.authenticate()?;
    let open = || rt::value(rt::call(tty::SPAWN, tty::PIPE_OPEN, 0, 0));
    let close = |h| rt::value(rt::call(rt::IPC_CLOSE, h, 0, 0));
    let notify = |ctrl, op, arg| rt::value(rt::call(rt::IPC_NOTIFY, ctrl, op, arg));
    let control = open()?;
    let mut reservation = 0;
    let mut producer = None;
    let result = (|| {
        if *mode == "quota" {
            let fourth = open()?;
            let fifth = open();
            close(fourth)?;
            return if fifth == Err(rt::LIMIT) {
                Ok(())
            } else {
                Err(rt::REFUSED)
            };
        }
        let key = notify(control, 0x4e5856505245, Program::Cat as u64)?;
        reservation = key;
        if *mode == "collision" {
            if notify(control, 0x4e585643414e, 0) != Err(rt::REVOKED) {
                return Err(rt::REFUSED);
            }
            let other = open()?;
            let wrong = notify(other, 0x4e585643414e, key);
            let busy = notify(other, 0x4e5856505245, Program::Cat as u64);
            close(other)?;
            if wrong != Err(rt::REVOKED) || busy != Err(rt::LIMIT) {
                return Err(rt::REFUSED);
            }
        }
        let argv = [alloc::string::String::from("lignes.tsv")];
        let selection = operand(Program::Cat, &["lignes.tsv"])?;
        super::capabilities::Grant::new().select_private(fs, &selection, key)?;
        match *mode {
            "expire" => {
                // IPC_RECEIVE compte des ticks à 250 Hz. 2 600 ticks couvrent
                // 10,4 s, au-delà des 10 s du raccord et dans la session de 60 s.
                if rt::receive_for(rt::config(17), 2600).err() != Some(rt::TIMED_OUT) {
                    return Err(rt::REFUSED);
                }
                if admit(key, &argv).is_ok() {
                    return Err(rt::REFUSED);
                }
            }
            "revoque" => {
                login.close()?;
                if admit(key, &argv).is_ok() {
                    return Err(rt::REFUSED);
                }
            }
            "collision" | "partiel" => {
                producer = Some(admit(key, &argv)?);
                if admit(key, &argv).is_ok() {
                    return Err(rt::REFUSED);
                }
                if notify(control, 0x4e5856524452, u64::MAX) != Err(rt::BAD_ARGUMENT) {
                    return Err(rt::REFUSED);
                }
            }
            _ => return Err(rt::BAD_ARGUMENT),
        }
        Ok(())
    })();
    let _ = notify(control, 0x4e585643414e, reservation);
    if let Some(w) = producer {
        let status = process::wait(w)?;
        if status != (false, 125) || process::wait(w).is_ok() {
            return Err(rt::REFUSED);
        }
    }
    let closed = close(control);
    result?;
    if !matches!(closed, Ok(_) | Err(rt::DENIED | rt::REVOKED | rt::CLOSED)) {
        return Err(rt::REFUSED);
    }
    super::console::write(
        tty::ERROR,
        &alloc::format!("NXVPIPE temoin {mode} termine\n"),
    )?;
    Ok(0)
}
