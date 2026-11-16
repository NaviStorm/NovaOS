#![no_std]
#![no_main]
extern crate alloc;
use alloc::{string::String, vec::Vec};
use nova_cli::{console, fileapi::Client, process, protocol as w, runtime as rt, tty};
use nova_sha256sum::{Platform, Source};

enum Input {
    Pipe(u64),
    File {
        handle: u64,
        offset: u64,
        size: u64,
        times: [u8; 24],
    },
}
struct Native<'a> {
    files: Option<Client>,
    grants: &'a str,
    pipe: u64,
}
impl Source for Native<'_> {
    type Stream = Input;
    type Error = u64;
    const BUFFER: usize = 4096;
    fn open(&mut self, name: &[u8]) -> rt::Result<Input> {
        if name == b"-" {
            return if self.pipe != 0 {
                Ok(Input::Pipe(self.pipe))
            } else {
                Err(w::UNSUPPORTED)
            };
        }
        let path = core::str::from_utf8(name).map_err(|_| rt::BAD_ARGUMENT)?;
        // Table privée préparée par nsh. Aucun chemin du manifeste ne traverse l'IPC.
        let value = self
            .grants
            .split('\0')
            .filter_map(|s| s.split_once(':'))
            .find(|(_, n)| *n == path)
            .map(|(v, _)| v)
            .ok_or(rt::DENIED)?;
        if let Some(code) = value.strip_prefix('!') {
            return Err(code.parse().unwrap_or(rt::BAD_ARGUMENT));
        }
        let handle = value.parse().map_err(|_| rt::BAD_ARGUMENT)?;
        let stat = self
            .files
            .as_mut()
            .ok_or(rt::DENIED)?
            .simple(w::STAT, handle)?;
        Ok(Input::File {
            handle,
            offset: 0,
            size: w::word(&stat, w::PAYLOAD + 16),
            times: stat[w::PAYLOAD + 37..w::PAYLOAD + 61].try_into().unwrap(),
        })
    }
    fn read(&mut self, stream: &mut Input, buf: &mut [u8]) -> rt::Result<usize> {
        match stream {
            Input::Pipe(key) => process::pipe_read(*key, buf),
            Input::File {
                handle,
                offset,
                size,
                times,
            } => {
                let fs = self.files.as_mut().ok_or(rt::DENIED)?;
                let n = (*size - *offset).min(buf.len() as u64) as usize;
                if n == 0 {
                    let stat = fs.simple(w::STAT, *handle)?;
                    if w::word(&stat, w::PAYLOAD + 16) != *size
                        || stat[w::PAYLOAD + 37..w::PAYLOAD + 61] != *times
                    {
                        return Err(rt::REVOKED);
                    }
                    return Ok(0);
                }
                let count = fs.read(*handle, *offset, &mut buf[..n])?;
                if count != n {
                    return Err(rt::REVOKED);
                }
                if *offset == 0
                    && option_env!("NOVA_DOC_TRACE") == Some("1")
                    && self.grants.starts_with("\0retrait\0")
                {
                    console::write(tty::ERROR, &alloc::format!("[sha256sum lecture={count}]\n"))?;
                    let ack = rt::receive_for(rt::config(0), 15_000)?;
                    if (ack.kind, ack.tag, ack.len, ack.payload[0])
                        != (1, tty::PIPE_PROBE_WAIT, 1, b'+')
                    {
                        return Err(rt::BAD_ARGUMENT);
                    }
                }
                *offset += count as u64;
                Ok(count)
            }
        }
    }
}
impl Platform for Native<'_> {
    fn missing(e: &u64) -> bool {
        *e == w::NOT_FOUND
    }
    fn reason(e: &u64) -> String {
        alloc::format!("{} ({e})", console::reason(*e))
    }
    fn write(&mut self, err: bool, bytes: &[u8]) -> rt::Result<()> {
        if bytes.contains(&0) {
            return Err(w::UNSUPPORTED);
        }
        let text = core::str::from_utf8(bytes).map_err(|_| rt::BAD_ARGUMENT)?;
        console::write(if err { tty::ERROR } else { tty::OUTPUT }, text)
    }
}

fn execute() -> rt::Result<bool> {
    let args = process::arguments()?;
    let fr =
        nova_cli::documentation::child_language(&args) == nova_cli::documentation::Language::French;
    let prep = rt::config(3);
    if prep != 0 {
        return Err(prep);
    }
    let (grants, args) = match args.split_first() {
        Some((map, rest)) if map.starts_with('\0') => (*map, rest),
        _ => ("", args.as_slice()),
    };
    let pipe = rt::config(2);
    let files = if grants
        .split('\0')
        .any(|line| line.as_bytes().first().is_some_and(u8::is_ascii_digit))
    {
        Some(Client::scoped()?)
    } else {
        None
    };
    if pipe != 0 && (files.is_some() || rt::channels().is_ok()) {
        return Err(rt::DENIED);
    }
    if option_env!("NOVA_DOC_TRACE") == Some("1") {
        console::write(
            tty::ERROR,
            &alloc::format!(
                "[sha256sum fichiers={} pipe={}]\n",
                u8::from(rt::channels().is_ok()),
                u8::from(pipe != 0)
            ),
        )?;
    }
    let mut io = Native {
        files,
        grants,
        pipe,
    };
    let argv: Vec<_> = args.iter().map(|s| s.as_bytes()).collect();
    nova_sha256sum::run(&mut io, &argv, fr)
}
#[unsafe(no_mangle)]
extern "C" fn run() -> ! {
    nova_cli::heap_profile::start();
    let result = execute();
    if option_env!("NOVA_SHA256_MEMORY") == Some("1")
        && let Some(bytes) = nova_cli::heap_profile::bytes()
    {
        let _ = console::write(
            tty::ERROR,
            &alloc::format!("[sha256sum pic_tas_octets={bytes}]\n"),
        );
    }
    match result {
        Ok(ok) => rt::exit(u64::from(!ok)),
        Err(code) => {
            let _ = console::write(
                tty::ERROR,
                &alloc::format!("sha256sum : {} ({code})\n", console::reason(code)),
            );
            rt::exit(code)
        }
    }
}
