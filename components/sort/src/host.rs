use crate::host_text::{argument, collation_key, columns, diagnostic, native};
use nova_sort::{
    args::Options,
    engine::{Order, merge},
};
use rayon::prelude::*;
use std::{
    cmp::Ordering,
    env,
    ffi::{CString, OsString},
    fs::{self, File},
    io::{self, BufRead, BufReader, BufWriter, Read, Write},
    os::unix::{ffi::OsStringExt, fs::MetadataExt},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
};
use tempfile::{NamedTempFile, TempPath};
type Res<T> = Result<T, Box<dyn std::error::Error>>;
const IO: usize = 128 * 1024;

fn cstr(s: &[u8]) -> CString {
    CString::new(s.split(|&b| b == 0).next().unwrap_or_default()).unwrap()
}
fn collate(a: &[u8], b: &[u8]) -> Ordering {
    let mut a = a.split(|&b| b == 0);
    let mut b = b.split(|&b| b == 0);
    loop {
        match (a.next(), b.next()) {
            (Some(x), Some(y)) => {
                let c = unsafe { libc::strcoll(cstr(x).as_ptr(), cstr(y).as_ptr()) };
                if c != 0 {
                    return c.cmp(&0);
                }
            }
            (x, y) => return x.is_some().cmp(&y.is_some()),
        }
    }
}
fn general(s: &[u8]) -> Option<f64> {
    let text = cstr(s);
    let mut end = core::ptr::null_mut();
    let v = unsafe { libc::strtod(text.as_ptr(), &mut end) };
    (!core::ptr::eq(text.as_ptr(), end)).then_some(v)
}
fn input(path: &Path) -> io::Result<Box<dyn Read>> {
    if path == Path::new("-") {
        Ok(Box::new(io::stdin()))
    } else {
        Ok(Box::new(File::open(path).map_err(|e| {
            io::Error::new(
                e.kind(),
                format!(
                    "cannot read: {}: {}",
                    diagnostic(path.as_os_str()),
                    os_error(&e)
                ),
            )
        })?))
    }
}
fn os_error(e: &io::Error) -> String {
    e.to_string()
        .split(" (os error")
        .next()
        .unwrap_or("I/O error")
        .to_string()
}
fn record(r: &mut dyn BufRead, buf: &mut Vec<u8>, delim: u8) -> io::Result<bool> {
    buf.clear();
    let n = r.read_until(delim, buf)?;
    if buf.last() == Some(&delim) {
        buf.pop();
    }
    Ok(n != 0)
}
enum Source {
    Input(PathBuf),
    Run(TempPath, bool),
}
struct Reader {
    stream: BufReader<Box<dyn Read>>,
    child: Option<(Child, String)>,
}
impl Reader {
    fn new(src: &Source, prog: Option<&str>) -> Res<Self> {
        let mut child = None;
        let stream = match src {
            Source::Input(p) => input(p)?,
            Source::Run(p, compressed) => {
                let f = File::open(p)?;
                if let Some(prog) = prog.filter(|_| *compressed) {
                    let mut c = Command::new(native(prog))
                        .arg("-d")
                        .stdin(f)
                        .stdout(Stdio::piped())
                        .spawn()?;
                    let stdout = c.stdout.take().unwrap();
                    child = Some((c, prog.to_owned()));
                    Box::new(stdout) as Box<dyn Read>
                } else {
                    Box::new(f)
                }
            }
        };
        Ok(Self {
            stream: BufReader::with_capacity(IO, stream),
            child,
        })
    }
    fn next(&mut self, buf: &mut Vec<u8>, d: u8) -> Res<bool> {
        let ok = record(&mut self.stream, buf, d)?;
        if !ok {
            if let Some((mut child, program)) = self.child.take() {
                if !child.wait()?.success() {
                    return Err(format!("'{program}' [-d] terminated abnormally").into());
                }
            }
        }
        Ok(ok)
    }
}
impl Drop for Reader {
    fn drop(&mut self) {
        if let Some((c, _)) = &mut self.child {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}

struct Temp<'a> {
    options: &'a Options,
    name: &'a str,
    codec_unavailable: bool,
    count: usize,
    bytes: u64,
    passes: usize,
}
impl Temp<'_> {
    fn write(&mut self, write: impl FnOnce(&mut dyn Write) -> Res<()>) -> Res<Source> {
        let opt = self.options;
        let file = if opt.temps.is_empty() {
            NamedTempFile::new()?
        } else {
            let dir = &opt.temps[self.count % opt.temps.len()];
            fs::metadata(native(dir)).map_err(|e| {
                format!("cannot create temporary file in '{dir}': {}", os_error(&e))
            })?;
            NamedTempFile::new_in(native(dir))
                .map_err(|e| format!("cannot create temporary file in '{dir}': {}", os_error(&e)))?
        };
        let mut codec = None;
        if let Some(prog) = opt.compressor.as_ref().filter(|_| !self.codec_unavailable) {
            match Command::new(native(prog))
                .stdin(Stdio::piped())
                .stdout(file.reopen()?)
                .spawn()
            {
                Ok(c) => codec = Some((c, prog)),
                Err(e)
                    if matches!(
                        e.kind(),
                        io::ErrorKind::NotFound | io::ErrorKind::PermissionDenied
                    ) =>
                {
                    eprintln!(
                        "{}: could not run compress program '{prog}': {}",
                        self.name,
                        os_error(&e)
                    );
                    self.codec_unavailable = true;
                }
                Err(e) => return Err(e.into()),
            }
        }
        let compressed = codec.is_some();
        if let Some((mut child, prog)) = codec {
            let mut w = BufWriter::with_capacity(IO, child.stdin.take().unwrap());
            let res = write(&mut w).and_then(|_| Ok(w.flush()?));
            drop(w);
            let status = child.wait()?;
            if !status.success() {
                return Err(format!("'{prog}' [-d] terminated abnormally").into());
            }
            res?
        } else {
            let mut w = BufWriter::with_capacity(IO, file.as_file());
            write(&mut w)?;
            w.flush()?
        }
        self.count += 1;
        self.bytes += file.as_file().metadata()?.len();
        Ok(Source::Run(file.into_temp_path(), compressed))
    }
}
fn merge_to(group: &[Source], order: &Order<'_>, emit: impl FnMut(&[u8]) -> Res<()>) -> Res<()> {
    let mut readers = group
        .iter()
        .map(|s| Reader::new(s, order.options.compressor.as_deref()))
        .collect::<Res<Vec<_>>>()?;
    merge(
        readers.len(),
        order,
        |i, b| readers[i].next(b, order.options.delim()),
        emit,
    )
}
fn raw(w: &mut dyn Write, row: &[u8], d: u8) -> Res<()> {
    w.write_all(row)?;
    w.write_all(&[d])?;
    Ok(())
}

fn sort_batch(rows: &mut [Vec<u8>], order: &Order<'_>, pool: &Option<rayon::ThreadPool>) {
    if let Some(pool) = pool {
        pool.install(|| rows.par_sort_by(|a, b| order.compare(a, b)))
    } else {
        order.sort(rows)
    }
}
fn output(o: &Options) -> Res<Box<dyn Write>> {
    Ok(if let Some(path) = &o.output {
        Box::new(BufWriter::with_capacity(
            IO,
            File::create(native(path))
                .map_err(|e| format!("open failed: {path}: {}", os_error(&e)))?,
        ))
    } else {
        Box::new(BufWriter::with_capacity(IO, io::stdout()))
    })
}
fn same(path: &Path, target: &str) -> bool {
    let Ok(target) = fs::metadata(native(target)) else {
        return false;
    };
    if path == Path::new("-") {
        let mut info = core::mem::MaybeUninit::<libc::stat>::uninit();
        // fstat identifies the actual redirected descriptor on Darwin.
        if unsafe { libc::fstat(0, info.as_mut_ptr()) } != 0 {
            return false;
        }
        let info = unsafe { info.assume_init() };
        return info.st_dev as u64 == target.dev() && info.st_ino == target.ino();
    }
    fs::metadata(path).is_ok_and(|a| a.dev() == target.dev() && a.ino() == target.ino())
}
fn check(path: &Path, order: &Order<'_>, quiet: bool, name: &str) -> Res<i32> {
    let mut r = BufReader::new(
        input(path).map_err(|e| e.to_string().replacen("cannot read:", "open failed:", 1))?,
    );
    let mut prev = Vec::new();
    let mut row = Vec::new();
    let mut n = 0;
    while record(&mut r, &mut row, order.options.delim())? {
        n += 1;
        let c = order.compare(&prev, &row);
        if n > 1 && (c.is_gt() || c.is_eq() && order.options.unique) {
            if !quiet {
                let mut err = io::stderr().lock();
                write!(err, "{name}: {}:{n}: disorder: ", path.display())?;
                err.write_all(&row)?;
                err.write_all(b"\n")?
            }
            return Ok(1);
        }
        std::mem::swap(&mut row, &mut prev)
    }
    Ok(0)
}
pub fn command(name: &str) -> Res<i32> {
    let args = env::args_os().skip(1).map(argument).collect::<Vec<_>>();
    let legacy = env::var("_POSIX2_VERSION")
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .is_some_and(|v| !(200112..200809).contains(&v));
    let mut o =
        Options::parse_legacy(&args.iter().map(String::as_str).collect::<Vec<_>>(), legacy)?;
    let tag = env::var("LC_ALL")
        .or_else(|_| env::var("LC_MESSAGES"))
        .or_else(|_| env::var("LANG"))
        .unwrap_or_default();
    if o.help || o.version {
        let lang = tag.split('.').next().unwrap_or("").replace('_', "-");
        print!(
            "{}",
            if o.help {
                crate::locale::Language::from_tag(Some(&lang)).select(crate::EN, crate::FR)
            } else {
                crate::VERSION
            }
        );
        return Ok(0);
    }
    // Set the process locale before creating workers.
    let locale_ok = unsafe { !libc::setlocale(libc::LC_ALL, c"".as_ptr()).is_null() };
    let locale = unsafe { libc::setlocale(libc::LC_COLLATE, std::ptr::null()) };
    let hard = !locale.is_null()
        && !matches!(
            unsafe { std::ffi::CStr::from_ptr(locale) }.to_bytes(),
            b"C" | b"POSIX"
        );
    // Copy C-library locale data before starting the worker pool.
    unsafe {
        let info = libc::localeconv();
        if !info.is_null() {
            let single = |p: *const libc::c_char| {
                std::ffi::CStr::from_ptr(p)
                    .to_bytes()
                    .first()
                    .copied()
                    .filter(|_| std::ffi::CStr::from_ptr(p).to_bytes().len() == 1)
            };
            o.decimal = single((*info).decimal_point);
            o.thousands = single((*info).thousands_sep);
        }
        for n in 0..12 {
            let p = libc::nl_langinfo(libc::ABMON_1 + n);
            o.months
                .push(std::ffi::CStr::from_ptr(p).to_bytes().to_vec());
        }
    }
    let mut paths = Vec::new();
    if let Some(list) = &o.list {
        let mut r = BufReader::new(
            input(Path::new(&native(list)))
                .map_err(|e| e.to_string().replacen("cannot read:", "open failed:", 1))?,
        );
        let mut buf = Vec::new();
        while record(&mut r, &mut buf, 0)? {
            if buf.is_empty() {
                return Err(
                    format!("{list}:{}: invalid zero-length file name", paths.len() + 1).into(),
                );
            }
            if list == "-" && buf == b"-" {
                return Err(
                    "when reading file names from standard input, no file name of '-' allowed"
                        .into(),
                );
            }
            paths.push(PathBuf::from(OsString::from_vec(buf.clone())))
        }
        if paths.is_empty() {
            return Err(format!("no input from '{list}'").into());
        }
    } else {
        paths.extend(o.files.iter().map(|s| PathBuf::from(native(s))));
        if paths.is_empty() {
            paths.push("-".into())
        }
    }
    if o.has_random() {
        let mut seed = [0u8; 16];
        let source = o.source.as_deref().unwrap_or("/dev/urandom");
        File::open(native(source))
            .map_err(|e| format!("open failed: {source}: {}", os_error(&e)))?
            .read_exact(&mut seed)
            .map_err(|e| {
                if e.kind() == io::ErrorKind::UnexpectedEof {
                    format!("'{source}': end of file")
                } else {
                    e.to_string()
                }
            })?;
        o.seed = seed
    }
    if o.merge {
        for p in &paths {
            if p != Path::new("-") {
                drop(input(p)?)
            }
        }
    }
    let order = Order {
        options: &o,
        collate: hard.then_some(collate),
        random_key: hard.then_some(collation_key),
        general,
    };
    if let Some(q) = o.check {
        if paths.len() != 1 {
            return Err("extra operand".into());
        }
        return check(&paths[0], &order, q, name);
    }
    let fd = unsafe { libc::sysconf(libc::_SC_OPEN_MAX) };
    let limit = if fd > 4 { fd as usize - 3 } else { 16 };
    let batch = o.batch.unwrap_or(16.min(limit));
    if batch > limit {
        let raw = args
            .iter()
            .find_map(|s| s.strip_prefix("--batch-size="))
            .unwrap_or("");
        let argument = if raw.is_empty() {
            batch.to_string()
        } else {
            raw.to_string()
        };
        return Err(format!("--batch-size argument '{argument}' too large\n{name}: maximum --batch-size argument with current rlimit is {limit}").into());
    }
    // A parent may pass open descriptors through exec. Intermediate merges
    // also need a descriptor for their output. Count unused slots on small limits.
    let batch = if fd > 0 && fd <= 65536 {
        let available = (3..fd as i32)
            .filter(|&descriptor| unsafe { libc::fcntl(descriptor, libc::F_GETFD) } < 0)
            .count();
        batch.min(available.saturating_sub(1).max(2))
    } else {
        batch
    };
    let ram = physical_ram();
    let budget = o.buffer(ram, (ram / 8).clamp(1024 * 1024, 512 * 1024 * 1024))?;
    let workers = o
        .parallel
        .unwrap_or_else(|| std::thread::available_parallelism().map_or(1, |v| v.get().min(8)));
    let pool = if workers > 1 {
        Some(
            rayon::ThreadPoolBuilder::new()
                .num_threads(workers.min(256))
                .build()?,
        )
    } else {
        None
    };
    let d = o.delim();
    let mut runs = Vec::new();
    let mut rows = Vec::new();
    let mut temp = Temp {
        options: &o,
        name,
        codec_unavailable: false,
        count: 0,
        bytes: 0,
        passes: 0,
    };
    if o.merge {
        for path in paths {
            if o.output.as_ref().is_some_and(|out| same(&path, out)) {
                runs.push(temp.write(|w| {
                    io::copy(&mut input(&path)?, w)?;
                    Ok(())
                })?)
            } else {
                runs.push(Source::Input(path))
            }
        }
    } else {
        let mut used = 0usize;
        for p in paths {
            let mut r = BufReader::with_capacity(IO, input(&p)?);
            loop {
                let mut row = Vec::new();
                if !record(&mut r, &mut row, d)
                    .map_err(|e| format!("read failed: {}: {}", p.display(), os_error(&e)))?
                {
                    break;
                }
                used = used.saturating_add(row.capacity() + 2 * size_of::<Vec<u8>>());
                rows.push(row);
                if used >= budget {
                    sort_batch(&mut rows, &order, &pool);
                    runs.push(temp.write(|w| {
                        for row in rows.drain(..) {
                            raw(w, &row, d)?
                        }
                        Ok(())
                    })?);
                    used = 0
                }
            }
        }
        sort_batch(&mut rows, &order, &pool);
        if !runs.is_empty() && !rows.is_empty() {
            runs.push(temp.write(|w| {
                for row in rows.drain(..) {
                    raw(w, &row, d)?
                }
                Ok(())
            })?)
        }
    }
    while runs.len() > batch {
        let mut next = Vec::new();
        let mut iter = runs.into_iter();
        loop {
            let mut group: Vec<_> = iter.by_ref().take(batch).collect();
            if group.is_empty() {
                break;
            }
            if group.len() == 1 {
                next.push(group.pop().unwrap())
            } else {
                next.push(temp.write(|w| merge_to(&group, &order, |row| raw(w, row, d)))?)
            }
        }
        runs = next;
        temp.passes += 1;
    }
    let mut w = output(&o)?;
    let mut prev: Option<Vec<u8>> = None;
    if o.debug {
        if !locale_ok {
            eprintln!("{name}: failed to set locale");
        }
        if hard {
            let rules = unsafe { std::ffi::CStr::from_ptr(locale) }.to_string_lossy();
            eprintln!("{name}: text ordering performed using ‘{rules}’ sorting rules");
        } else {
            eprintln!("{name}: text ordering performed using simple byte comparison");
        }
        for warning in nova_sort::engine::debug::warnings(&o) {
            eprintln!("{name}: {warning}");
        }
    }
    let mut emit = |row: &[u8]| -> Res<()> {
        if o.unique {
            if prev.as_ref().is_some_and(|p| order.compare(p, row).is_eq()) {
                return Ok(());
            }
            let saved = prev.get_or_insert_with(Vec::new);
            saved.clear();
            saved.extend_from_slice(row);
        }
        if o.debug {
            let line: Vec<_> = row.iter().map(|&b| if b == 9 { b'>' } else { b }).collect();
            raw(&mut w, &line, 10)?;
            for i in 0..o.keys.len().max(1) {
                let (a, b) = order.annotation(row, i);
                w.write_all(&vec![b' '; columns(&row[..a], hard)])?;
                if a == b {
                    w.write_all(b"^ no match for key")?
                } else {
                    w.write_all(&vec![b'_'; columns(&row[a..b], hard)])?
                }
                w.write_all(b"\n")?
            }
            if (!o.keys.is_empty() || o.flags.has_transform()) && !o.stable && !o.unique {
                raw(&mut w, &vec![b'_'; columns(row, hard)], 10)?
            }
            Ok(())
        } else {
            raw(&mut w, row, d)
        }
    };
    if runs.is_empty() {
        for row in &rows {
            emit(row)?
        }
    } else {
        merge_to(&runs, &order, emit)?;
        temp.passes += 1
    }
    w.flush()?;
    if let Some(path) = env::var_os("NOVA_SORT_METRICS") {
        fs::write(
            path,
            format!(
                "{{\"runs\":{},\"passes\":{},\"temp_bytes_written\":{},\"workers\":{}}}\n",
                temp.count, temp.passes, temp.bytes, workers
            ),
        )?
    }
    Ok(0)
}
fn physical_ram() -> usize {
    let mut ram = 0u64;
    let mut n = size_of::<u64>();
    let res = unsafe {
        libc::sysctlbyname(
            c"hw.memsize".as_ptr(),
            (&raw mut ram).cast(),
            &mut n,
            std::ptr::null_mut(),
            0,
        )
    };
    if res == 0 { ram as usize } else { 256 << 20 }
}
