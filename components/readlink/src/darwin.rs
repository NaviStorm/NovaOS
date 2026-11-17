use super::*;
use std::{
    env,
    io::{self, BufWriter, IsTerminal, Write},
    os::unix::ffi::OsStrExt,
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

struct Darwin;
impl resolver::Namespace for Darwin {
    type Error = io::Error;
    fn cwd(&self) -> io::Result<Vec<u8>> {
        Ok(env::current_dir()?.as_os_str().as_bytes().to_vec())
    }
    fn link(&self, path: &[u8]) -> io::Result<Vec<u8>> {
        Ok(std::fs::read_link(std::ffi::OsStr::from_bytes(path))?
            .as_os_str()
            .as_bytes()
            .to_vec())
    }
    fn search_directory(&self, path: &[u8]) -> io::Result<()> {
        let check = [path, b"/"].concat();
        std::fs::metadata(std::ffi::OsStr::from_bytes(&check)).map(|_| ())
    }
    fn ordinary(e: &io::Error) -> bool {
        e.raw_os_error() == Some(libc::EINVAL)
    }
    fn missing(e: &io::Error) -> bool {
        e.raw_os_error() == Some(libc::ENOENT)
    }
    fn empty() -> io::Error {
        io::Error::from_raw_os_error(libc::ENOENT)
    }
    fn cycle() -> io::Error {
        io::Error::from_raw_os_error(libc::ELOOP)
    }
}

// Un descripteur hérité fermé ne devient une erreur que lors d’une écriture.
struct Stdout;
impl Write for Stdout {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let n = unsafe { libc::write(1, bytes.as_ptr().cast(), bytes.len()) };
        if n < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(n as usize)
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn execute() -> io::Result<u8> {
    let mut arguments = env::args_os();
    let invocation = arguments.next().unwrap_or_default();
    let args: Vec<_> = arguments.collect();
    let bytes: Vec<_> = args.iter().map(|a| a.as_bytes()).collect();
    let mut status = 0;
    let action = match options::parse(&bytes, env::var_os("POSIXLY_CORRECT").is_some()) {
        Ok(action) => action,
        Err(reason) => {
            let invoked = invocation.as_bytes();
            let prefix = if reason == b"missing operand" {
                invoked.rsplit(|&b| b == b'/').next().unwrap()
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
    let mut out = BufWriter::new(Stdout);
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
        options::Action::Paths { names, flags } => {
            let tty = !flags.zero && io::stdout().is_terminal();
            let mut style = "shell-escape";

            // Programme mono-thread, l'encodage actif vaut pour toute la sortie.
            unsafe {
                libc::setlocale(libc::LC_CTYPE, c"".as_ptr());
            }
            let utf8 = unsafe { std::ffi::CStr::from_ptr(libc::nl_langinfo(libc::CODESET)) }
                .to_bytes()
                .eq_ignore_ascii_case(b"UTF-8");
            if tty {
                if let Some(value) = env::var_os("QUOTING_STYLE") {
                    if let Some(s) = value.to_str().and_then(quote::style) {
                        style = s;
                    } else {
                        let mut err = io::stderr().lock();
                        err.write_all(b"readlink: ignoring invalid value of environment variable QUOTING_STYLE: ")?;
                        err.write_all(&quoted(value.as_bytes(), "locale", utf8))?;
                        err.write_all(b"\n")?;
                    }
                }
            }
            let omit = flags.no_newline && names.len() == 1;
            if flags.no_newline && !omit {
                io::stderr()
                    .write_all(b"readlink: ignoring --no-newline with multiple arguments\n")?;
            }
            for name in names {
                let value = match resolver::resolve(&Darwin, name, flags.mode) {
                    Ok(v) => v,
                    Err(e) => {
                        status = 1;
                        if flags.verbose {
                            let mut err = io::stderr().lock();
                            err.write_all(b"readlink: ")?;
                            if name.is_empty() {
                                err.write_all(b"''")?;
                            } else {
                                err.write_all(&quoted(name, "shell-escape", utf8))?;
                            }
                            let reason = unsafe {
                                std::ffi::CStr::from_ptr(libc::strerror(
                                    e.raw_os_error().unwrap_or(libc::EIO),
                                ))
                            };
                            err.write_all(b": ")?;
                            err.write_all(reason.to_bytes())?;
                            err.write_all(b"\n")?;
                        }
                        continue;
                    }
                };
                if tty && style != "literal" {
                    out.write_all(&quoted(&value, style, utf8))?;
                } else {
                    out.write_all(&value)?;
                }
                if !omit {
                    out.write_all(if flags.zero { b"\0" } else { b"\n" })?;
                }
            }
        }
    }
    out.flush()?;
    Ok(status)
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
        let _ = writeln!(io::stderr(), "readlink: write error: {reason}");
        1
    })
}
