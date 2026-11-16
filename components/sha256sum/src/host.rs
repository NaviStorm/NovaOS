use nova_sha256sum::{Platform, Source, run};
use std::{
    ffi::OsStr,
    fs::File,
    io::{self, Read, Write},
    os::unix::ffi::OsStrExt,
};
#[path = "../../libraries/documentation_locale.rs"]
mod locale;
struct Darwin;
impl Source for Darwin {
    type Stream = Option<File>;
    type Error = io::Error;
    const BUFFER: usize = 65536;
    fn open(&mut self, path: &[u8]) -> io::Result<Self::Stream> {
        if path == b"-" {
            Ok(None)
        } else {
            File::open(OsStr::from_bytes(path)).map(Some)
        }
    }
    fn read(&mut self, stream: &mut Self::Stream, buf: &mut [u8]) -> io::Result<usize> {
        loop {
            let result = match stream {
                Some(f) => f.read(buf),
                None => io::stdin().lock().read(buf),
            };
            if !matches!(&result,Err(e) if e.kind()==io::ErrorKind::Interrupted) {
                return result;
            }
        }
    }
}
impl Platform for Darwin {
    fn missing(e: &io::Error) -> bool {
        e.kind() == io::ErrorKind::NotFound
    }
    fn reason(e: &io::Error) -> String {
        let msg = e.to_string();
        msg.split(" (os error ").next().unwrap().into()
    }
    fn write(&mut self, err: bool, bytes: &[u8]) -> io::Result<()> {
        if err {
            io::stderr().lock().write_all(bytes)
        } else {
            io::stdout().lock().write_all(bytes)
        }
    }
}
pub fn main() {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let args: Vec<_> = args.iter().map(|s| s.as_bytes()).collect();
    let tag = ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .find_map(|k| std::env::var(k).ok().filter(|s| !s.is_empty()));
    let tag = tag
        .as_deref()
        .map(|s| s.split('.').next().unwrap().replace('_', "-"));
    let fr = locale::Language::from_tag(tag.as_deref()).select("en", "fr") == "fr";
    let ok = run(&mut Darwin, &args, fr)
        .and_then(|ok| io::stdout().flush().map(|_| ok))
        .unwrap_or_else(|e| {
            let _ = writeln!(io::stderr(), "sha256sum: {e}");
            false
        });
    std::process::exit(i32::from(!ok));
}
