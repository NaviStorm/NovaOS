//! La sélection du parent dépend exclusivement de la commande et des autorisations
//! explicites de la session. Le shell ne lit pas le contenu des manifestes.
use super::{capabilities::Grant, paths};
use alloc::{format, string::String, vec::Vec};
use nova_cli::{
    command::Program, console, fileapi::Client, process, protocol as w, runtime as rt, tty,
};
use nova_sha256sum::options::Options;

pub fn execute<'a>(
    fs: &mut Client,
    args: &'a [String],
    allowed: impl Iterator<Item = &'a str>,
    test: bool,
    revoke: bool,
) -> rt::Result<u64> {
    let words: Vec<_> = args.iter().map(|s| s.as_bytes()).collect();
    let opt = Options::parse(&words).map_err(|_| rt::BAD_ARGUMENT)?;
    let mut names = Vec::new();
    for path in opt
        .paths
        .iter()
        .copied()
        .map(|s| core::str::from_utf8(s).unwrap())
        .chain(allowed.filter(|_| opt.check))
    {
        if path != "-" && !names.contains(&path) {
            names.push(path);
        }
    }
    let mut grant = Grant::new();
    let result = (|| {
        let mut map = String::from("\0");
        if revoke {
            map.push_str("retrait\0");
        }
        let mut handles = 0;
        if !names.is_empty() {
            fs.simple(w::STAGE, 0)?;
        }
        for name in names {
            let selected = paths::resolve(fs, name)
                .and_then(|path| fs.at(w::STAGE, 0, path.as_bytes(), w::SELECT_FILE));
            match selected {
                Ok(_) => {
                    handles += 1;
                    map.push_str(&format!("{handles}:{name}\0"));
                }
                Err(code) => map.push_str(&format!("!{code}:{name}\0")),
            }
        }
        if handles > 0 {
            grant.delegate(fs, w::GROUP, 0, &[])?;
        }
        let mut argv = alloc::vec![map];
        argv.extend_from_slice(args);
        let child = process::spawn(Program::Sha256sum, &argv, grant.channels, 0)?;
        let trial = (|| {
            if revoke && handles > 0 {
                let key = rt::value(rt::call(tty::CONSOLE, tty::INPUT.into(), 0, 0))?;
                if key != u64::from(b'+') {
                    return Err(rt::BAD_ARGUMENT);
                }
                rt::value(rt::call(rt::IPC_REVOKE, grant.channels.0, 0, 0))?;
                rt::send_for(grant.channels.1, tty::PIPE_PROBE_WAIT, b"+", 15_000)?;
            }
            Ok(())
        })();
        let (fault, status) = process::wait(child)?;
        trial?;
        if test {
            console::write(
                tty::OUTPUT,
                &format!(
                    "[statut {} {status}]\n",
                    if fault { "faute" } else { "sortie" }
                ),
            )?;
        }
        Ok(if fault { 128 } else { status })
    })();
    let released = grant.release(fs);
    result.and_then(|s| released.map(|_| s))
}
