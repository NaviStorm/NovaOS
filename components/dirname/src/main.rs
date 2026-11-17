#![cfg_attr(target_os = "none", no_std)]
#![no_main]
extern crate alloc;
#[cfg(not(target_os = "none"))]
#[path = "../../libraries/documentation.rs"]
#[allow(dead_code)]
mod documentation;
mod options;
mod path;
mod quote;
use alloc::vec::Vec;
use documentation::Language;
#[cfg(target_os = "none")]
use nova_cli::documentation;

fn document(help: bool, lang: Language) -> Vec<u8> {
    let page = documentation::lookup("dirname").unwrap();
    if help {
        page.render(&[], false, lang).unwrap().into_bytes()
    } else {
        page.version(lang).into_bytes()
    }
}

#[cfg(target_os = "none")]
fn diagnostic(reason: &[u8]) -> Vec<u8> {
    [
        b"dirname: ".as_slice(),
        reason,
        b"\nTry 'dirname --help' for more information.\n",
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
            Ok(options::Action::Paths { names, zero }) => {
                for name in names {
                    let parent = path::parent(name);
                    let mut line = if zero {
                        parent.to_vec()
                    } else {
                        quote::quoted(parent, "shell-escape", true, tty::printable)
                    };
                    line.push(if zero { 0 } else { b'\n' });
                    console::write(
                        tty::OUTPUT,
                        core::str::from_utf8(&line).map_err(|_| rt::BAD_ARGUMENT)?,
                    )?;
                }
                return Ok(0);
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
            &alloc::format!("dirname: {}\n", console::reason(code)),
        );
        1
    }))
}
