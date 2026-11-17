#![cfg_attr(target_os = "none", no_std)]
#![no_main]
extern crate alloc;
#[cfg(not(target_os = "none"))]
#[path = "../../libraries/documentation.rs"]
#[allow(dead_code)]
mod documentation;
mod options;
mod path;
#[cfg(not(target_os = "none"))]
mod quote;
use alloc::vec::Vec;
use documentation::Language;
#[cfg(target_os = "none")]
use nova_cli::documentation;

fn document(help: bool, lang: Language) -> Vec<u8> {
    let page = documentation::lookup("realpath").unwrap();
    if help {
        page.render(&[], false, lang).unwrap().into_bytes()
    } else {
        page.version(lang).into_bytes()
    }
}

#[cfg(target_os = "none")]
fn diagnostic(reason: &[u8]) -> Vec<u8> {
    [
        b"realpath: ".as_slice(),
        reason,
        b"\nTry 'realpath --help' for more information.\n",
    ]
    .concat()
}

#[cfg(not(target_os = "none"))]
mod darwin;
#[cfg(not(target_os = "none"))]
#[unsafe(no_mangle)]
extern "C" fn main() -> i32 {
    // Conserver stdout fermé si le parent le transmet ainsi. Le démarrage
    // standard Rust rouvre les descripteurs manquants sur /dev/null.
    i32::from(darwin::run())
}

#[cfg(target_os = "none")]
#[unsafe(no_mangle)]
extern "C" fn run() -> ! {
    use nova_cli::{console, process, runtime as rt, tty};
    let result = (|| {
        let args = process::arguments()?;
        let lang = documentation::child_language(&args);
        let bytes: Vec<_> = args.iter().map(|s| s.as_bytes()).collect();
        let output = match options::parse(&bytes, false) {
            Ok(options::Action::Help) => document(true, lang),
            Ok(options::Action::Version) => document(false, lang),
            Ok(options::Action::Paths(opt)) => {
                let mut fs = Session;
                let relative = match path::Relative::new(&mut fs, &opt) {
                    Ok(r) => r,
                    Err((name, e)) => {
                        guest_error(name, e)?;
                        return Ok(1);
                    }
                };
                let mut status = 0;
                for name in &opt.names {
                    match path::resolve(&mut fs, name, &opt) {
                        Err(e) => {
                            status = 1;
                            if !opt.quiet {
                                guest_error(name, e)?
                            }
                        }
                        Ok(p) => {
                            let mut line = relative.display(p);
                            line.push(if opt.zero { 0 } else { b'\n' });
                            console::write(
                                tty::OUTPUT,
                                core::str::from_utf8(&line).map_err(|_| rt::BAD_ARGUMENT)?,
                            )?
                        }
                    }
                }
                return Ok(status);
            }
            Err(reason) => {
                console::printable(
                    tty::ERROR,
                    &alloc::string::String::from_utf8_lossy(&diagnostic(&reason)),
                )?;
                return Ok(1);
            }
        };
        console::write(tty::OUTPUT, core::str::from_utf8(&output).unwrap())?;
        Ok(0)
    })();
    rt::exit(result.unwrap_or_else(|code| {
        let _ = console::write(
            tty::ERROR,
            &alloc::format!("realpath: {}\n", console::reason(code)),
        );
        1
    }))
}

#[cfg(target_os = "none")]
struct Session;
#[cfg(target_os = "none")]
impl path::Namespace for Session {
    fn cwd(&mut self) -> Result<Vec<u8>, path::Error> {
        let mut client =
            nova_cli::fileapi::Client::scoped().map_err(|_| path::Error::Unavailable)?;
        nova_cli::directory::cwd(&mut client)
            .map(|s| s.into_bytes())
            .map_err(|_| path::Error::Unavailable)
    }
    // La délégation CWD_VIEW ne donne aucun droit LOOKUP_AT sur un chemin.
    fn link(&mut self, _: &[u8]) -> Result<Option<Vec<u8>>, path::Error> {
        Err(path::Error::Unavailable)
    }
    fn check(&mut self, _: &[u8], _: bool) -> Result<(), path::Error> {
        Err(path::Error::Unavailable)
    }
}
#[cfg(target_os = "none")]
fn guest_error(name: &[u8], e: path::Error) -> nova_cli::runtime::Result<()> {
    let why = match e {
        path::Error::Missing => "No such file or directory",
        path::Error::Loop => "Too many levels of symbolic links",
        path::Error::NotDirectory => "Not a directory",
        _ => "session namespace unavailable",
    };
    nova_cli::console::printable(
        nova_cli::tty::ERROR,
        &alloc::format!(
            "realpath: {}: {why}\n",
            alloc::string::String::from_utf8_lossy(name)
        ),
    )
}
