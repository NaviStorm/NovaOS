use crate::{Locale, Output, engine, language::Language};
use std::{
    env,
    ffi::CStr,
    io::{self, Write},
};

// Darwin ARM64 SDK: __mbstate_t contains 128 bytes aligned to long long.
type MbState = [u64; 16];
unsafe extern "C" {
    fn mbrtowc(w: *mut libc::wchar_t, s: *const i8, n: usize, state: *mut MbState) -> usize;
    fn iswprint(w: libc::c_int) -> libc::c_int;
}
fn decode(s: &[u8]) -> Option<(u32, usize, bool)> {
    // Fresh state for the independent character at the front of this operand.
    let mut state = unsafe { core::mem::zeroed() };
    let mut w = 0;
    let n = unsafe { mbrtowc(&mut w, s.as_ptr().cast(), s.len(), &mut state) };
    (n <= s.len()).then(|| (w as u32, n.max(1), unsafe { iswprint(w as _) } != 0))
}
// Darwin wchar_t can hold a packed legacy character code. iconv performs
// Unicode-to-codeset conversion for those locales, including shift reset.
#[allow(deprecated)]
fn encode(ch: char, out: &mut Vec<u8>) -> bool {
    let mut utf8 = [0u8; 4];
    let input = ch.encode_utf8(&mut utf8);
    let codeset = unsafe { libc::nl_langinfo(libc::CODESET) };
    if unsafe { CStr::from_ptr(codeset) }
        .to_bytes()
        .eq_ignore_ascii_case(b"UTF-8")
    {
        out.extend(input.as_bytes());
        return true;
    }
    let cd = unsafe { libc::iconv_open(codeset, c"UTF-8".as_ptr()) };
    if cd as isize == -1 {
        return false;
    }
    let mut source = input.as_mut_ptr().cast();
    let mut input_left = input.len();
    let mut buf = [0u8; 64];
    let mut dest = buf.as_mut_ptr().cast();
    let mut left = buf.len();
    let count = unsafe { libc::iconv(cd, &mut source, &mut input_left, &mut dest, &mut left) };
    let ok = count != usize::MAX
        && input_left == 0
        && !(count > 0 && left == buf.len() - 1 && buf[0] == b'?')
        && unsafe {
            libc::iconv(
                cd,
                core::ptr::null_mut(),
                core::ptr::null_mut(),
                &mut dest,
                &mut left,
            )
        } != usize::MAX;
    unsafe {
        libc::iconv_close(cd);
    }
    if ok {
        out.extend(&buf[..buf.len() - left])
    }
    ok
}
// Stdout's standard-library wrapper deliberately suppresses EBADF.
struct Descriptor(libc::c_int);
impl Write for Descriptor {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        let n = unsafe { libc::write(self.0, b.as_ptr().cast(), b.len()) };
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
struct Files<'a> {
    stdout: io::BufWriter<Descriptor>,
    stderr: io::StderrLock<'a>,
    failure: Option<io::Error>,
}
impl Output for Files<'_> {
    fn write(&mut self, err: bool, b: &[u8]) -> Result<(), ()> {
        if err {
            self.stderr.write_all(b).map_err(|_| ())
        } else {
            self.stdout.write_all(b).map_err(|e| self.failure = Some(e))
        }
    }
}
// Enter through Darwin's CRT so a deliberately closed stdout stays closed.
#[unsafe(no_mangle)]
extern "C" fn main(argc: libc::c_int, argv: *const *const libc::c_char) -> libc::c_int {
    let words = unsafe { core::slice::from_raw_parts(argv, argc as usize) };
    let args: Vec<_> = words
        .iter()
        .skip(1)
        .map(|&p| unsafe { CStr::from_ptr(p) }.to_bytes())
        .collect();
    run(&args).into()
}
fn run(args: &[&[u8]]) -> u8 {
    let tag = ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .find_map(|k| env::var(k).ok().filter(|s| !s.is_empty()));
    let tag = tag
        .as_deref()
        .unwrap_or("")
        .split('.')
        .next()
        .unwrap_or("")
        .replace('_', "-");
    // Only this single-threaded CLI changes its process locale.
    unsafe {
        libc::setlocale(libc::LC_ALL, c"".as_ptr());
    }
    let conv = unsafe { &*libc::localeconv() };
    let bytes = |ptr| unsafe { CStr::from_ptr(ptr).to_bytes() };
    let ctype =
        unsafe { CStr::from_ptr(libc::setlocale(libc::LC_CTYPE, core::ptr::null())) }.to_bytes();
    let c_locale = matches!(ctype, b"C" | b"POSIX");
    let lang = Locale {
        decimal: bytes(conv.decimal_point),
        separator: bytes(conv.thousands_sep),
        groups: bytes(conv.grouping),
        decode: if c_locale { crate::ascii } else { decode },
        encode: if c_locale {
            crate::encode_ascii
        } else {
            encode
        },
        curved_quotes: unsafe { CStr::from_ptr(libc::nl_langinfo(libc::CODESET)) }
            .to_bytes()
            .eq_ignore_ascii_case(b"UTF-8"),
        french: Language::from_tag(Some(&tag)).select("en", "fr") == "fr",
        posix: env::var_os("POSIXLY_CORRECT").is_some(),
    };
    let mut io = Files {
        stdout: io::BufWriter::new(Descriptor(1)),
        stderr: io::stderr().lock(),
        failure: None,
    };
    let status = engine::run(&mut io, args, lang);
    let during_format = io.failure.is_some();
    if let Some(e) = io.failure.take().or_else(|| io.stdout.flush().err()) {
        if during_format && e.kind() == io::ErrorKind::BrokenPipe {
            let _ = writeln!(io.stderr, "printf: write error");
            return 1;
        }
        let reason = e
            .raw_os_error()
            .map(|code| {
                unsafe { CStr::from_ptr(libc::strerror(code)) }
                    .to_string_lossy()
                    .into_owned()
            })
            .unwrap_or_else(|| e.to_string());
        let _ = writeln!(io.stderr, "printf: write error: {reason}");
        return 1;
    }
    status
}
