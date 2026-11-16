use crate::{
    diagnostic,
    documentation::{self, Language},
    engine,
    options::{self, Action},
    quoting::{Style, quote},
};
use std::{
    env,
    ffi::CStr,
    io::{self, IsTerminal, Write},
    os::unix::ffi::OsStrExt,
};

struct Output;

impl Write for Output {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        unsafe extern "C" {
            fn write(fd: i32, buf: *const core::ffi::c_void, len: usize) -> isize;
        }
        let n = unsafe { write(1, bytes.as_ptr().cast(), bytes.len()) };
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

/// Convertir les caractères de la locale Darwin tout en conservant les octets source.
pub fn native_units(mut data: &[u8], mut visit: impl FnMut(&[u8], bool)) {
    unsafe extern "C" {
        fn mbrtowc(wide: *mut i32, src: *const u8, len: usize, state: *mut u64) -> usize;
        fn iswprint(wide: i32) -> i32;
    }
    // arm/_types.h : mbstate_t est une union de 128 octets alignée sur 8.
    let mut state = [0u64; 16];
    while !data.is_empty() {
        let mut wide = 0;
        let n = unsafe { mbrtowc(&mut wide, data.as_ptr(), data.len(), state.as_mut_ptr()) };
        let valid = n > 0 && n <= data.len();
        let len = if valid {
            n
        } else {
            state.fill(0);
            1
        };
        visit(&data[..len], valid && unsafe { iswprint(wide) != 0 });
        data = &data[len..];
    }
}

fn run(argv: &[&[u8]]) -> Result<(), u8> {
    let args = &argv[1..];
    let locale = ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .find_map(|key| env::var(key).ok().filter(|v| !v.is_empty()));
    let language = Language::from_tag(locale.as_deref());
    unsafe extern "C" {
        fn setlocale(category: i32, name: *const i8) -> *const i8;
        fn nl_langinfo(item: i32) -> *const i8;
    }
    // Darwin, LC_CTYPE=2. Une locale inconnue laisse la locale de démarrage C.
    let utf8 = unsafe {
        setlocale(2, c"".as_ptr());
        CStr::from_ptr(nl_langinfo(0))
            .to_bytes()
            .eq_ignore_ascii_case(b"UTF-8")
    };
    let program = argv[0];
    let mut err = io::stderr().lock();
    let mut report = |message: &[u8]| {
        for part in [program, b": ", message, b"\n"] {
            let _ = err.write_all(part);
        }
    };
    let action = options::parse(args).map_err(|problem| {
        report(&diagnostic::message(problem, utf8));
        let _ = io::stderr().write_all(
            &[
                b"Try '".as_slice(),
                program,
                b" --help' for more information.\n",
            ]
            .concat(),
        );
        1
    })?;
    let stdout = io::stdout();
    let tty = stdout.is_terminal();
    // Le wrapper Stdout de Rust absorbe EBADF. Écrire sur le descripteur
    // hérité conserve le statut réel des erreurs de destination.
    let mut out = io::BufWriter::new(Output);
    let write = || -> io::Result<()> {
        match action {
            Action::Names {
                paths,
                suffix,
                zero,
            } => {
                let style = if !tty || zero {
                    Style::Literal
                } else {
                    env::var_os("QUOTING_STYLE").map_or(Style::ShellEscape, |raw| {
                        Style::parse(raw.as_bytes()).unwrap_or_else(|| {
                            report(&[b"ignoring invalid value of environment variable QUOTING_STYLE: ".as_slice(),
                                &quote(raw.as_bytes(),Style::Locale,utf8)].concat());
                            Style::ShellEscape
                        })
                    })
                };
                for path in paths {
                    let (name, delimiter) = engine::record(path, suffix, zero);
                    if style == Style::Literal {
                        out.write_all(name)?
                    } else {
                        out.write_all(&quote(name, style, utf8))?
                    }
                    out.write_all(&[delimiter])?;
                }
            }
            doc => {
                let page = documentation::lookup("basename").unwrap();
                let text = if matches!(doc, Action::Help) {
                    page.render(&[], false, language).unwrap()
                } else {
                    page.version(language)
                };
                out.write_all(text.as_bytes())?;
            }
        }
        out.flush()
    };
    write().map_err(|e| {
        unsafe extern "C" {
            fn strerror(code: i32) -> *const i8;
        }
        let reason = e
            .raw_os_error()
            .map(|code| unsafe { CStr::from_ptr(strerror(code)) }.to_bytes())
            .unwrap_or(b"I/O error");
        report(&[b"write error: ".as_slice(), reason].concat());
        1
    })
}
// Entrée C de Darwin. Le processus conserve ainsi le traitement de SIGPIPE
// hérité du parent et les descripteurs fermés, comme les autres outils Unix.
#[unsafe(no_mangle)]
unsafe extern "C" fn main(argc: i32, argv: *const *const i8) -> i32 {
    let pointers = unsafe { core::slice::from_raw_parts(argv, argc as usize) };
    let args: Vec<_> = pointers
        .iter()
        .map(|p| unsafe { CStr::from_ptr(*p) }.to_bytes())
        .collect();
    run(&args).err().unwrap_or(0).into()
}
