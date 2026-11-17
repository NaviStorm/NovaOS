#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#[cfg(target_os = "none")]
#[allow(dead_code)]
mod engine;
#[cfg(target_os = "none")]
#[allow(dead_code)]
mod quoting;
#[cfg(target_os = "none")]
#[allow(dead_code)]
mod signals;
#[cfg(target_os = "none")]
#[allow(dead_code)]
mod split;

// Profil de recette indépendant. Les entrées simulées appartiennent au témoin.
#[cfg(target_os = "none")]
fn check_engine() -> bool {
    use engine::{Entries, display, parse};
    let inherited = alloc::vec![b"A=parent".to_vec()];
    let args = [b"-0S".as_slice(), b"--env0-from=- -u A X=${A}"];
    let Ok(p) = parse(
        args.iter().map(|a| a.to_vec()).collect(),
        Some(&inherited),
        &mut |_| {},
    ) else {
        return false;
    };
    let Ok(entries) = p.environment(
        inherited,
        |_| Ok(b"A=1\0raw\0\0B=1\0B=2\0".to_vec()),
        &mut |_| {},
    ) else {
        return false;
    };
    let mut bytes = alloc::vec::Vec::new();
    for e in entries {
        bytes.extend(display(&e, quoting::Style::Literal, false));
        bytes.push(0);
    }
    if bytes != b"raw\0\0B=2\0X=parent\0" || !p.zero || p.validate().is_err() {
        return false;
    }
    let Ok(p) = parse(
        alloc::vec![b"-i".to_vec(), b"--env0-from=-".to_vec()],
        Some(&Entries::new()),
        &mut |_| {},
    ) else {
        return false;
    };
    p.environment(
        Entries::new(),
        |_| Ok(b"dup=1\0dup=2\0\xff\0".to_vec()),
        &mut |_| {},
    )
    .is_ok_and(|v| v == [b"dup=1".as_slice(), b"dup=2", b"\xff"])
}
#[cfg(target_os = "none")]
#[unsafe(no_mangle)]
extern "C" fn run() -> ! {
    use nova_cli::{console, process, runtime as rt, tty};
    let args = process::arguments().unwrap();
    if args == ["--moteur"] {
        let ok = check_engine();
        if ok {
            console::write(tty::OUTPUT, "engine bytes OK\n").unwrap();
        }
        rt::exit(if ok { 0 } else { 1 });
    }
    for (i, s) in args.iter().enumerate() {
        console::printable(tty::OUTPUT, &alloc::format!("arg[{i}]={s}\n")).unwrap();
    }
    console::write(
        tty::OUTPUT,
        "cwd, argv0, environment: unavailable in startup ABI\n",
    )
    .unwrap();
    rt::exit(args.first().and_then(|s| s.parse().ok()).unwrap_or(0))
}
#[cfg(target_os = "none")]
extern crate alloc;
#[cfg(not(target_os = "none"))]
fn main() {
    use std::{
        env,
        io::{self, Write},
        os::unix::ffi::OsStrExt,
    };
    let mut out = io::stdout().lock();
    let mut field = |k: &[u8], v: &[u8]| {
        for s in [k, b"=", v, b"\0"] {
            out.write_all(s).unwrap();
        }
    };
    for (i, arg) in env::args_os().enumerate() {
        field(format!("arg[{i}]").as_bytes(), arg.as_bytes());
    }
    field(b"cwd", env::current_dir().unwrap().as_os_str().as_bytes());
    for k in ["ENV_TEST", "ENV_OTHER", "LC_ALL", "LANG"] {
        if let Some(v) = env::var_os(k) {
            field(k.as_bytes(), v.as_bytes());
        }
    }
    unsafe {
        let mut set = 0;
        libc::sigprocmask(0, std::ptr::null(), &mut set);
        let mut sa = std::mem::zeroed();
        libc::sigaction(libc::SIGUSR1, std::ptr::null(), &mut sa);
        field(
            b"USR1-block",
            if libc::sigismember(&set, libc::SIGUSR1) > 0 {
                b"yes"
            } else {
                b"no"
            },
        );
        field(
            b"USR1-ignore",
            if sa.sa_sigaction == libc::SIG_IGN {
                b"yes"
            } else {
                b"no"
            },
        );
    }
    let rc = env::args_os()
        .nth(1)
        .and_then(|s| s.to_str()?.parse::<u8>().ok())
        .unwrap_or(0);
    field(b"status", rc.to_string().as_bytes());
    out.flush().unwrap();
    std::process::exit(rc.into())
}
