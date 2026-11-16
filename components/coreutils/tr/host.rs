#![no_main]
extern crate alloc;
#[path = "../../libraries/documentation.rs"]
mod documentation;
mod engine;
use engine::Action;
use std::{
    env,
    ffi::{CStr, c_char},
    io::{self, Read, Write},
};

type IoResult = Result<(), (&'static str, io::Error)>;

fn filter(input: &mut impl Read, output: &mut impl Write, mut filter: engine::Filter) -> IoResult {
    let mut buf = [0u8; 65536];
    let chunk = env::var("NOVA_TR_READ_SIZE")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(buf.len())
        .clamp(1, buf.len());
    loop {
        let n = match input.read(&mut buf[..chunk]) {
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            v => v.map_err(|e| ("read error", e))?,
        };
        if n == 0 {
            break;
        }
        let end = filter.apply(&mut buf[..n]);
        output
            .write_all(&buf[..end])
            .map_err(|e| ("write error", e))?;
    }
    Ok(())
}

// L'entrée C de Darwin conserve la disposition de SIGPIPE choisie par l'appelant.
// Le CRT garantit argc pointeurs vers des chaînes C pendant toute l'invocation.
#[unsafe(no_mangle)]
unsafe extern "C" fn main(argc: i32, argv: *const *const c_char) -> i32 {
    let argv = unsafe { std::slice::from_raw_parts(argv, argc as usize) };
    let words: Vec<_> = argv
        .iter()
        .map(|&p| unsafe { CStr::from_ptr(p) }.to_bytes())
        .collect();
    let Some((name, args)) = words.split_first() else {
        return 1;
    };
    let name = String::from_utf8_lossy(name);
    let locale = ["LC_ALL", "LC_CTYPE", "LANG"]
        .iter()
        .find_map(|k| env::var(k).ok().filter(|s| !s.is_empty()))
        .unwrap_or_default()
        .to_ascii_lowercase();
    let utf8 = locale.contains("utf-8") || locale.contains("utf8");
    let lang = documentation::Language::from_tag(env::var("LANG").ok().as_deref());
    let page = documentation::lookup("tr").unwrap();
    let action = engine::prepare(&name, args, utf8, |msg| eprintln!("{name}: {msg}"));
    let mut output = io::BufWriter::with_capacity(65536, io::stdout().lock());
    let result = match action {
        Err(msg) => {
            let _ = io::stderr()
                .lock()
                .write_all(&[name.as_bytes(), b": ", &msg, b"\n"].concat());
            return 1;
        }
        Ok(Action::Help) => output
            .write_all(page.render(&[], false, lang).unwrap().as_bytes())
            .map_err(|e| ("write error", e)),
        Ok(Action::Version) => output
            .write_all(page.version(lang).as_bytes())
            .map_err(|e| ("write error", e)),
        Ok(Action::Filter(f)) => filter(&mut io::stdin().lock(), &mut output, f),
    }
    .and_then(|_| output.flush().map_err(|e| ("write error", e)));
    if let Err((operation, error)) = result {
        let message = error.to_string();
        let message = message.split(" (os error ").next().unwrap_or(&message);
        eprintln!("{name}: {operation}: {message}");
        return 1;
    }
    0
}
