use crate::{
    documentation,
    format::{self, Value as V},
    options,
    quoting::{Style, quote},
};
use std::{
    env,
    ffi::{CStr, CString, OsStr},
    io::{self, Write},
    mem::MaybeUninit,
    os::unix::ffi::OsStrExt,
    path::Path,
};
include!("darwin_locale.rs");
fn os_error() -> String {
    unsafe {
        CStr::from_ptr(libc::strerror(*libc::__error()))
            .to_string_lossy()
            .into_owned()
    }
}
fn volume(path: &CStr) -> Result<libc::statfs, String> {
    let mut v = MaybeUninit::uninit();
    if unsafe { libc::statfs(path.as_ptr(), v.as_mut_ptr()) } < 0 {
        return Err(os_error());
    }
    Ok(unsafe { v.assume_init() })
}
fn fsfield(v: &libc::statfs, k: &[u8]) -> V {
    match k {
        b"a" => V::dec(v.f_bavail),
        b"b" => V::dec(v.f_blocks),
        b"c" => V::dec(v.f_files),
        b"d" => V::dec(v.f_ffree),
        b"f" => V::dec(v.f_bfree),
        b"i" => {
            let words: [u32; 2] = unsafe { std::mem::transmute(v.f_fsid) };
            V::hex((u64::from(words[0]) << 32) | u64::from(words[1]))
        }
        b"l" => V::text("?"),
        b"s" | b"S" => V::dec(v.f_bsize.into()),
        b"t" => V::hex(v.f_type.into()),
        b"T" => V::text(unsafe { CStr::from_ptr(v.f_fstypename.as_ptr()) }.to_bytes()),
        _ => V::Unknown,
    }
}
fn kind(s: &libc::stat) -> (&'static str, u8) {
    match s.st_mode & libc::S_IFMT {
        libc::S_IFDIR => ("directory", b'd'),
        libc::S_IFLNK => ("symbolic link", b'l'),
        libc::S_IFCHR => ("character special file", b'c'),
        libc::S_IFBLK => ("block special file", b'b'),
        libc::S_IFSOCK => ("socket", b's'),
        libc::S_IFIFO => ("fifo", b'p'),
        _ if s.st_size == 0 => ("regular empty file", b'-'),
        _ => ("regular file", b'-'),
    }
}
fn permissions(s: &libc::stat) -> Vec<u8> {
    let mut out = b"-rwxrwxrwx".to_vec();
    out[0] = kind(s).1;
    for n in 1..10 {
        if s.st_mode & (1 << (9 - n)) == 0 {
            out[n] = b'-'
        }
    }
    for (n, bit, on, off) in [
        (3, 0o4000, b's', b'S'),
        (6, 0o2000, b's', b'S'),
        (9, 0o1000, b't', b'T'),
    ] {
        if s.st_mode & bit != 0 {
            out[n] = if out[n] == b'x' { on } else { off }
        }
    }
    out
}
fn stamp(sec: i64, ns: i64) -> V {
    let mut t = MaybeUninit::uninit();
    let mut buf = [0i8; 64];
    let mut zone = [0i8; 16];
    unsafe {
        if libc::localtime_r(&sec, t.as_mut_ptr()).is_null() {
            return V::Unknown;
        }
        libc::strftime(
            buf.as_mut_ptr(),
            64,
            c"%Y-%m-%d %H:%M:%S".as_ptr(),
            t.as_ptr(),
        );
        libc::strftime(zone.as_mut_ptr(), 16, c"%z".as_ptr(), t.as_ptr());
        V::text(format!(
            "{}.{ns:09} {}",
            CStr::from_ptr(buf.as_ptr()).to_string_lossy(),
            CStr::from_ptr(zone.as_ptr()).to_string_lossy()
        ))
    }
}
fn mount_point(name: &CStr, s: &libc::stat) -> V {
    use std::os::unix::fs::MetadataExt;
    let path = Path::new(OsStr::from_bytes(name.to_bytes()));
    let dir = if kind(s).1 == b'd' {
        path
    } else {
        path.parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."))
    };
    let Ok(mut dir) = dir.canonicalize() else {
        return V::Unknown;
    };
    let Ok(st) = dir.metadata() else {
        return V::Unknown;
    };
    while let Some(parent) = dir.parent() {
        let Ok(up) = parent.metadata() else {
            return V::Unknown;
        };
        if up.dev() != st.dev() {
            break;
        }
        dir.pop();
    }
    V::text(dir.as_os_str().as_bytes())
}
fn filefield(s: &libc::stat, k: &[u8], path: &CStr) -> V {
    let dev = s.st_dev as u32;
    let rdev = s.st_rdev as u32;
    match k {
        b"a" => V::Number(u64::from(s.st_mode & 0o7777), 8),
        b"A" => V::Text(permissions(s)),
        b"b" => V::dec(s.st_blocks as u64),
        b"B" => V::dec(512),
        b"d" => V::dec(dev.into()),
        b"D" => V::hex(dev.into()),
        b"Hd" => V::dec((dev >> 24).into()),
        b"Ld" => V::dec((dev & 0xffffff).into()),
        b"f" => V::hex(s.st_mode.into()),
        b"F" => V::text(kind(s).0),
        b"g" => V::dec(s.st_gid.into()),
        b"u" => V::dec(s.st_uid.into()),
        b"U" | b"G" => unsafe {
            let ptr = if k == b"U" {
                libc::getpwuid(s.st_uid).as_ref().map(|p| p.pw_name)
            } else {
                libc::getgrgid(s.st_gid).as_ref().map(|p| p.gr_name)
            };
            ptr.map_or(V::text("UNKNOWN"), |p| {
                V::text(CStr::from_ptr(p).to_bytes())
            })
        },
        b"h" => V::dec(s.st_nlink.into()),
        b"i" => V::dec(s.st_ino),
        b"o" => V::dec(s.st_blksize as u64),
        b"s" => V::dec(s.st_size as u64),
        b"m" => mount_point(path, s),
        b"r" => V::dec(rdev.into()),
        b"R" => V::hex(rdev.into()),
        b"Hr" => V::dec((rdev >> 24).into()),
        b"Lr" => V::dec((rdev & 0xffffff).into()),
        b"t" => V::hex((rdev >> 24).into()),
        b"T" => V::hex((rdev & 0xffffff).into()),
        b"x" | b"X" | b"y" | b"Y" | b"z" | b"Z" | b"w" | b"W" => {
            let (sec, ns) = match k[0].to_ascii_lowercase() {
                b'x' => (s.st_atime, s.st_atime_nsec),
                b'y' => (s.st_mtime, s.st_mtime_nsec),
                b'z' => (s.st_ctime, s.st_ctime_nsec),
                _ => (s.st_birthtime, s.st_birthtime_nsec),
            };
            if k == b"w" && sec == 0 && ns == 0 {
                V::text("-")
            } else if k[0].is_ascii_uppercase() {
                V::Epoch(sec, ns as u32)
            } else {
                stamp(sec, ns)
            }
        }
        _ => V::Unknown,
    }
}
pub fn run() -> i32 {
    let args: Vec<_> = env::args_os().skip(1).collect();
    let args: Vec<_> = args.iter().map(|a| a.as_bytes()).collect();
    let utf8 = unsafe {
        libc::setlocale(libc::LC_CTYPE, c"".as_ptr());
        CStr::from_ptr(libc::nl_langinfo(libc::CODESET))
            .to_bytes()
            .eq_ignore_ascii_case(b"UTF-8")
    };
    let opt = match options::parse(&args, env::var_os("POSIXLY_CORRECT").is_some()) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("stat: {e}\nTry 'stat --help' for more information.");
            return 1;
        }
    };
    use std::os::fd::FromRawFd;
    // Le processus possède le descripteur hérité et se termine après le dernier flush.
    let mut out = io::BufWriter::new(unsafe { std::fs::File::from_raw_fd(1) });
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }

    if let Some(help) = opt.doc {
        let lang = ["LC_ALL", "LC_MESSAGES", "LANG"]
            .iter()
            .find_map(|k| env::var(k).ok().filter(|s| !s.is_empty()));
        let lang = documentation::Language::from_tag(lang.as_deref());
        let page = documentation::lookup("stat").unwrap();
        let text = if help {
            page.render(&[], false, lang).unwrap()
        } else {
            page.version(lang)
        };
        return i32::from(
            out.write_all(text.as_bytes())
                .and_then(|_| out.flush())
                .is_err(),
        );
    }
    let requested = env::var_os("QUOTING_STYLE");
    let parsed = requested.as_ref().and_then(|s| Style::parse(s.as_bytes()));
    let mut invalid_style = requested.as_ref().filter(|_| parsed.is_none());
    let style = parsed.unwrap_or(Style::ShellEscape);
    let ordinary = format::template(&opt, false);
    let device = format::template(&opt, true);
    let mut bytes = Vec::new();
    let mut failed = false;
    for &name in &opt.files {
        let path = CString::new(name).unwrap();
        let quoted =
            || String::from_utf8_lossy(&quote(name, Style::ShellEscapeAlways, utf8)).into_owned();
        let mut stat = MaybeUninit::uninit();
        let mut target = None;
        let mut fs = None;
        if opt.fs {
            if name == b"-" {
                eprintln!(
                    "stat: using '-' to denote standard input does not work in file system mode"
                );
                failed = true;
                continue;
            }
            match volume(&path) {
                Ok(v) => fs = Some(v),
                Err(e) => {
                    eprintln!(
                        "stat: cannot read file system information for {}: {e}",
                        quoted()
                    );
                    failed = true;
                    continue;
                }
            }
        } else {
            let rc = unsafe {
                if name == b"-" {
                    libc::fstat(0, stat.as_mut_ptr())
                } else if opt.follow {
                    libc::stat(path.as_ptr(), stat.as_mut_ptr())
                } else {
                    libc::lstat(path.as_ptr(), stat.as_mut_ptr())
                }
            };
            if rc < 0 {
                let e = os_error();
                eprintln!("stat: cannot stat {}: {e}", quoted());
                failed = true;
                continue;
            }
            target = Some(unsafe { stat.assume_init() })
        }
        let fmt = if target
            .as_ref()
            .is_some_and(|s| matches!(kind(s).1, b'b' | b'c'))
        {
            &device
        } else {
            &ordinary
        };
        bytes.clear();
        let result = format::render(
            fmt,
            opt.escapes,
            opt.fs,
            |k| match k {
                b"n" => V::text(name),
                b"Qn" | b"N" if !opt.fs || k == b"Qn" => {
                    if let Some(s) = invalid_style.take() {
                        eprintln!(
                            "stat: ignoring invalid value of environment variable QUOTING_STYLE: '{}'",
                            s.to_string_lossy()
                        );
                    }
                    let q = quote(name, style, utf8);
                    if k == b"N" && target.as_ref().is_some_and(|s| kind(s).1 == b'l') {
                        match std::fs::read_link(Path::new(OsStr::from_bytes(name))) {
                            Ok(p) => {
                                return V::Link(q, quote(p.as_os_str().as_bytes(), style, utf8));
                            }
                            Err(e) => {
                                failed = true;
                                eprintln!("stat: cannot read symbolic link {}: {e}", quoted())
                            }
                        }
                    }
                    V::Text(q)
                }
                b"C" if !opt.fs => {
                    eprintln!(
                        "stat: failed to get security context of {}: Operation not supported",
                        quoted()
                    );
                    failed = true;
                    V::text("?")
                }
                _ => {
                    if let Some(v) = &fs {
                        fsfield(v, k)
                    } else {
                        filefield(target.as_ref().unwrap(), k, &path)
                    }
                }
            },
            &mut bytes,
            |w| eprintln!("stat: {w}"),
        );
        if out.write_all(&bytes).is_err() {
            return 1;
        }
        if let Err(e) = result {
            eprintln!("stat: {e}");
            failed = true;
            break;
        }
    }
    i32::from(out.flush().is_err() || failed)
}
