use crate::{
    args::{self, Action, Config},
    engine::{self, Io, Stats},
};
use alloc::{
    format,
    string::{String, ToString},
    vec::Vec,
};
use nova_cli::{console, documentation, process, runtime as rt, tty};
use nova_exec_client::Launch;
struct Streams<'a> {
    read: &'a mut dyn FnMut(&mut [u8]) -> rt::Result<usize>,
    launch: &'a Launch<'a>,
    frame: [u8; process::PIPE_FRAME],
    start: usize,
    end: usize,
    pipe: bool,
    progress: Option<u64>,
}
fn reason(code: u64) -> alloc::string::String {
    console::reason(code).to_string()
}
impl Io for Streams<'_> {
    fn read(&mut self, out: &mut [u8]) -> engine::Result<usize> {
        if !self.pipe {
            return (self.read)(out).map_err(reason);
        }
        if self.start == self.end {
            self.end = (self.read)(&mut self.frame).map_err(reason)?;
            self.start = 0
        }
        let n = out.len().min(self.end - self.start);
        out[..n].copy_from_slice(&self.frame[self.start..self.start + n]);
        self.start += n;
        Ok(n)
    }
    fn write(&mut self, data: &[u8]) -> engine::Result<()> {
        self.launch.write_stdout(data).map_err(reason)
    }
    fn report(&mut self, stats: &Stats) {
        if let Some(previous) = self.progress
            && let Some(now) = clock()
            && now > previous
        {
            let _ = self
                .launch
                .write_stderr(format!("\r{} bytes copied", stats.bytes).as_bytes());
            self.progress = Some(now);
        }
    }
    fn warning(&mut self, msg: &str) {
        let _ = self.launch.write_stderr(format!("dd: {msg}\n").as_bytes());
    }
}
fn clock() -> Option<u64> {
    rt::value(rt::call(rt::CLOCK_UTC, 0, 0, 0)).ok()
}
fn run_copy(launch: &Launch, c: &Config) -> rt::Result<()> {
    // Le client commun offre une ouverture nommée en lecture et des ports d’octets.
    // Rejeter toute sémantique indisponible avant la première lecture.
    if c.output.is_some()
        || c.seek != 0
        || c.input == Some(b"-")
        || c.has("fsync")
        || c.has("fdatasync")
        || c.inf.iter().any(|s| {
            !matches!(
                *s,
                "count_bytes" | "skip_bytes" | "seek_bytes" | "fullblock"
            )
        })
        || c.outf
            .iter()
            .any(|s| !matches!(*s, "count_bytes" | "skip_bytes" | "seek_bytes"))
    {
        return Err(nova_cli::protocol::UNSUPPORTED);
    }
    let start = if matches!(c.status, "" | "progress") {
        clock()
    } else {
        None
    };
    let mut s = Stats::default();
    let result = launch.with_input(c.input.unwrap_or(b"-"), |read| {
        let mut streams = Streams {
            read,
            launch,
            frame: [0; process::PIPE_FRAME],
            start: 0,
            end: 0,
            pipe: c.input.is_none(),
            progress: if c.status == "progress" { start } else { None },
        };
        engine::copy(c, &mut streams, &mut s).map_err(|msg| {
            streams.warning(&msg);
            nova_cli::protocol::IO
        })
    });
    if c.status != "none" {
        launch.write_stderr(s.records().as_bytes())?;
        if c.status != "noxfer" {
            let duration = start.zip(clock()).and_then(|(a, b)| b.checked_sub(a));
            let suffix = match duration {
                Some(sec) if sec > 0 => format!(", {sec} s, {} B/s", s.bytes / sec),
                _ => String::new(),
            };
            launch.write_stderr(format!("{} bytes copied{suffix}\n", s.bytes).as_bytes())?
        }
    }
    result
}
fn execute(launch: &Launch) -> rt::Result<u64> {
    let words: Vec<_> = launch.argv.iter().map(|s| s.as_bytes()).collect();
    match args::parse(&words, launch.posix_options) {
        Ok(doc @ (Action::Help | Action::Version)) => {
            let page = documentation::lookup("dd").unwrap();
            let lang = if launch.french {
                documentation::Language::French
            } else {
                documentation::Language::English
            };
            let text = if matches!(doc, Action::Help) {
                page.render(&[], false, lang).unwrap()
            } else {
                page.version(lang)
            };
            launch.write_stdout(text.as_bytes())?;
            Ok(0)
        }
        Ok(Action::Run(c)) => {
            for _ in 0..c.warnings {
                launch.write_stderr(
                    b"dd: warning: '0x' is a zero multiplier; use '00x' if that is intended\n",
                )?
            }
            match run_copy(launch, &c) {
                Ok(()) => Ok(0),
                Err(e) => {
                    launch.write_stderr(format!("dd: {}\n", console::reason(e)).as_bytes())?;
                    Ok(1)
                }
            }
        }
        Err(args::Error(msg, usage)) => {
            launch.write_stderr(format!("dd: {msg}\n").as_bytes())?;
            if usage {
                launch.write_stderr(b"Try 'dd --help' for more information.\n")?
            }
            Ok(1)
        }
    }
}
#[unsafe(no_mangle)]
extern "C" fn run() -> ! {
    // Le lancement documentaire ARC-026 historique fournit seulement OUTPUT.
    // Une demande métier passe exclusivement par le contrat NXEXEC01.
    if let Ok(launch) = Launch::current() {
        rt::exit(execute(&launch).unwrap_or(1))
    }
    let code = (|| {
        let argv = process::arguments()?;
        let words: Vec<_> = argv.iter().map(|s| s.as_bytes()).collect();
        match args::parse(&words, false) {
            Ok(doc @ (Action::Help | Action::Version)) => console::document(
                documentation::lookup("dd").unwrap(),
                &argv,
                matches!(doc, Action::Help),
            ),
            _ => {
                console::write(tty::ERROR, "dd: lancement de flux indisponible\n")?;
                Err(rt::DENIED)
            }
        }
    })();
    rt::exit(code.err().unwrap_or(0))
}
