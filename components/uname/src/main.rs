#![cfg_attr(target_os = "none", no_std)]
#![no_main]
extern crate alloc;
use alloc::vec::Vec;
#[cfg(target_os = "none")]
use nova_cli::documentation;
#[cfg(target_os = "macos")]
#[path = "../../libraries/documentation.rs"]
#[allow(dead_code)]
mod documentation;
mod options;
use documentation::Language;
use options::{Action, Error, Selection};

const LABELS: [&[u8]; 8] = [
    b"Kernel name",
    b"Node name",
    b"Kernel release",
    b"Kernel version",
    b"Machine",
    b"Processor",
    b"Hardware platform",
    b"Operating system",
];

fn render(fields: &Selection, values: &[Option<&[u8]>; 8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut first = true;
    for (i, value) in values.iter().enumerate() {
        if fields.mask & (1 << i) == 0 || (fields.all && (i == 5 || i == 6) && value.is_none()) {
            continue;
        }
        if fields.labeled {
            out.extend(LABELS[i]);
            out.extend(b": ");
        } else if !first {
            out.push(b' ');
        }
        first = false;
        out.extend(value.unwrap_or(b"unknown"));
        if fields.labeled {
            out.push(b'\n')
        }
    }
    if !fields.labeled {
        out.push(b'\n')
    }
    out
}

fn document(action: Action, lang: Language) -> Vec<u8> {
    let page = documentation::lookup("uname").unwrap();
    match action {
        Action::Help => page.render(&[], false, lang).unwrap(),
        Action::Version => page.version(lang),
        Action::Fields(_) => unreachable!(),
    }
    .into_bytes()
}

fn diagnostic(error: &Error, utf8: bool, invoked: &[u8], program: &[u8]) -> Vec<u8> {
    let prefix = if matches!(error, Error::Operand(_)) {
        program
    } else {
        invoked
    };
    [
        prefix,
        b": ",
        &error.message(utf8),
        b"\nTry '",
        invoked,
        b" --help' for more information.\n",
    ]
    .concat()
}

#[cfg(target_os = "macos")]
mod darwin;

#[cfg(target_os = "macos")]
#[unsafe(no_mangle)]
extern "C" fn main() -> i32 {
    darwin::run()
}

#[cfg(target_os = "none")]
#[unsafe(no_mangle)]
extern "C" fn run() -> ! {
    use nova_cli::{console, process, runtime as rt, tty};
    let result = (|| {
        let args = process::arguments()?;
        let lang = documentation::child_language(&args);
        let bytes: Vec<_> = args.iter().map(|s| s.as_bytes()).collect();
        let out = match options::parse(&bytes, false) {
            Ok(Action::Fields(fields)) => {
                #[cfg(target_arch = "aarch64")]
                let machine = b"aarch64".as_slice();
                #[cfg(target_arch = "x86_64")]
                let machine = b"x86_64".as_slice();
                render(
                    &fields,
                    &[
                        Some(b"NovaOS"),
                        None,
                        Some(env!("NOVA_KERNEL_RELEASE").as_bytes()),
                        Some(concat!("NovaOS v", env!("NOVA_KERNEL_RELEASE")).as_bytes()),
                        Some(machine),
                        None,
                        None,
                        Some(b"NovaOS"),
                    ],
                )
            }
            Ok(action) => document(action, lang),
            Err(e) => {
                console::printable(
                    tty::ERROR,
                    &alloc::string::String::from_utf8_lossy(&diagnostic(
                        &e, false, b"uname", b"uname",
                    )),
                )?;
                return Ok(1);
            }
        };
        console::write(tty::OUTPUT, core::str::from_utf8(&out).unwrap())?;
        Ok(0)
    })();
    rt::exit(result.unwrap_or_else(|e| {
        let _ = console::write(
            tty::ERROR,
            &alloc::format!("uname: {}\n", console::reason(e)),
        );
        1
    }))
}
