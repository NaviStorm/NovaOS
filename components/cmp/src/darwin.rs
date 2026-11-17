use super::*;
use std::fs::Metadata;
use std::io::{BufWriter, Stdout};
use std::os::fd::AsFd as _;
use std::{
    env,
    ffi::OsStr,
    fs::File,
    io::{self, Read, Seek, SeekFrom, Write},
    os::unix::{ffi::OsStrExt, fs::{FileTypeExt, MetadataExt}},
};

struct Host {
    out: BufWriter<Stdout>,
    null: bool,
    stdin_regular: bool,
    stdin_block: usize,
}
struct Source {
    file: File,
    meta: Metadata,
    at: Option<u64>,
    name: String,
    stdin: bool,
}
fn reason(e: io::Error) -> String {
    let text = e.to_string();
    text.split(" (os error ").next().unwrap_or(&text).into()
}
impl Input for Source {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, String> {
        loop {
            match self.file.read(buf) {
                Ok(n) => {
                    if let Some(p) = &mut self.at {
                        *p += n as u64;
                    }
                    return Ok(n);
                }
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(format!("{}: {}", self.name, reason(e))),
            }
        }
    }
    fn remaining(&self) -> Option<u64> {
        self.at
            .filter(|_| self.meta.is_file())
            .map(|at| self.meta.len().saturating_sub(at))
    }
    fn identity(&self) -> Option<(u64, u64, u64)> {
        Some((
            self.meta.dev() as u64,
            self.meta.ino(),
            self.at.unwrap_or(u64::MAX),
        ))
    }
    fn skip(&mut self, count: u64) -> Result<(), String> {
        // Deux opérandes « - » partagent la position de la description ouverte.
        if self.stdin && self.at.is_some() {
            self.at = self.file.stream_position().ok();
        }
        if count == 0 {
            return Ok(());
        }
        if let Some(at) = self.at {
            let next = at.saturating_add(count).min(i64::MAX as u64);
            if self.file.seek(SeekFrom::Start(next)).is_ok() {
                self.at = Some(next);
                return Ok(());
            }
            if self.meta.is_file() && next >= self.meta.len() {
                self.at = Some(self.meta.len());
                self.file.seek(SeekFrom::End(0)).map_err(reason)?;
                return Ok(());
            }
        }
        engine::discard(self, count).map_err(|e| {
            if count > options::LIMIT {
                format!("{}: {e}", self.name)
            } else {
                e
            }
        })
    }
}
impl Output for Host {
    fn emit(&mut self, err: bool, b: &[u8]) -> Result<(), String> {
        if err {
            io::stderr().write_all(b)
        } else {
            self.out.write_all(b)
        }
        .map_err(reason)
    }
}
impl Platform for Host {
    type Source = Source;
    fn open(&mut self, name: &[u8], _: usize) -> Result<Source, String> {
        let label = if !name.is_empty()
            && name
                .iter()
                .all(|b| b.is_ascii_alphanumeric() || b"./-_+%:,@^".contains(b))
        {
            String::from_utf8_lossy(name).into()
        } else {
            shell_name(name)
        };
        let file = if name == b"-" {
            io::stdin().as_fd().try_clone_to_owned().map(File::from)
        } else {
            File::open(OsStr::from_bytes(name))
        };
        let mut file = file.map_err(|e| format!("{label}: {}", reason(e)))?;
        let meta = file.metadata().map_err(reason)?;
        let at = file.stream_position().ok();
        if name == b"-" {
            self.stdin_regular = meta.is_file();
            self.stdin_block = (meta.blksize() as usize).max(1);
        }
        Ok(Source {
            file,
            meta,
            at,
            name: label,
            stdin: name == b"-",
        })
    }
    fn block(&self) -> usize {
        if self.stdin_regular {
            self.stdin_block
        } else {
            65536
        }
    }
    fn null_output(&self) -> bool {
        self.null
    }
}
fn shell_name(raw: &[u8]) -> String {
    if raw.iter().all(|&b| (32..127).contains(&b)) {
        let text = String::from_utf8_lossy(raw);
        if text.contains('\'') && !text.contains(['"', '$', '\\', '!', '\x60']) {
            return format!("\"{text}\"");
        }
        return format!("'{}'", text.replace('\'', "'\\''"));
    }
    let mut text = String::from("'");
    let mut escaped = false;
    for &b in raw {
        if (32..127).contains(&b) {
            if escaped {
                text.push_str("''");
                escaped = false
            }
            if b == b'\'' {
                text.push_str("'\\''")
            } else {
                text.push(b as char)
            }
        } else {
            if !escaped {
                text.push_str("'$'");
                escaped = true
            }
            match b {
                b'\n' => text.push_str("\\n"),
                b'\t' => text.push_str("\\t"),
                b'\r' => text.push_str("\\r"),
                _ => text.push_str(&format!("\\{b:03o}")),
            }
        }
    }
    text.push('\'');
    text
}
pub fn run() -> u8 {
    let args: Vec<_> = env::args_os().collect();
    let bytes: Vec<_> = args[1..].iter().map(|a| a.as_bytes()).collect();
    let fr = super::locale::Language::from_tag(env::var("LANG").ok().as_deref())
        == super::locale::Language::French;
    let null = io::stdout()
        .as_fd()
        .try_clone_to_owned()
        .ok()
        .and_then(|fd| File::from(fd).metadata().ok())
        .is_some_and(|a| {
            a.file_type().is_char_device()
                && std::fs::metadata("/dev/null")
                    .is_ok_and(|b| a.dev() == b.dev() && a.ino() == b.ino())
        });
    let mut host = Host {
        out: BufWriter::new(io::stdout()),
        null,
        stdin_regular: false,
        stdin_block: 4096,
    };
    let result = execute(
        &mut host,
        &bytes,
        args[0].as_bytes(),
        fr,
        env::var_os("POSIXLY_CORRECT").is_some(),
    );
    match result.and_then(|status| {
        host.out
            .flush()
            .map(|_| status)
            .map_err(|e| format!("standard output: {}", reason(e)))
    }) {
        Ok(s) => s,
        Err(e) => {
            let _ = writeln!(io::stderr(), "cmp: {e}");
            2
        }
    }
}
