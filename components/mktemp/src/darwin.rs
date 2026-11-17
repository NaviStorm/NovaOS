use super::*;
use std::{
    env,
    ffi::{CStr, OsStr},
    fs::{self, DirBuilder, OpenOptions},
    io,
    os::unix::{
        ffi::OsStrExt,
        fs::{DirBuilderExt, OpenOptionsExt},
        io::IntoRawFd,
    },
    path::Path,
};

fn reason(e: io::Error) -> Vec<u8> {
    unsafe { CStr::from_ptr(libc::strerror(e.raw_os_error().unwrap_or(libc::EIO))) }
        .to_bytes()
        .to_vec()
}
fn write(fd: i32, mut bytes: &[u8]) -> io::Result<()> {
    while !bytes.is_empty() {
        let n = unsafe { libc::write(fd, bytes.as_ptr().cast(), bytes.len()) };
        if n < 0 {
            let err = io::Error::last_os_error();
            if err.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return Err(err);
        }
        bytes = &bytes[n as usize..];
    }
    Ok(())
}
fn close(fd: i32) -> io::Result<()> {
    if unsafe { libc::close(fd) } == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}
fn emit(bytes: &[u8]) -> io::Result<()> {
    write(1, bytes)?;
    close(1)
}

fn claim(name: &[u8], opt: &options::Options) -> io::Result<bool> {
    let path = Path::new(OsStr::from_bytes(name));
    if opt.dry {
        return match fs::symlink_metadata(path) {
            Ok(_) => Ok(false),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(true),
            Err(e) => Err(e),
        };
    }
    let result = if opt.directory {
        DirBuilder::new().mode(0o700).create(path)
    } else {
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)
        {
            Ok(file) => return close(file.into_raw_fd()).map(|_| true),
            Err(e) => Err(e),
        }
    };
    match result {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Ok(false),
        Err(e) => Err(e),
    }
}
fn letters(out: &mut [u8]) {
    const ALPHABET: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";
    for c in out {
        *c = ALPHABET[unsafe { libc::arc4random_uniform(62) } as usize];
    }
}
pub fn run() -> i32 {
    let mut argv = env::args_os();
    let argv0 = argv.next().unwrap_or_default();
    let full = argv0.as_bytes();
    let prog = full.rsplit(|&c| c == b'/').next().unwrap_or(b"mktemp");
    let diagnostic = |msg: &[u8]| {
        let _ = write(2, &[prog, b": ", msg, b"\n"].concat());
        1
    };
    let args: Vec<_> = argv.collect();
    let words: Vec<_> = args.iter().map(|s| s.as_bytes()).collect();
    let opt = match options::parse(&words, env::var_os("POSIXLY_CORRECT").is_some()) {
        Err(msg) => {
            let prefix = if msg == b"too many templates" {
                prog
            } else {
                full
            };
            let _ = write(
                2,
                &[
                    prefix,
                    b": ",
                    &msg,
                    b"\nTry '",
                    full,
                    b" --help' for more information.\n",
                ]
                .concat(),
            );
            return 1;
        }
        Ok(options::Action::Create(o)) => o,
        Ok(action) => {
            let tag = ["LC_ALL", "LC_MESSAGES", "LANG"]
                .into_iter()
                .filter_map(|k| env::var(k).ok())
                .find(|s| !s.is_empty());
            return match emit(
                document(
                    matches!(action, options::Action::Help),
                    documentation::Language::from_tag(tag.as_deref()),
                )
                .as_bytes(),
            ) {
                Ok(()) => 0,
                Err(e) => diagnostic(&[b"write error: ".as_slice(), &reason(e)].concat()),
            };
        }
    };
    let envdir = env::var_os("TMPDIR").unwrap_or_default();
    let t = match template::prepare(&opt, envdir.as_bytes(), b"/tmp") {
        Ok(t) => t,
        Err(msg) => return diagnostic(&msg),
    };
    let result = template::reserve(&t, letters, |n| claim(n, &opt))
        .and_then(|v| v.ok_or_else(|| io::Error::from_raw_os_error(libc::EEXIST)));
    let name = match result {
        Ok(n) => n,
        Err(e) => {
            if opt.quiet {
                return 1;
            }
            let kind = if opt.directory {
                b"directory".as_slice()
            } else {
                b"file"
            };
            return diagnostic(
                &[
                    b"failed to create ",
                    kind,
                    b" via template '",
                    &quote::escaped(&t.name),
                    b"': ",
                    &reason(e),
                ]
                .concat(),
            );
        }
    };
    let path = OsStr::from_bytes(&name);
    if let Err(e) = emit(&[name.as_slice(), b"\n"].concat()) {
        if !opt.dry {
            if opt.directory {
                let _ = fs::remove_dir(path);
            } else {
                let _ = fs::remove_file(path);
            }
        }
        if opt.quiet && !opt.dry {
            return 1;
        }
        return diagnostic(&[b"write error: ".as_slice(), &reason(e)].concat());
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn collision_reelle_ne_modifie_ni_fichier_ni_lien() {
        use std::os::unix::fs::symlink;
        let root = env::var_os("MKTEMP_TEST_AREA").unwrap();
        let dir = Path::new(&root);
        let occupied = dir.join("arc.000");
        let target = dir.join("temoin");
        fs::write(&target, b"ARC-027").unwrap();
        symlink(&target, &occupied).unwrap();
        for (directory, dry) in [(false, false), (true, false), (false, true)] {
            let raw = dir.join("arc.XXX");
            let opt = options::Options {
                directory,
                dry,
                template: Some(raw.as_os_str().as_bytes()),
                ..Default::default()
            };
            let t = template::prepare(&opt, b"", b"").unwrap();
            let mut count = 0;
            let result = template::reserve(
                &t,
                |letters| {
                    letters.copy_from_slice(if count == 0 { b"000" } else { b"001" });
                    count += 1;
                },
                |p| claim(p, &opt),
            )
            .unwrap()
            .unwrap();
            assert_eq!(count, 2);
            let new = Path::new(OsStr::from_bytes(&result));
            if dry {
                assert!(!new.exists());
            } else if directory {
                fs::remove_dir(new).unwrap();
            } else {
                fs::remove_file(new).unwrap();
            }
            assert_eq!(fs::read_link(&occupied).unwrap(), target);
            assert_eq!(fs::read(&target).unwrap(), b"ARC-027");
        }
    }
}
