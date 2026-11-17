use super::*;
use std::{
    env,
    fs::File,
    io::{self, BufWriter, IsTerminal, Write},
    os::{
        fd::{FromRawFd, OwnedFd},
        unix::ffi::OsStrExt,
    },
};

unsafe extern "C" {
    fn iswprint(c: i32) -> i32;
    fn mbtowc(wc: *mut i32, s: *const libc::c_char, len: usize) -> i32;
}

// mbtowc emploie la locale du processus mono-thread, y compris ISO-8859-1.
fn quoted(bytes: &[u8], style: &str, utf8: bool) -> Vec<u8> {
    let reset = || unsafe { mbtowc(core::ptr::null_mut(), core::ptr::null(), 0) };
    reset();
    let mut hidden = Vec::with_capacity(bytes.len());
    while hidden.len() < bytes.len() {
        let rest = &bytes[hidden.len()..];
        let mut scalar = 0;
        let n = unsafe { mbtowc(&mut scalar, rest.as_ptr().cast(), rest.len()) };
        if n <= 0 {
            hidden.push(quote::ByteClass::Hidden);
            reset();
        } else {
            let visible = unsafe { iswprint(scalar) } != 0;
            quote::mark(&mut hidden, n as usize, visible);
        }
    }
    quote::render(bytes, style, utf8, &hidden)
}

fn streams() -> io::Result<BufWriter<File>> {
    // dup préserve le descripteur public et donne à File un propriétaire unique.
    let fd = unsafe { libc::dup(1) };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(BufWriter::new(File::from(unsafe {
        OwnedFd::from_raw_fd(fd)
    })))
}

fn execute() -> io::Result<u8> {
    let mut arguments = env::args_os();
    let invocation = arguments.next().unwrap_or_default();
    let args: Vec<_> = arguments.collect();
    let bytes: Vec<_> = args.iter().map(|a| a.as_bytes()).collect();
    let action = match options::parse(&bytes, env::var_os("POSIXLY_CORRECT").is_some()) {
        Ok(action) => action,
        Err(reason) => {
            let invoked = invocation.as_bytes();
            let prefix = if reason == b"missing operand" {
                b"dirname"
            } else {
                invoked
            };
            let message = [
                prefix,
                b": ",
                &reason,
                b"\nTry '",
                invoked,
                b" --help' for more information.\n",
            ]
            .concat();
            io::stderr().write_all(&message)?;
            return Ok(1);
        }
    };
    let mut out = streams()?;
    match action {
        options::Action::Help | options::Action::Version => {
            let lang = ["LC_ALL", "LC_MESSAGES", "LANG"]
                .into_iter()
                .filter_map(|k| env::var(k).ok())
                .find(|v| !v.is_empty());
            out.write_all(&document(
                matches!(action, options::Action::Help),
                Language::from_tag(lang.as_deref()),
            ))?;
        }
        options::Action::Paths { names, zero } => {
            let tty = !zero && io::stdout().is_terminal();
            let mut style = "shell-escape";
            let mut utf8 = false;
            if tty {
                // Programme mono-thread, l'encodage actif vaut pour toute la sortie.
                unsafe {
                    libc::setlocale(libc::LC_CTYPE, c"".as_ptr());
                }
                utf8 = unsafe { std::ffi::CStr::from_ptr(libc::nl_langinfo(libc::CODESET)) }
                    .to_bytes()
                    .eq_ignore_ascii_case(b"UTF-8");
                if let Some(value) = env::var_os("QUOTING_STYLE") {
                    if let Some(s) = value.to_str().and_then(quote::style) {
                        style = s;
                    } else {
                        let mut err = io::stderr().lock();
                        err.write_all(b"dirname: ignoring invalid value of environment variable QUOTING_STYLE: ")?;
                        err.write_all(&quoted(value.as_bytes(), "locale", utf8))?;
                        err.write_all(b"\n")?;
                    }
                }
            }
            for name in names {
                let parent = path::parent(name);
                if tty && style != "literal" {
                    out.write_all(&quoted(parent, style, utf8))?;
                } else {
                    out.write_all(parent)?;
                }
                out.write_all(if zero { b"\0" } else { b"\n" })?;
            }
        }
    }
    out.flush()?;
    Ok(0)
}

pub fn run() -> u8 {
    execute().unwrap_or_else(|e| {
        let reason = e
            .raw_os_error()
            .map(|n| unsafe {
                std::ffi::CStr::from_ptr(libc::strerror(n))
                    .to_string_lossy()
                    .into_owned()
            })
            .unwrap_or_else(|| e.to_string());
        let _ = writeln!(io::stderr(), "dirname: write error: {reason}");
        1
    })
}
