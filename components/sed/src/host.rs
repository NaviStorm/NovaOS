use crate::{
    engine::{Program, Streams},
    locale::Language,
    options::Options,
};
use std::{
    collections::{BTreeMap, VecDeque},
    ffi::OsStr,
    fs::{self, File},
    io::{self, BufReader, BufWriter, Read, Write},
    os::{
        fd::AsRawFd,
        unix::{ffi::OsStrExt, fs::MetadataExt},
    },
    path::{Path, PathBuf},
    process::Command,
};

fn path(bytes: &[u8]) -> &Path {
    Path::new(OsStr::from_bytes(bytes))
}
fn fail(error: impl std::fmt::Display) -> String {
    error.to_string()
}
enum Named {
    Input(BufReader<File>),
    Output(BufWriter<File>),
}
struct Host<'a> {
    pending: VecDeque<Vec<u8>>,
    input: Option<Box<dyn Read>>,
    name: Vec<u8>,
    output: &'a mut dyn Write,
    persistent: BTreeMap<Vec<u8>, usize>,
    named: BTreeMap<usize, Named>,
    next: usize,
    missing: bool,
    unbuffered: bool,
    last_byte: Option<u8>,
    separator: u8,
    boundary: bool,
}
impl<'a> Host<'a> {
    fn new(files: &[Vec<u8>], output: &'a mut dyn Write, opts: &Options) -> Self {
        let pending = if files.is_empty() {
            VecDeque::from([b"-".to_vec()])
        } else {
            files.iter().cloned().collect()
        };
        Self {
            pending,
            input: None,
            name: vec![],
            output,
            persistent: BTreeMap::new(),
            named: BTreeMap::new(),
            next: 1,
            missing: false,
            unbuffered: opts.unbuffered,
            last_byte: None,
            separator: if opts.zero { 0 } else { b'\n' },
            boundary: false,
        }
    }
}
impl Host<'_> {
    fn finish(&mut self) -> Result<(), String> {
        self.output.flush().map_err(fail)?;
        for stream in self.named.values_mut() {
            if let Named::Output(output) = stream {
                output.flush().map_err(fail)?;
            }
        }
        Ok(())
    }
}
impl Streams for Host<'_> {
    fn open(&mut self, name: &[u8], out: bool, persistent: bool) -> Result<usize, String> {
        if persistent && let Some(&id) = self.persistent.get(name) {
            return Ok(id);
        }
        let p = path(name);
        let stream = if out {
            Named::Output(BufWriter::new(File::create(p).map_err(fail)?))
        } else {
            Named::Input(BufReader::new(File::open(p).map_err(fail)?))
        };
        let id = self.next;
        self.next += 1;
        self.named.insert(id, stream);
        if persistent {
            self.persistent.insert(name.to_vec(), id);
        }
        Ok(id)
    }
    fn close(&mut self, id: usize) -> Result<(), String> {
        if self.persistent.values().any(|&h| h == id) {
            return Ok(());
        }
        if let Some(Named::Output(mut f)) = self.named.remove(&id) {
            f.flush().map_err(fail)?;
        }
        Ok(())
    }
    fn read(&mut self, id: usize, buf: &mut [u8]) -> Result<usize, String> {
        if id != 0 {
            return match self.named.get_mut(&id) {
                Some(Named::Input(r)) => r.read(buf).map_err(fail),
                _ => Err("invalid input".into()),
            };
        }
        let max = if self.unbuffered { 1 } else { buf.len() };
        loop {
            if let Some(r) = self.input.as_mut() {
                let n = r.read(&mut buf[..max]).map_err(fail)?;
                if n > 0 {
                    self.last_byte = Some(buf[n - 1]);
                    return Ok(n);
                }
                self.input = None;
                self.boundary = self.last_byte.is_some_and(|b| b != self.separator);
                self.last_byte = None;
            }
            let Some(name) = self.pending.pop_front() else {
                return Ok(0);
            };
            let p = path(&name);
            let input: Box<dyn Read> = if name == b"-" {
                Box::new(io::stdin())
            } else {
                match File::open(p) {
                    Ok(f) => Box::new(f),
                    Err(e) => {
                        eprintln!("sed: {}: {e}", p.display());
                        self.missing = true;
                        continue;
                    }
                }
            };
            self.input = Some(input);
            self.name = name;
            if self.boundary {
                self.boundary = false;
                buf[0] = self.separator;
                return Ok(1);
            }
        }
    }
    fn write(&mut self, id: usize, data: &[u8]) -> Result<(), String> {
        let out: &mut dyn Write = if id == 0 {
            self.output
        } else {
            match self.named.get_mut(&id) {
                Some(Named::Output(w)) => w,
                _ => return Err("invalid output".into()),
            }
        };
        out.write_all(data).map_err(fail)?;
        if self.unbuffered {
            out.flush().map_err(fail)?;
        }
        Ok(())
    }
    fn shell(&mut self, command: &[u8]) -> Result<Vec<u8>, String> {
        Command::new("/bin/sh")
            .arg("-c")
            .arg(OsStr::from_bytes(command))
            .stderr(std::process::Stdio::inherit())
            .output()
            .map(|out| out.stdout)
            .map_err(fail)
    }
    fn filename(&self) -> &[u8] {
        if self.name == b"-" { b"-" } else { &self.name }
    }
}

fn render(
    program: &mut Program,
    opts: &Options,
    files: &[Vec<u8>],
    out: &mut dyn Write,
) -> Result<(Option<i32>, bool), String> {
    let mut host = Host::new(files, out, opts);
    let status = program.run(&mut host)?;
    host.finish()?;
    Ok((status, host.missing))
}

fn backup_name(source: &Path, suffix: &[u8]) -> PathBuf {
    let name = source.as_os_str().as_bytes();
    let mut out = Vec::new();
    if suffix.contains(&b'*') {
        for &c in suffix {
            if c == b'*' {
                out.extend_from_slice(name)
            } else {
                out.push(c)
            }
        }
    } else {
        out.extend_from_slice(name);
        out.extend_from_slice(suffix);
    }
    path(&out).to_owned()
}

fn edit(
    program: &mut Program,
    opts: &Options,
    file: &[u8],
    suffix: &[u8],
) -> Result<(Option<i32>, bool), String> {
    let dest = if opts.follow {
        fs::canonicalize(path(file)).map_err(fail)?
    } else {
        path(file).to_owned()
    };
    if !fs::metadata(&dest).map_err(fail)?.is_file() {
        return Err("in-place editing requires a regular file".into());
    }
    // Cette ouverture vérifie les droits sur l'original et fixe l'objet à lire.
    let original = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&dest)
        .map_err(fail)?;
    let metadata = original.metadata().map_err(fail)?;
    let directory_entry = fs::symlink_metadata(&dest).map_err(fail)?;
    let parent = dest
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut candidate = tempfile::NamedTempFile::new_in(parent).map_err(fail)?;
    let status = {
        let mut writer = BufWriter::new(candidate.as_file_mut());
        let mut io = Host::new(&[], &mut writer, opts);
        io.pending.clear();
        io.name = file.to_vec();
        io.input = Some(Box::new(original.try_clone().map_err(fail)?));
        let status = program.run(&mut io)?;
        io.finish()?;
        status
    };
    let result = (status, false);
    #[cfg(all(feature = "fault-tests", target_os = "macos"))]
    if std::env::var_os("NOVA_SED_TEST_EIO").is_some() {
        return Err(fail(io::Error::from_raw_os_error(5)));
    }
    let new_metadata = candidate.as_file().metadata().map_err(fail)?;
    if (metadata.uid(), metadata.gid()) != (new_metadata.uid(), new_metadata.gid()) {
        unsafe extern "C" {
            fn fchown(fd: i32, uid: u32, gid: u32) -> i32;
        }
        if unsafe {
            fchown(
                candidate.as_file().as_raw_fd(),
                metadata.uid(),
                metadata.gid(),
            )
        } != 0
        {
            return Err(fail(io::Error::last_os_error()));
        }
    }
    candidate
        .as_file()
        .set_permissions(metadata.permissions())
        .map_err(fail)?;
    #[cfg(target_os = "macos")]
    {
        unsafe extern "C" {
            fn fcopyfile(src: i32, dst: i32, state: *mut core::ffi::c_void, flags: u32) -> i32;
        }
        // copyfile.h : COPYFILE_ACL | COPYFILE_XATTR, sans recopier les données.
        if unsafe {
            fcopyfile(
                original.as_raw_fd(),
                candidate.as_file().as_raw_fd(),
                core::ptr::null_mut(),
                1 | 4,
            )
        } != 0
        {
            return Err(fail(io::Error::last_os_error()));
        }
    }
    let current = fs::symlink_metadata(&dest).map_err(fail)?;
    let content = original.metadata().map_err(fail)?;
    if (directory_entry.dev(), directory_entry.ino()) != (current.dev(), current.ino())
        || (metadata.ctime(), metadata.ctime_nsec()) != (content.ctime(), content.ctime_nsec())
    {
        return Err("input changed during in-place editing".into());
    }
    candidate.as_file().sync_all().map_err(fail)?;
    if !suffix.is_empty() {
        let backup = backup_name(&dest, suffix);
        if backup != dest {
            let dir = backup
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or(Path::new("."));
            let staging = tempfile::tempdir_in(dir).map_err(fail)?;
            let link = staging.path().join("original");
            fs::hard_link(&dest, &link).map_err(fail)?;
            fs::rename(link, backup).map_err(fail)?;
        }
    }
    #[cfg(all(feature = "fault-tests", target_os = "macos"))]
    if std::env::var_os("NOVA_SED_TEST_PAUSE_PUBLICATION").is_some() {
        unsafe extern "C" {
            fn raise(signal: i32) -> i32;
        }
        unsafe {
            raise(17);
        } // SIGSTOP sur Darwin, après la sauvegarde et avant le renommage.
    }
    candidate.persist(&dest).map_err(fail)?;
    Ok(result)
}

fn run(opts: Options) -> Result<i32, (i32, String)> {
    if opts.help || opts.version {
        let tag = std::env::var("LANG").ok();
        let text = if opts.help {
            Language::from_tag(tag.as_deref()).select(crate::HELP_EN, crate::HELP_FR)
        } else {
            crate::VERSION
        };
        io::stdout()
            .lock()
            .write_all(text.as_bytes())
            .map_err(|e| (4, fail(e)))?;
        return Ok(0);
    }
    if opts.debug {
        return Err((1, "--debug: unsupported".into()));
    }
    let mut script = Vec::new();
    for (from_file, s) in &opts.scripts {
        if !script.is_empty() && !script.ends_with(b"\n") {
            script.push(b'\n');
        }
        if *from_file {
            if s == b"-" {
                io::stdin()
                    .read_to_end(&mut script)
                    .map_err(|e| (4, fail(e)))?;
            } else {
                script.extend(fs::read(path(s)).map_err(|e| (4, fail(e)))?);
            }
        } else {
            script.extend(s);
        }
    }
    let mut program = Program::compile(&script, opts.flags(&script), opts.width.unwrap_or(70))
        .map_err(|e| (1, e))?;
    let mut out = BufWriter::new(io::stdout().lock());
    let mut missing = false;
    if opts.inplace.is_some() && opts.files.is_empty() {
        return Err((4, "no input files".into()));
    }
    if opts.separate || opts.inplace.is_some() {
        let default = [b"-".to_vec()];
        for file in if opts.files.is_empty() {
            &default[..]
        } else {
            &opts.files
        } {
            let result = if let Some(suffix) = &opts.inplace {
                if fs::metadata(path(file)).is_err() {
                    eprintln!("sed: cannot read {}", path(file).display());
                    missing = true;
                    continue;
                }
                edit(&mut program, &opts, file, suffix)
            } else {
                render(&mut program, &opts, std::slice::from_ref(file), &mut out)
            };
            let (status, failed) = result.map_err(|e| (4, e))?;
            missing |= failed;
            if let Some(n) = status {
                return Ok(if missing { 2 } else { n });
            }
        }
    } else {
        let (status, failed) =
            render(&mut program, &opts, &opts.files, &mut out).map_err(|e| (4, e))?;
        if failed {
            return Ok(2);
        }
        if let Some(n) = status {
            return Ok(n);
        }
    }
    Ok(if missing { 2 } else { 0 })
}
pub fn main() -> i32 {
    let args = std::env::args_os()
        .skip(1)
        .map(|s| s.as_bytes().to_vec())
        .collect::<Vec<_>>();
    match Options::parse(&args).map_err(|e| (1, e)).and_then(run) {
        Ok(n) => n,
        Err((code, e)) => {
            eprintln!("sed: {e}");
            code
        }
    }
}
