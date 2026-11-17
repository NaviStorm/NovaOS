use super::*;
use std::{
    env,
    ffi::OsStr,
    fs,
    io::{self, IsTerminal, Read, Write},
    os::unix::{ffi::OsStrExt, fs::MetadataExt},
    path::Path,
    process::{Command, Stdio},
};
// Darwin exécute ce constructeur avant que std n'ignore SIGPIPE. Conserver
// le choix du parent permet aux consommateurs de fermer le pipe normalement.
static PIPE_IGNORED: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(true);
#[used]
#[unsafe(link_section = "__DATA,__mod_init_func")]
static PIPE_INIT: extern "C" fn() = {
    extern "C" fn remember_pipe() {
        let mut action = core::mem::MaybeUninit::<libc::sigaction>::uninit();
        // Un pointeur d'action nul interroge uniquement la disposition héritée.
        unsafe {
            if libc::sigaction(libc::SIGPIPE, core::ptr::null(), action.as_mut_ptr()) == 0 {
                PIPE_IGNORED.store(
                    action.assume_init().sa_sigaction == libc::SIG_IGN,
                    core::sync::atomic::Ordering::Relaxed,
                );
            }
        }
    }
    remember_pipe
};
fn path(b: &[u8]) -> &Path {
    Path::new(OsStr::from_bytes(b))
}
fn fail(p: &[u8], e: io::Error) -> String {
    let msg = e.to_string();
    format!(
        "{}: {}",
        txt(p),
        msg.split(" (os error ").next().unwrap_or(&msg)
    )
}
struct Host;
fn stamp(sec: i64, ns: i64, context: bool) -> Vec<u8> {
    let mut time = core::mem::MaybeUninit::uninit();
    // localtime_r initialise tm à partir du mtime et de TZ du processus.
    unsafe {
        if libc::localtime_r(&sec, time.as_mut_ptr()).is_null() {
            return vec![];
        }
    }
    let time = unsafe { time.assume_init() };
    let mut buf = [0u8; 96];
    let fmt = if context {
        c"%a %b %e %T %Y"
    } else {
        c"%Y-%m-%d %H:%M:%S"
    };
    let n = unsafe { libc::strftime(buf.as_mut_ptr().cast(), buf.len(), fmt.as_ptr(), &time) };
    let mut s = buf[..n].to_vec();
    if !context {
        let n =
            unsafe { libc::strftime(buf.as_mut_ptr().cast(), buf.len(), c"%z".as_ptr(), &time) };
        s.extend(format!(".{ns:09} ").bytes());
        s.extend(&buf[..n])
    }
    s
}
impl Platform for Host {
    fn stdin_child(&self, name: &[u8]) -> Result<Vec<u8>, String> {
        let mut buf = [0u8; libc::PATH_MAX as usize];
        if unsafe { libc::fcntl(0, libc::F_GETPATH, buf.as_mut_ptr()) } < 0 {
            return Err(fail(b"-", io::Error::last_os_error()));
        }
        let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
        Ok([&buf[..end], b"/", name].concat())
    }
    fn utf8(&self) -> bool {
        ["LC_ALL", "LC_CTYPE", "LANG"]
            .iter()
            .find_map(|k| env::var(k).ok().filter(|s| !s.is_empty()))
            .is_some_and(|s| s.to_ascii_uppercase().contains("UTF"))
    }
    fn dumb(&self) -> bool {
        env::var("TERM").as_deref() == Ok("dumb")
    }
    fn stat(&mut self, p: &[u8], o: &Options) -> Result<Option<Meta>, String> {
        let result = if p == b"-" {
            fs::metadata("/dev/fd/0")
        } else if o.has("no-dereference") {
            fs::symlink_metadata(path(p))
        } else {
            fs::metadata(path(p))
        };
        match result {
            Ok(m) => Ok(Some(Meta {
                kind: if m.is_dir() {
                    Kind::Dir
                } else if m.is_symlink() {
                    Kind::Link
                } else if m.is_file() {
                    Kind::File
                } else {
                    Kind::Other
                },
                id: Some((m.dev(), m.ino())),
                stamp: stamp(m.mtime(), m.mtime_nsec(), o.style == "context"),
            })),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(fail(p, e)),
        }
    }
    fn read(&mut self, p: &[u8]) -> Result<Vec<u8>, String> {
        if p == b"-" {
            let mut b = vec![];
            io::stdin()
                .lock()
                .read_to_end(&mut b)
                .map_err(|e| fail(p, e))?;
            Ok(b)
        } else {
            fs::read(path(p)).map_err(|e| fail(p, e))
        }
    }
    fn list(&mut self, p: &[u8]) -> Result<Vec<Vec<u8>>, String> {
        fs::read_dir(path(p))
            .map_err(|e| fail(p, e))?
            .map(|e| {
                e.map(|v| v.file_name().as_bytes().to_vec())
                    .map_err(|e| fail(p, e))
            })
            .collect()
    }
    fn link(&mut self, p: &[u8]) -> Result<Vec<u8>, String> {
        fs::read_link(path(p))
            .map(|s| s.as_os_str().as_bytes().to_vec())
            .map_err(|e| fail(p, e))
    }
    fn emit(&mut self, err: bool, b: &[u8]) -> Result<(), String> {
        if err {
            io::stderr().lock().write_all(b)
        } else {
            {
                let mut output = io::stdout().lock();
                output.write_all(b).and_then(|()| output.flush())
            }
        }
        .map_err(|e| {
            fail(
                if err {
                    b"standard error"
                } else {
                    b"standard output"
                },
                e,
            )
        })
    }
    fn paginate(&mut self, title: &[u8], b: &[u8]) -> Result<Vec<u8>, String> {
        let mut child = Command::new("/usr/bin/pr")
            .arg("-h")
            .arg(OsStr::from_bytes(title))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .map_err(|e| e.to_string())?;
        let mut stdin = child.stdin.take().unwrap();
        std::thread::scope(|scope| {
            let writer = scope.spawn(move || stdin.write_all(b));
            let out = child.wait_with_output().map_err(|e| e.to_string())?;
            writer
                .join()
                .map_err(|_| "pr writer")?
                .map_err(|e| e.to_string())?;
            if out.status.success() {
                Ok(out.stdout)
            } else {
                Err("pr failed".into())
            }
        })
    }
}
pub fn run() -> u8 {
    if !PIPE_IGNORED.load(core::sync::atomic::Ordering::Relaxed) {
        // exec remet les gestionnaires personnalisés à leur disposition par défaut.
        unsafe {
            libc::signal(libc::SIGPIPE, libc::SIG_DFL);
        }
    }
    let args: Vec<_> = env::args_os().collect();
    let argv: Vec<_> = args.iter().skip(1).map(|a| a.as_bytes()).collect();
    let fr =
        locale::Language::from_tag(env::var("LANG").ok().as_deref()) == locale::Language::French;
    execute(
        &mut Host,
        &argv,
        args[0].as_bytes(),
        fr,
        env::var_os("POSIXLY_CORRECT").is_some(),
        io::stdout().is_terminal() && env::var("TERM").as_deref() != Ok("dumb"),
    )
}
