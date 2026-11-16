use super::{
    args::{Action, parse},
    engine::Engine,
};
use std::{
    env,
    ffi::OsStr,
    fs::{File, OpenOptions},
    io::{self, BufRead, BufReader, BufWriter, Write},
    os::unix::ffi::OsStrExt,
};
#[path = "../../libraries/documentation_locale.rs"]
mod locale;
fn reason(e: io::Error) -> String {
    let s = e.to_string();
    s.split(" (os error ").next().unwrap_or(&s).to_owned()
}
fn run() -> Result<(), String> {
    let argv: Vec<_> = env::args_os().skip(1).collect();
    let args: Vec<_> = argv.iter().map(|a| a.as_bytes()).collect();
    let locale = ["LC_ALL", "LC_CTYPE", "LANG"]
        .iter()
        .find_map(|k| env::var(k).ok().filter(|s| !s.is_empty()))
        .unwrap_or_default()
        .to_ascii_lowercase();
    let utf8 = locale.contains("utf-8") || locale.contains("utf8");
    let strict = (200112..200809).contains(
        &env::var("_POSIX2_VERSION")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(200112),
    );
    let (o, files) = match parse(
        &args,
        utf8,
        env::var_os("POSIXLY_CORRECT").is_some(),
        !strict,
    )? {
        Action::Help => {
            let lang = locale::Language::from_tag(env::var("LANG").ok().as_deref());
            let help = lang.select(include_str!("../options.en"), include_str!("../options.fr"));
            let syntax = lang.select(
                "uniq [OPTION]... [INPUT [OUTPUT]]",
                "uniq [OPTION]... [ENTRÉE [SORTIE]]",
            );
            writeln!(
                io::stdout().lock(),
                "{syntax}\n{help}\nsort cities.txt | uniq -c\nuniq -D -i cities.txt"
            )
            .map_err(reason)?;
            return Ok(());
        }
        Action::Version => {
            writeln!(
                io::stdout().lock(),
                "uniq (NovaOS) {}",
                env!("CARGO_PKG_VERSION")
            )
            .map_err(reason)?;
            return Ok(());
        }
        Action::Run(o, f) => (o, f),
    };
    let name = |i| {
        files
            .get(i)
            .copied()
            .filter(|&p: &&[u8]| p != b"-")
            .map(OsStr::from_bytes)
    };
    let input: Box<dyn io::Read> = match name(0) {
        Some(p) => {
            Box::new(File::open(p).map_err(|e| format!("{}: {}", p.to_string_lossy(), reason(e)))?)
        }
        None => Box::new(io::stdin().lock()),
    };
    let output: Box<dyn Write> = match name(1) {
        Some(p) => Box::new(
            OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(p)
                .map_err(|e| format!("{}: {}", p.to_string_lossy(), reason(e)))?,
        ),
        None => Box::new(io::stdout().lock()),
    };
    let mut reader = BufReader::with_capacity(65536, input);
    let mut writer = BufWriter::with_capacity(65536, output);
    let d = o.delimiter;
    let mut engine = Engine::new(o);
    let mut row = Vec::new();
    let mut emit = |b: &[u8]| {
        writer
            .write_all(b)
            .map_err(|e| format!("write error: {}", reason(e)))
    };
    loop {
        row.clear();
        if reader.read_until(d, &mut row).map_err(|e| {
            format!(
                "error reading '{}': {}",
                name(0).unwrap_or(OsStr::new("-")).to_string_lossy(),
                reason(e)
            )
        })? == 0
        {
            break;
        }
        if row.last() == Some(&d) {
            row.pop();
        }
        engine.record(&row, &mut emit)?;
    }
    engine.finish(&mut emit)?;
    writer
        .flush()
        .map_err(|e| format!("write error: {}", reason(e)))
}
pub fn main() {
    // Le programme en ligne de commande doit transmettre SIGPIPE au shell appelant.
    unsafe extern "C" {
        fn signal(sig: i32, handler: usize) -> usize;
    }
    unsafe {
        signal(13, 0);
    }
    if let Err(e) = run() {
        eprintln!("uniq: {e}");
        std::process::exit(1)
    }
}
