#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(not(test), no_main)]
extern crate alloc;
#[cfg(target_os = "macos")]
#[allow(dead_code)]
#[path = "../../libraries/documentation.rs"]
mod documentation;
#[cfg(target_os = "none")]
mod exchange;
mod options;
#[cfg(target_os = "none")]
mod private;
mod quote;
mod template;
#[cfg(target_os = "none")]
mod worker;
use alloc::{string::String, vec::Vec};
#[cfg(target_os = "none")]
use nova_cli::documentation;

fn document(help: bool, lang: documentation::Language) -> String {
    let p = documentation::lookup("mktemp").unwrap();
    if help {
        p.render(&[], false, lang).unwrap()
    } else {
        p.version(lang)
    }
}

#[cfg(target_os = "macos")]
mod darwin;
#[cfg(all(target_os = "macos", not(test)))]
#[unsafe(no_mangle)]
extern "C" fn main() -> i32 {
    darwin::run()
}

#[cfg(target_os = "none")]
#[unsafe(no_mangle)]
extern "C" fn run() -> ! {
    use nova_cli::{console, process, runtime as rt, tty};
    let result = (|| {
        if rt::config(4) == 0 && rt::config(0) != 0 {
            return worker::run(rt::config(0));
        }
        let args = process::arguments()?;
        let lang = documentation::child_language(&args);
        let words: Vec<_> = args.iter().map(|s| s.as_bytes()).collect();
        let (port, code, text) = match options::parse(&words, false) {
            Ok(options::Action::Help) => (tty::OUTPUT, 0, document(true, lang)),
            Ok(options::Action::Version) => (tty::OUTPUT, 0, document(false, lang)),
            Err(reason) => (
                tty::ERROR,
                1,
                alloc::format!(
                    "mktemp: {}\nTry 'mktemp --help' for more information.\n",
                    String::from_utf8_lossy(&reason)
                ),
            ),
            Ok(options::Action::Create(opt)) => match template::prepare(&opt, b"", b"tmp") {
                Err(reason) => (
                    tty::ERROR,
                    1,
                    alloc::format!("mktemp: {}\n", String::from_utf8_lossy(&reason)),
                ),
                Ok(_) if opt.quiet => return Ok(1),
                Ok(_) => (
                    tty::ERROR,
                    1,
                    String::from("mktemp: caller temporary directory unavailable\n"),
                ),
            },
        };
        console::write(port, &text)?;
        Ok::<u64, u64>(code)
    })();
    rt::exit(result.unwrap_or(1))
}
