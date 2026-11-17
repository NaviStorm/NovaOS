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

use path::{Error, Namespace, Relative};
use std::ffi::OsStr;
fn errno(e: io::Error) -> Error {
    match e.raw_os_error().unwrap_or(libc::EIO) {
        libc::ENOENT => Error::Missing,
        libc::ENOTDIR => Error::NotDirectory,
        libc::ELOOP => Error::Loop,
        n => Error::System(n),
    }
}
struct Darwin;
impl Namespace for Darwin {
    fn cwd(&mut self) -> Result<Vec<u8>, Error> {
        env::current_dir()
            .map(|s| s.as_os_str().as_bytes().to_vec())
            .map_err(errno)
    }
    fn link(&mut self, path: &[u8]) -> Result<Option<Vec<u8>>, Error> {
        match std::fs::read_link(OsStr::from_bytes(path)) {
            Ok(s) => Ok(Some(s.as_os_str().as_bytes().to_vec())),
            Err(e) if e.raw_os_error() == Some(libc::EINVAL) => Ok(None),
            Err(e) => Err(errno(e)),
        }
    }
    fn check(&mut self, path: &[u8], directory: bool) -> Result<(), Error> {
        let p = std::path::Path::new(OsStr::from_bytes(path));
        let m = std::fs::metadata(p).map_err(errno)?;
        if directory && !m.is_dir() {
            return Err(Error::NotDirectory);
        }
        Ok(())
    }
}
fn report(name: &[u8], e: Error) -> io::Result<()> {
    let code = match e {
        Error::Missing => libc::ENOENT,
        Error::Loop => libc::ELOOP,
        Error::NotDirectory => libc::ENOTDIR,
        Error::System(n) => n,
        Error::Unavailable => libc::EPERM,
    };
    let mut err = io::stderr().lock();
    err.write_all(b"realpath: ")?;
    err.write_all(&quoted(name, "shell-escape", false))?;
    err.write_all(b": ")?;
    err.write_all(unsafe { std::ffi::CStr::from_ptr(libc::strerror(code)) }.to_bytes())?;
    err.write_all(b"\n")
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
                b"realpath"
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
    let mut status = 0;
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
        options::Action::Paths(opt) => {
            let zero = opt.zero;
            let mut fs = Darwin;
            let relative = match Relative::new(&mut fs, &opt) {
                Ok(v) => v,
                Err((p, e)) => {
                    report(p, e)?;
                    return Ok(1);
                }
            };
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
                        err.write_all(b"realpath: ignoring invalid value of environment variable QUOTING_STYLE: ")?;
                        err.write_all(&quoted(value.as_bytes(), "locale", utf8))?;
                        err.write_all(b"\n")?;
                    }
                }
            }
            for name in &opt.names {
                let resolved = match path::resolve(&mut fs, name, &opt) {
                    Ok(p) => relative.display(p),
                    Err(e) => {
                        status = 1;
                        if !opt.quiet {
                            report(name, e)?
                        }
                        continue;
                    }
                };
                let parent = &resolved;
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
        let _ = writeln!(io::stderr(), "realpath: write error: {reason}");
        1
    })
}
