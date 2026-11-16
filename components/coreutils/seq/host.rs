#![no_main]
extern crate alloc;
#[allow(dead_code)]
#[path = "../../libraries/documentation.rs"]
mod documentation;
mod engine;
mod format;
mod number;
mod options;
use engine::{Action, Failure};
use std::{
    ffi::{CStr, c_char, c_void},
    io::{self, BufWriter, Write},
};

#[repr(C)]
struct NumericConventions {
    point: *const c_char,
    thousands: *const c_char,
    grouping: *const c_char,
}
unsafe extern "C" {
    fn setlocale(category: i32, locale: *const c_char) -> *const c_char;
    #[link_name = "write"]
    fn fd_write(fd: i32, data: *const c_void, len: usize) -> isize;
    fn close(fd: i32) -> i32;
    fn localeconv() -> *const NumericConventions;
}
// stdout de std ignore EBADF sur un descripteur fermé. Garder le statut
// du noyau pour les écritures partielles et la fermeture différée.
struct Output;
impl Write for Output {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let count = unsafe { fd_write(1, bytes.as_ptr().cast(), bytes.len()) };
        if count < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(count as usize)
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn locale() -> number::Locale<'static> {
    // Processus monothread. Les chaînes de la libc restent valides tant que
    // la locale ne change plus.
    unsafe {
        if setlocale(0, c"".as_ptr()).is_null() {
            return number::C;
        }
        let c = &*localeconv();
        let ctype = CStr::from_ptr(setlocale(2, core::ptr::null())).to_string_lossy();
        number::Locale {
            point: CStr::from_ptr(c.point).to_str().unwrap_or("."),
            thousands: CStr::from_ptr(c.thousands).to_str().unwrap_or(""),
            grouping: CStr::from_ptr(c.grouping).to_bytes(),
            utf8: ctype.to_ascii_lowercase().contains("utf"),
        }
    }
}

// Entrée C pour préserver la disposition héritée de SIGPIPE, y compris SIG_IGN.
#[unsafe(no_mangle)]
unsafe extern "C" fn main(argc: i32, argv: *const *const c_char) -> i32 {
    let raw = unsafe { std::slice::from_raw_parts(argv, argc as usize) };
    let args: Vec<_> = raw
        .iter()
        .skip(1)
        .map(|&p| unsafe { CStr::from_ptr(p) }.to_bytes())
        .collect();
    let loc = locale();
    let mut out = BufWriter::with_capacity(65536, Output);
    let result = engine::execute(&args, &loc, |bytes| out.write_all(bytes));
    let io_result = match result {
        Err(Failure::Arguments(message)) => {
            let _ = io::stderr().write_all(&[&b"seq: "[..], &message.0, b"\n"].concat());
            return 1;
        }
        Err(Failure::Write(e)) => Err(e),
        Ok(Action::Done) => out.flush(),
        Ok(action) => {
            let tag = ["LC_ALL", "LC_MESSAGES", "LANG"]
                .iter()
                .find_map(|k| std::env::var(k).ok().filter(|v| !v.is_empty()));
            let tag = tag
                .as_deref()
                .unwrap_or("")
                .split('.')
                .next()
                .unwrap()
                .replace('_', "-");
            let lang = documentation::Language::from_tag(Some(&tag));
            let page = documentation::lookup("seq").unwrap();
            let doc = if matches!(action, Action::Help) {
                page.render(&[], false, lang).unwrap()
            } else {
                page.version(lang)
            };
            out.write_all(doc.as_bytes()).and_then(|_| out.flush())
        }
    };
    let _ = out.into_parts(); // Une erreur ne rejoue jamais un préfixe déjà écrit.
    let closed = unsafe { close(1) };
    let io_result = io_result.and_then(|_| {
        if closed != 0 {
            let e = io::Error::last_os_error();
            if e.raw_os_error() != Some(9) {
                return Err(e);
            }
        }
        Ok(())
    });
    if let Err(e) = io_result {
        let msg = e.to_string();
        let reason = msg.split(" (os error ").next().unwrap();
        let _ = writeln!(io::stderr(), "seq: write error: {reason}");
        return 1;
    }
    0
}
