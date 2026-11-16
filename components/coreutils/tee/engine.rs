//! Sélection des sorties et diffusion des blocs, indépendante des appels système.
use alloc::{format, string::String, vec, vec::Vec};

#[derive(Debug, Default, PartialEq, Eq, Copy, Clone)]
pub enum Mode {
    #[default]
    Default,
    Warn,
    WarnNoPipe,
    Exit,
    ExitNoPipe,
}

#[derive(Default)]
pub struct Options<'a> {
    pub append: bool,
    pub ignore_interrupts: bool,
    pub mode: Mode,
    pub files: Vec<&'a [u8]>,
}
pub enum Action<'a> {
    Copy(Options<'a>),
    Help,
    Version,
}

fn spelling(word: &[u8]) -> String {
    String::from_utf8_lossy(word).into_owned()
}

fn mode(value: &[u8]) -> Result<Mode, String> {
    let choices: &[(&[u8], Mode)] = &[
        (b"warn", Mode::Warn),
        (b"warn-nopipe", Mode::WarnNoPipe),
        (b"exit", Mode::Exit),
        (b"exit-nopipe", Mode::ExitNoPipe),
    ];
    let mut found = None;
    for &(name, mode) in choices {
        if name == value {
            return Ok(mode);
        }
        if name.starts_with(value) {
            if found.is_some() {
                return Err(format!(
                    "ambiguous argument '{}' for '--output-error'",
                    spelling(value)
                ));
            }
            found = Some(mode);
        }
    }
    found.ok_or_else(|| {
        format!(
            "invalid argument '{}' for '--output-error'",
            spelling(value)
        )
    })
}
pub fn parse<'a>(args: &[&'a [u8]], posix: bool) -> Result<Action<'a>, String> {
    let mut opts = Options::default();
    let mut literal = false;
    for &word in args {
        if literal || word == b"-" || !word.starts_with(b"-") {
            opts.files.push(word);
            literal |= posix;
        } else if word == b"--" {
            literal = true;
        } else if let Some(long) = word.strip_prefix(b"--") {
            let mut parts = long.splitn(2, |&b| b == b'=');
            let key = parts.next().unwrap();
            let value = parts.next();
            let names = [
                "append",
                "ignore-interrupts",
                "output-error",
                "help",
                "version",
            ];
            if key.is_empty() {
                return Err(format!(
                    "option '{}' is ambiguous; possibilities: '--append' '--ignore-interrupts' '--output-error' '--help' '--version'",
                    spelling(word)
                ));
            }
            let Some(name) = names.iter().find(|s| s.as_bytes().starts_with(key)) else {
                return Err(format!("unrecognized option '{}'", spelling(word)));
            };
            if value.is_some() && *name != "output-error" {
                return Err(format!("option '--{name}' doesn't allow an argument"));
            }
            match *name {
                "append" => opts.append = true,
                "ignore-interrupts" => opts.ignore_interrupts = true,
                "help" => return Ok(Action::Help),
                "version" => return Ok(Action::Version),
                _ => opts.mode = value.map_or(Ok(Mode::WarnNoPipe), mode)?,
            }
        } else {
            for &c in &word[1..] {
                match c {
                    b'a' => opts.append = true,
                    b'i' => opts.ignore_interrupts = true,
                    b'p' => opts.mode = Mode::WarnNoPipe,
                    _ => return Err(format!("invalid option -- '{}'", char::from(c))),
                }
            }
        }
    }
    Ok(Action::Copy(opts))
}

pub trait Streams {
    type Sink;
    type Error: Copy;
    fn stdout(&mut self) -> Self::Sink;
    fn open(&mut self, index: usize, name: &[u8], append: bool) -> Result<Self::Sink, Self::Error>;
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error>;
    fn write(&mut self, sink: &mut Self::Sink, bytes: &[u8]) -> Result<(), Self::Error>;
    fn close(&mut self, sink: Self::Sink) -> Result<(), Self::Error>;
    fn broken(&self, sink: &Self::Sink, error: Self::Error) -> bool;
    fn report(&mut self, name: &[u8], error: Self::Error);
    /// Avec une entrée intermittente, observer aussi la disparition du dernier lecteur.
    fn reader_gone(&mut self, _sink: &Self::Sink) -> Result<bool, Self::Error> {
        Ok(false)
    }
    fn default_pipe_status(&self) -> i32 {
        0
    }
}

pub fn copy<S: Streams>(opts: &Options, io: &mut S, buf: &mut [u8]) -> i32 {
    let stop = matches!(opts.mode, Mode::Exit | Mode::ExitNoPipe);
    let nopipe = matches!(opts.mode, Mode::WarnNoPipe | Mode::ExitNoPipe);
    let mut sinks = vec![(b"standard output".as_slice(), io.stdout())];
    let mut status = 0;
    for (i, &file) in opts.files.iter().enumerate() {
        match io.open(i, file, opts.append) {
            Ok(s) => sinks.push((file, s)),
            Err(e) => {
                io.report(file, e);
                status = 1;
                if stop {
                    return 1;
                }
            }
        }
    }
    while !sinks.is_empty() {
        if nopipe {
            match io.reader_gone(&sinks[0].1) {
                Ok(true) => {
                    let (_, s) = sinks.remove(0);
                    let _ = io.close(s);
                    continue;
                }
                Err(e) => {
                    io.report(b"iopoll error", e);
                    status = 1;
                }
                Ok(false) => {}
            }
        }
        let n = match io.read(buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) => {
                io.report(b"read error", e);
                status = 1;
                break;
            }
        };
        let mut i = 0;
        while i < sinks.len() {
            if let Err(e) = io.write(&mut sinks[i].1, &buf[..n]) {
                let (name, s) = sinks.remove(i);
                let broken = io.broken(&s, e);
                let _ = io.close(s);
                if broken && opts.mode == Mode::Default {
                    let code = io.default_pipe_status();
                    if code != 0 {
                        return code;
                    }
                } else if !broken || !nopipe {
                    io.report(name, e);
                    status = 1;
                    if stop {
                        return 1;
                    }
                }
            } else {
                i += 1;
            }
        }
    }
    for (name, s) in sinks {
        if let Err(e) = io.close(s) {
            io.report(name, e);
            status = 1;
        }
    }
    status
}
