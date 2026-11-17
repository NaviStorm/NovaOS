use crate::{
    args::{self, Action, Config},
    engine::{self, Io, Stats},
};
#[allow(dead_code)]
#[path = "../libraries/documentation.rs"]
mod documentation;
use std::{
    env,
    ffi::{CStr, OsString},
    fs::{File, OpenOptions},
    io::{self, Read, Seek, SeekFrom, Write},
    os::unix::{
        ffi::OsStrExt,
        fs::OpenOptionsExt,
        io::{AsRawFd, FromRawFd, IntoRawFd},
    },
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};
type Result<T> = engine::Result<T>;
static STDERR_CLOSED: AtomicBool = AtomicBool::new(false);
static STDERR_FAILED: AtomicBool = AtomicBool::new(false);

// Le runtime Rust rouvre les descripteurs fermés sur /dev/null avant main.
// Conserver l’état hérité pour rendre l’échec d’une écriture de statistiques.
#[used]
#[unsafe(link_section = "__DATA,__mod_init_func")]
static INSPECT_STDERR: extern "C" fn() = {
    extern "C" fn inspect() {
        let closed = unsafe { libc::fcntl(libc::STDERR_FILENO, libc::F_GETFD) } == -1;
        STDERR_CLOSED.store(closed, Ordering::Relaxed);
    }
    inspect
};

fn diagnostic(args: core::fmt::Arguments<'_>) {
    if STDERR_CLOSED.load(Ordering::Relaxed) || io::stderr().lock().write_fmt(args).is_err() {
        STDERR_FAILED.store(true, Ordering::Relaxed);
    }
}

static INTERRUPTED: AtomicBool = AtomicBool::new(false);
static INFO: AtomicBool = AtomicBool::new(false);
extern "C" fn info(sig: i32) {
    if sig == libc::SIGINT {
        INTERRUPTED.store(true, Ordering::Relaxed);
    } else {
        INFO.store(true, Ordering::Relaxed);
    }
}
fn errno(error: io::Error) -> String {
    let number = error.raw_os_error().unwrap_or(libc::EIO);
    unsafe { CStr::from_ptr(libc::strerror(number)) }
        .to_string_lossy()
        .into_owned()
}
use args::quote::file as quote_file;
fn name(path: Option<&[u8]>, default: &str) -> Vec<u8> {
    path.unwrap_or(default.as_bytes()).to_vec()
}
fn open(path: Option<&[u8]>, input: bool, c: &Config) -> Result<File> {
    let flags = if input { &c.inf } else { &c.outf };
    let mut bits = 0;
    for flag in flags {
        bits |= match *flag {
            "append" => libc::O_APPEND,
            "directory" => libc::O_DIRECTORY,
            "dsync" => libc::O_DSYNC,
            "sync" => libc::O_SYNC,
            "nonblock" => libc::O_NONBLOCK,
            "noctty" => libc::O_NOCTTY,
            "nofollow" => libc::O_NOFOLLOW,
            _ => 0,
        }
    }
    if let Some(path) = path {
        let mut options = OpenOptions::new();
        options.read(input).write(!input).custom_flags(bits);
        if !input {
            options
                .create(!c.has("nocreat"))
                .create_new(c.has("excl"))
                .truncate(!c.has("notrunc") && c.seek / (c.obs as u64) == 0);
        }
        return options
            .open(std::ffi::OsStr::from_bytes(path))
            .map_err(|e| format!("failed to open {}: {}", quote_file(path, true), errno(e)));
    }
    let fd = unsafe { libc::dup(if input { 0 } else { 1 }) };
    if fd < 0 {
        return Err(errno(io::Error::last_os_error()));
    }
    let file = unsafe { File::from_raw_fd(fd) };
    let label = if input {
        "standard input"
    } else {
        "standard output"
    };
    if bits & libc::O_DIRECTORY != 0 && !file.metadata().map_err(errno)?.is_dir() {
        return Err(format!("setting flags for '{label}': Not a directory"));
    }
    if bits != 0 {
        let old = unsafe { libc::fcntl(fd, libc::F_GETFL) };
        if old < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, old | bits) } < 0 {
            return Err(format!(
                "setting flags for '{label}': {}",
                errno(io::Error::last_os_error())
            ));
        }
    }
    Ok(file)
}
struct Host {
    input: File,
    output: File,
    src: Vec<u8>,
    dst: Vec<u8>,
    sparse: bool,
    hole: bool,
    written: usize,
    start: Instant,
    next: Duration,
    status: &'static str,
    progress_line: bool,
    last: Stats,
}
impl Host {
    fn interrupt(&mut self) {
        if INFO.swap(false, Ordering::Relaxed) {
            self.summary(&self.last.clone());
        }
        if INTERRUPTED.load(Ordering::Relaxed) {
            self.summary(&self.last.clone());
            unsafe {
                libc::signal(libc::SIGINT, libc::SIG_DFL);
                libc::raise(libc::SIGINT);
            }
        }
    }
    fn transfer(&self, s: &Stats) -> String {
        let elapsed = self.start.elapsed().as_secs_f64().max(1e-9);
        let n = s.bytes;
        fn units(n: f64, base: f64, labels: &[&str]) -> String {
            let mut v = n;
            let mut i = 0;
            while (v >= base || (i > 0 && v.round_ties_even() >= base)) && i + 1 < labels.len() {
                v /= base;
                i += 1
            }
            if i == 0 || v >= 10. {
                format!("{v:.0} {}", labels[i])
            } else {
                format!("{v:.1} {}", labels[i])
            }
        }
        let decimal = ["B", "kB", "MB", "GB", "TB", "PB", "EB"];
        let binary = ["B", "KiB", "MiB", "GiB", "TiB", "PiB", "EiB"];
        let sizes = if n < 1000 {
            String::new()
        } else if n < 1024 {
            format!(" ({})", units(n as f64, 1000., &decimal))
        } else {
            format!(
                " ({}, {})",
                units(n as f64, 1000., &decimal),
                units(n as f64, 1024., &binary)
            )
        };
        format!(
            "{n} byte{}{sizes} copied, {elapsed:.6} s, {}/s",
            if n == 1 { "" } else { "s" },
            units(n as f64 / elapsed, 1000., &decimal)
        )
    }
    fn summary(&mut self, s: &Stats) {
        if self.status == "none" {
            return;
        }
        if self.progress_line {
            diagnostic(format_args!("\n"));
            self.progress_line = false
        }
        diagnostic(format_args!("{}", s.records()));
        if self.status != "noxfer" {
            diagnostic(format_args!("{}\n", self.transfer(s)))
        }
    }
    fn finish(&mut self, c: &Config) -> Result<()> {
        if self.hole {
            let pos = self.output.stream_position().map_err(errno)?;
            let meta = self.output.metadata().map_err(errno)?;
            if meta.is_file() && meta.len() < pos {
                self.output.set_len(pos).map_err(errno)?
            }
        }
        if c.has("fsync") || c.has("fdatasync") {
            if unsafe { libc::fsync(self.output.as_raw_fd()) } < 0 {
                return Err(format!(
                    "fsync failed for {}: {}",
                    quote_file(&self.dst, false),
                    errno(io::Error::last_os_error())
                ));
            }
        }
        if c.count == Some(0) && c.tail == 0 {
            for (f, flags, label) in [
                (&mut self.input, &c.inf, &self.src),
                (&mut self.output, &c.outf, &self.dst),
            ] {
                if flags.contains(&"nocache") {
                    let cause = f
                        .stream_position()
                        .err()
                        .map(errno)
                        .unwrap_or_else(|| "Operation not supported".into());
                    return Err(format!(
                        "failed to discard cache for: {}: {cause}",
                        quote_file(label, false)
                    ));
                }
            }
        }
        Ok(())
    }
}
impl Io for Host {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        loop {
            match self.input.read(buf) {
                Err(e) if e.kind() == io::ErrorKind::Interrupted => self.interrupt(),
                r => {
                    return r.map_err(|e| {
                        format!(
                            "error reading {}: {}",
                            quote_file(&self.src, true),
                            errno(e)
                        )
                    });
                }
            }
        }
    }
    fn write(&mut self, data: &[u8]) -> Result<()> {
        self.hole = false;
        self.written = 0;
        if self.sparse && data.iter().all(|b| *b == 0) {
            if self
                .output
                .seek(SeekFrom::Current(data.len() as i64))
                .is_ok()
            {
                self.hole = true;
                return Ok(());
            }
            self.sparse = false
        }
        while self.written < data.len() {
            match self.output.write(&data[self.written..]) {
                Ok(0) => {
                    return Err(format!(
                        "error writing {}: No space left on device",
                        quote_file(&self.dst, true)
                    ));
                }
                Ok(n) => self.written += n,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => self.interrupt(),
                Err(e) => {
                    return Err(format!(
                        "error writing {}: {}",
                        quote_file(&self.dst, true),
                        errno(e)
                    ));
                }
            }
        }
        Ok(())
    }
    fn after_read_error(&mut self, bytes: u64) -> Result<bool> {
        match self.input.seek(SeekFrom::Current(
            i64::try_from(bytes).map_err(|_| "skip offset overflow")?,
        )) {
            Ok(_) => Ok(!self.input.metadata().map_err(errno)?.is_dir()),
            Err(e) if e.raw_os_error() == Some(libc::ESPIPE) => Ok(true),
            Err(e) => Err(errno(e)),
        }
    }
    fn written_before_error(&self) -> usize {
        self.written
    }
    fn input_seek(&mut self, n: u64) -> Result<Option<bool>> {
        let offset = i64::try_from(n).map_err(|_| "skip offset overflow")?;
        match self.input.seek(SeekFrom::Current(offset)) {
            Ok(pos) => {
                let meta = self.input.metadata().map_err(errno)?;
                Ok(Some(
                    !meta.is_dir() && (!meta.is_file() || pos <= meta.len()),
                ))
            }
            Err(_) => Ok(None),
        }
    }
    fn warning(&mut self, msg: &str) {
        if msg == "skip" {
            diagnostic(format_args!(
                "dd: {}: cannot skip to specified offset\n",
                quote_file(&self.src, false)
            ))
        } else if msg.ends_with('\n') {
            diagnostic(format_args!("{msg}"))
        } else {
            diagnostic(format_args!("dd: {msg}\n"))
        }
    }
    fn report(&mut self, s: &Stats) {
        self.last = *s;
        self.interrupt();
        if self.status == "progress" && self.start.elapsed() >= self.next {
            diagnostic(format_args!("\r{}", self.transfer(s)));
            self.progress_line = true;
            self.next = self.start.elapsed() + Duration::from_secs(1)
        }
    }
    fn case(&self, b: u8, upper: bool) -> u8 {
        unsafe {
            if upper {
                libc::toupper(b.into()) as u8
            } else {
                libc::tolower(b.into()) as u8
            }
        }
    }
}
fn transfer(c: &Config) -> Result<()> {
    let input = open(c.input, true, c)?;
    let output = open(c.output, false, c)?;
    let mut h = Host {
        input,
        output,
        src: name(c.input, "standard input"),
        dst: name(c.output, "standard output"),
        sparse: c.has("sparse"),
        hole: false,
        written: 0,
        start: Instant::now(),
        next: Duration::from_secs(1),
        status: c.status,
        progress_line: false,
        last: Stats::default(),
    };
    let mut s = Stats::default();
    let result = (|| {
        if c.seek > 0 {
            if c.output.is_some()
                && c.seek >= c.obs as u64
                && !c.has("notrunc")
                && h.output.metadata().map_err(errno)?.is_file()
            {
                h.output.set_len(c.seek).map_err(errno)?
            }
            h.output
                .seek(SeekFrom::Current(c.seek as i64))
                .map_err(|e| format!("{}: cannot seek: {}", quote_file(&h.dst, false), errno(e)))?;
        }
        engine::copy(c, &mut h, &mut s)
    })();
    let finished = h.finish(c);
    let mut failed = false;
    for err in [result, finished].into_iter().filter_map(|r| r.err()) {
        diagnostic(format_args!("dd: {err}\n"));
        failed = true
    }
    h.summary(&s);
    let fd = h.output.into_raw_fd();
    if unsafe { libc::close(fd) } < 0 {
        return Err(errno(io::Error::last_os_error()));
    }
    if failed { Err(String::new()) } else { Ok(()) }
}
pub fn run() -> i32 {
    let status = run_command();
    status.max(i32::from(STDERR_FAILED.load(Ordering::Relaxed)))
}

fn run_command() -> i32 {
    unsafe {
        libc::setlocale(libc::LC_CTYPE, c"".as_ptr());
        let mut action: libc::sigaction = core::mem::zeroed();
        action.sa_sigaction = info as *const () as usize;
        libc::sigemptyset(&mut action.sa_mask);
        libc::sigaction(libc::SIGINFO, &action, core::ptr::null_mut());
        let mut previous: libc::sigaction = core::mem::zeroed();
        libc::sigaction(libc::SIGINT, core::ptr::null(), &mut previous);
        if previous.sa_sigaction != libc::SIG_IGN {
            libc::sigaction(libc::SIGINT, &action, core::ptr::null_mut());
        }
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
    let mut args = env::args_os();
    let program = args.next().unwrap_or_default();
    let words: Vec<OsString> = args.collect();
    let raw: Vec<_> = words.iter().map(|s| s.as_bytes()).collect();
    match args::parse(&raw, env::var_os("POSIXLY_CORRECT").is_some()) {
        Ok(doc @ (Action::Help | Action::Version)) => {
            let locale = ["LC_ALL", "LC_MESSAGES", "LANG"]
                .iter()
                .filter_map(|k| env::var(k).ok())
                .find(|s| !s.is_empty());
            let lang = documentation::Language::from_tag(locale.as_deref());
            let page = documentation::lookup("dd").unwrap();
            let text = if matches!(doc, Action::Help) {
                page.render(&[], false, lang).unwrap()
            } else {
                page.version(lang)
            };
            i32::from(io::stdout().lock().write_all(text.as_bytes()).is_err())
        }
        Ok(Action::Run(c)) => {
            for _ in 0..c.warnings {
                diagnostic(format_args!(
                    "dd: warning: '0x' is a zero multiplier; use '00x' if that is intended\n"
                ))
            }
            match transfer(&c) {
                Ok(()) => 0,
                Err(e) => {
                    if !e.is_empty() {
                        diagnostic(format_args!("dd: {e}\n"))
                    }
                    1
                }
            }
        }
        Err(args::Error(e, usage)) => {
            diagnostic(format_args!("dd: {e}\n"));
            if usage {
                diagnostic(format_args!(
                    "Try '{} --help' for more information.\n",
                    program.to_string_lossy()
                ))
            }
            1
        }
    }
}
