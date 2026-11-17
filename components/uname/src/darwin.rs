use super::*;
use std::{
    env,
    ffi::CStr,
    io::{self, Write},
    os::unix::ffi::OsStrExt,
};

fn fields(selection: &Selection) -> io::Result<Vec<u8>> {
    let mut values: [Option<&[u8]>; 8] = [None; 8];
    let mut kernel = core::mem::MaybeUninit::<libc::utsname>::uninit();
    let overrides = [
        "UNAME_SYSNAME",
        "UNAME_NODENAME",
        "UNAME_RELEASE",
        "UNAME_VERSION",
        "UNAME_MACHINE",
    ]
    .map(env::var_os);
    if selection.mask & 31 != 0 {
        if unsafe { libc::uname(kernel.as_mut_ptr()) } != 0 {
            return Err(io::Error::last_os_error());
        }
        let kernel = unsafe { kernel.assume_init_ref() };
        for (i, bytes) in [
            &kernel.sysname,
            &kernel.nodename,
            &kernel.release,
            &kernel.version,
            &kernel.machine,
        ]
        .into_iter()
        .enumerate()
        {
            values[i] = Some(overrides[i].as_ref().map_or_else(
                || unsafe { CStr::from_ptr(bytes.as_ptr()) }.to_bytes(),
                |v| v.as_bytes(),
            ));
        }
    }
    values[5] = Some(if cfg!(target_arch = "aarch64") {
        b"arm"
    } else {
        b"i386"
    });
    values[7] = Some(b"Darwin");
    Ok(render(selection, &values))
}

fn os_error(program: &[u8], action: &[u8], e: io::Error) -> i32 {
    let n = e.raw_os_error().unwrap_or(libc::EIO);
    let msg = unsafe { CStr::from_ptr(libc::strerror(n)) }.to_bytes();
    let _ = io::stderr().write_all(&[program, b": ", action, b": ", msg, b"\n"].concat());
    1
}

// Un caractère graphique encodé sur plusieurs octets reste indivisible.
fn printable_len(bytes: &[u8]) -> usize {
    unsafe extern "C" {
        fn mbtowc(wc: *mut i32, bytes: *const libc::c_char, len: usize) -> i32;
        fn iswprint(wc: i32) -> i32;
    }
    let mut wc = 0;
    let n = unsafe { mbtowc(&mut wc, bytes.as_ptr().cast(), bytes.len()) };
    if n > 0 && unsafe { iswprint(wc) } != 0 {
        n as usize
    } else {
        0
    }
}

pub fn run() -> i32 {
    let mut words = env::args_os();
    let invoked = words.next().unwrap_or_default();
    let invoked = invoked.as_bytes();
    // Darwin err(3) emploie le nom du fichier exécutable pour les erreurs métier.
    unsafe extern "C" {
        fn getprogname() -> *const libc::c_char;
    }
    let program = unsafe { CStr::from_ptr(getprogname()) }.to_bytes();
    let args: Vec<_> = words.collect();
    let bytes: Vec<_> = args.iter().map(|s| s.as_bytes()).collect();
    let locale = ["LC_ALL", "LC_MESSAGES", "LANG"]
        .into_iter()
        .filter_map(|k| env::var(k).ok())
        .find(|s| !s.is_empty());
    let out = match options::parse(&bytes, env::var_os("POSIXLY_CORRECT").is_some()) {
        Ok(Action::Fields(s)) => match fields(&s) {
            Ok(out) => out,
            Err(e) => return os_error(program, b"cannot get system name", e),
        },
        Ok(action) => document(action, Language::from_tag(locale.as_deref())),
        Err(e) => {
            unsafe {
                libc::setlocale(libc::LC_ALL, c"".as_ptr());
            }
            let utf8 = unsafe { CStr::from_ptr(libc::nl_langinfo(libc::CODESET)) }
                .to_bytes()
                .eq_ignore_ascii_case(b"UTF-8");
            let _ = io::stderr().write_all(&diagnostic(&e, utf8, printable_len, invoked, program));
            return 1;
        }
    };
    // write(2) conserve EBADF si l'appelant a fermé stdout.
    let mut rest = out.as_slice();
    while !rest.is_empty() {
        let n = unsafe { libc::write(1, rest.as_ptr().cast(), rest.len()) };
        if n < 0 {
            let e = io::Error::last_os_error();
            if e.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return os_error(program, b"write error", e);
        }
        rest = &rest[n as usize..];
    }
    0
}
