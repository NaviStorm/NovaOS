use crate::{
    documentation,
    engine::*,
    quoting::{Style, quote},
    signals::{NAMES, Signals},
};
struct Output;
impl Write for Output {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        let n = unsafe { libc::write(1, data.as_ptr().cast(), data.len()) };
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
pub fn utf8() -> bool {
    unsafe {
        CStr::from_ptr(libc::nl_langinfo(libc::CODESET))
            .to_bytes()
            .eq_ignore_ascii_case(b"UTF-8")
    }
}
use std::{
    ffi::{CStr, CString, OsStr},
    io::{self, IsTerminal, Read, Write},
    os::unix::ffi::OsStrExt,
    ptr,
};
pub fn native_units(mut data: &[u8], mut visit: impl FnMut(&[u8], bool)) {
    unsafe extern "C" {
        fn mbrtowc(w: *mut i32, s: *const u8, n: usize, state: *mut u64) -> usize;
        fn iswprint(c: i32) -> i32;
    }
    let mut state = [0u64; 16];
    while !data.is_empty() {
        let mut w = 0;
        let n = unsafe { mbrtowc(&mut w, data.as_ptr(), data.len(), state.as_mut_ptr()) };
        let valid = n > 0 && n <= data.len();
        let n = if valid {
            n
        } else {
            state.fill(0);
            1
        };
        visit(&data[..n], valid && unsafe { iswprint(w) != 0 });
        data = &data[n..];
    }
}
fn os_error(msg: Word, err: io::Error) -> Error {
    let reason = unsafe { CStr::from_ptr(libc::strerror(err.raw_os_error().unwrap_or(libc::EIO))) }
        .to_bytes();
    Error::new(cat(&[&msg, b": ", reason]))
}
fn read_file(name: &[u8]) -> Result<Word, Error> {
    let read = || -> io::Result<Word> {
        if name == b"-" {
            let mut bytes = Vec::new();
            io::stdin().lock().read_to_end(&mut bytes)?;
            Ok(bytes)
        } else {
            std::fs::read(OsStr::from_bytes(name))
        }
    };
    read().map_err(|e| {
        os_error(
            cat(&[
                b"cannot read ",
                &quote(name, Style::ShellEscapeAlways, false),
            ]),
            e,
        )
    })
}
fn trace(line: &[u8]) {
    let _ = io::stderr().lock().write_all(line);
}
fn apply_signals(s: &Signals, debug: bool) -> Result<(), Error> {
    unsafe {
        let mut mask = 0;
        if libc::sigprocmask(0, ptr::null(), &mut mask) != 0 {
            return Err(os_error(
                b"failed to get signal process mask".to_vec(),
                io::Error::last_os_error(),
            ));
        }
        for (n, opt) in s.handler.iter().enumerate().skip(1) {
            let Some((ignore, all)) = *opt else { continue };
            let mut action = std::mem::zeroed();
            if libc::sigaction(n as i32, ptr::null(), &mut action) != 0 && !all {
                return Err(os_error(
                    format!("failed to get signal action for signal {n}").into_bytes(),
                    io::Error::last_os_error(),
                ));
            }
            action.sa_sigaction = if ignore { libc::SIG_IGN } else { libc::SIG_DFL };
            let failed = libc::sigaction(n as i32, &action, ptr::null_mut()) != 0;
            if failed && !all {
                return Err(os_error(
                    format!("failed to set signal action for signal {n}").into_bytes(),
                    io::Error::last_os_error(),
                ));
            }
            if debug {
                trace(
                    format!(
                        "Reset signal {} ({n}) to {}{}\n",
                        NAMES[n],
                        if ignore { "IGNORE" } else { "DEFAULT" },
                        if failed { " (failure ignored)" } else { "" }
                    )
                    .as_bytes(),
                )
            }
        }
        for (n, opt) in s.block.iter().enumerate().skip(1) {
            if let Some(block) = opt {
                if *block {
                    libc::sigaddset(&mut mask, n as i32);
                } else {
                    libc::sigdelset(&mut mask, n as i32);
                }
                if debug {
                    trace(
                        format!(
                            "signal {} ({n}) mask set to {}\n",
                            NAMES[n],
                            if *block { "BLOCK" } else { "UNBLOCK" }
                        )
                        .as_bytes(),
                    )
                }
            }
        }
        if s.block.iter().any(Option::is_some)
            && libc::sigprocmask(libc::SIG_SETMASK, &mask, ptr::null_mut()) != 0
        {
            return Err(os_error(
                b"failed to set signal process mask".to_vec(),
                io::Error::last_os_error(),
            ));
        }
        if s.list {
            libc::sigprocmask(0, ptr::null(), &mut mask);
            for (n, name) in NAMES.iter().enumerate().skip(1) {
                let mut action = std::mem::zeroed();
                if libc::sigaction(n as i32, ptr::null(), &mut action) != 0 {
                    continue;
                }
                let flags = match (
                    libc::sigismember(&mask, n as i32) > 0,
                    action.sa_sigaction == libc::SIG_IGN,
                ) {
                    (true, true) => "BLOCK,IGNORE",
                    (true, false) => "BLOCK",
                    (false, true) => "IGNORE",
                    _ => continue,
                };
                trace(format!("{name:<10} ({n:2}): {flags}\n").as_bytes())
            }
        }
    }
    Ok(())
}
fn run(args: Entries, inherited: Entries, program: &[u8]) -> Result<(), Error> {
    unsafe {
        libc::setlocale(libc::LC_CTYPE, c"".as_ptr());
    }
    let utf8 = utf8();
    let p = parse(args, Some(&inherited), &mut trace)?;
    let language = ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .find_map(|s| value(&inherited, s.as_bytes()).filter(|x| !x.is_empty()))
        .and_then(|v| std::str::from_utf8(v).ok());
    // Utiliser le fd hérité, même lorsqu'il est fermé : stdout() absorbe EBADF.
    let mut out = io::BufWriter::new(Output);
    let write_error = |e| os_error(b"write error".to_vec(), e);
    if let Some(help) = p.doc {
        let page = documentation::lookup("env").unwrap();
        let lang = documentation::Language::from_tag(language);
        let text = if help {
            page.render(&[], false, lang).unwrap()
        } else {
            page.version(lang)
        };
        out.write_all(text.as_bytes())
            .and_then(|_| out.flush())
            .map_err(write_error)?;
        return Ok(());
    }
    let mut style = Style::ShellEscape;
    if !p.zero && io::stdout().is_terminal() {
        if let Some(v) = value(&inherited, b"QUOTING_STYLE") {
            style = Style::parse(v).unwrap_or_else(|| {
                trace(&cat(&[
                    program,
                    b": ignoring invalid value of environment variable QUOTING_STYLE: ",
                    &quoted(v),
                    b"\n",
                ]));
                Style::ShellEscape
            })
        }
    }
    if !io::stdout().is_terminal() || p.zero {
        style = Style::Literal
    }
    if p.empty && p.debug {
        trace(b"clearing environment\n")
    }
    let entries = p.environment(inherited, read_file, &mut trace)?;
    p.validate()?;
    if p.command.is_empty() {
        for e in entries {
            let row = display(&e, style, utf8);
            out.write_all(&row)
                .and_then(|_| out.write_all(if p.zero { b"\0" } else { b"\n" }))
                .map_err(write_error)?;
        }
        return out.flush().map_err(write_error);
    }
    apply_signals(&p.signals, p.debug)?;
    if let Some(dir) = p.cwd {
        if p.debug {
            trace(&cat(&[b"change dir: ", &quoted(&dir), b"\n"]))
        }
        std::env::set_current_dir(OsStr::from_bytes(&dir)).map_err(|e| {
            os_error(
                cat(&[
                    b"cannot change directory to ",
                    &quote(&dir, Style::ShellEscapeAlways, utf8),
                ]),
                e,
            )
        })?;
    }
    let mut args = p.command;
    let command = CString::new(args[0].as_slice()).unwrap();
    if let Some(name) = p.argv0 {
        if p.debug {
            trace(&cat(&[b"argv0:     ", &quoted(&name), b"\n"]))
        }
        args[0] = name;
    }
    if p.debug {
        trace(&cat(&[b"executing: ", &quoted(command.as_bytes()), b"\n"]));
        for (i, a) in args.iter().enumerate() {
            trace(&cat(&[
                format!("   arg[{i}]= ").as_bytes(),
                &quoted(a),
                b"\n",
            ]))
        }
    }
    let args: Vec<_> = args.into_iter().map(|s| CString::new(s).unwrap()).collect();
    let env: Vec<_> = entries
        .into_iter()
        .map(|s| CString::new(s).unwrap())
        .collect();
    let argv: Vec<_> = args
        .iter()
        .map(|s| s.as_ptr().cast_mut())
        .chain([ptr::null_mut()])
        .collect();
    let mut envp: Vec<_> = env
        .iter()
        .map(|s| s.as_ptr().cast_mut())
        .chain([ptr::null_mut()])
        .collect();
    // Processus monothread : execvp doit chercher PATH dans le nouvel environnement,
    // y compris lorsqu'il contient des entrées brutes impossibles via setenv.
    let err = unsafe {
        let slot = libc::_NSGetEnviron();
        let saved = *slot;
        *slot = envp.as_mut_ptr();
        libc::execvp(command.as_ptr(), argv.as_ptr().cast());
        let e = io::Error::last_os_error();
        *slot = saved;
        e
    };
    let code = if err.raw_os_error() == Some(libc::ENOENT) {
        127
    } else {
        126
    };
    let mut e = os_error(quoted(command.as_bytes()), err);
    if code == 127
        && command
            .as_bytes()
            .iter()
            .any(|b| b.is_ascii_whitespace() || *b == 11)
    {
        e.message.extend(cat(&[
            b"\n",
            program,
            b": use -[v]S to pass options in shebang lines",
        ]));
    }
    e.code = code;
    Err(e)
}
#[unsafe(no_mangle)]
unsafe extern "C" fn main(argc: i32, argv: *const *const i8, envp: *const *const i8) -> i32 {
    let args: Entries = unsafe { std::slice::from_raw_parts(argv, argc as usize) }
        .iter()
        .map(|p| unsafe { CStr::from_ptr(*p) }.to_bytes().to_vec())
        .collect();
    let invocation = args[0].clone();
    let program = invocation.rsplit(|b| *b == b'/').next().unwrap_or(b"env");
    let mut entries = Vec::new();
    let mut ptr = envp;
    unsafe {
        while !(*ptr).is_null() {
            entries.push(CStr::from_ptr(*ptr).to_bytes().to_vec());
            ptr = ptr.add(1);
        }
    }
    if let Err(e) = run(args.into_iter().skip(1).collect(), entries, program) {
        trace(&cat(&[
            if e.getopt { &invocation } else { program },
            b": ",
            &e.message,
            b"\n",
        ]));
        if e.shebang {
            trace(&cat(&[
                program,
                b": use -[v]S to pass options in shebang lines\n",
            ]));
        }
        if e.usage {
            trace(&cat(&[
                b"Try '",
                &invocation,
                b" --help' for more information.\n",
            ]));
        }
        return e.code.into();
    }
    0
}
