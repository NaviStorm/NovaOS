use super::{
    args::{Action, parse},
    engine::{Config, Stream},
};
use std::{
    env,
    ffi::OsStr,
    fs::File,
    io::{self, Read, Write},
    os::unix::ffi::{OsStrExt, OsStringExt},
    path::Path,
};
#[allow(dead_code)]
#[path = "../../libraries/documentation.rs"]
mod documentation;
fn process<R: Read, W: Write>(mut input: R, output: &mut W, cfg: &Config) -> io::Result<()> {
    let mut buf = vec![
        0;
        if cfg.mode == super::engine::Mode::Fields && cfg.ranges.is_empty() {
            262144
        } else {
            65536
        }
    ];
    let mut result = Vec::with_capacity(65536);
    let mut stream = Stream::new(cfg);
    loop {
        let n = match input.read(&mut buf) {
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            r => r?,
        };
        result.clear();
        if n == 0 {
            stream.finish(&mut result);
            return output.write_all(&result);
        }
        stream.feed(&buf[..n], &mut result);
        output.write_all(&result)?;
    }
}
fn filename(bytes: &[u8], utf8: bool) -> String {
    let printable = |b: u8| (32..127).contains(&b);
    let mut parts = Vec::new();
    for chunk in bytes.utf8_chunks() {
        for c in chunk.valid().chars() {
            let mut buf = [0; 4];
            let unit = c.encode_utf8(&mut buf).as_bytes();
            let normal = if c.is_ascii() {
                printable(unit[0])
            } else {
                utf8 && !c.is_control()
            };
            parts.extend(unit.iter().map(|&b| (b, normal)));
        }
        parts.extend(chunk.invalid().iter().map(|&b| (b, false)));
    }
    if !parts.is_empty()
        && parts.iter().enumerate().all(|(i, &(b, normal))| {
            normal
                && (b >= 128
                    || b.is_ascii_alphanumeric()
                    || b"_+,-./@%".contains(&b)
                    || i > 0 && b"#~".contains(&b))
        })
    {
        return String::from_utf8_lossy(bytes).into_owned();
    }
    if bytes.contains(&b'\'')
        && parts
            .iter()
            .all(|&(b, normal)| normal && !b"\"$\\\x60".contains(&b))
    {
        return format!("\"{}\"", String::from_utf8_lossy(bytes));
    }
    let mut out = vec![b'\''];
    let mut escaped = false;
    for (b, normal) in parts {
        if !normal {
            if !escaped {
                out.extend_from_slice(b"'$'");
                escaped = true
            }
            match b {
                7..=13 => {
                    out.push(b'\\');
                    out.push(b"abtnvfr"[(b - 7) as usize]);
                }
                _ => out.extend(format!("\\{b:03o}").bytes()),
            }
        } else {
            if escaped {
                out.extend_from_slice(b"''");
                escaped = false
            }
            if b == b'\'' {
                out.extend_from_slice(br"'\''")
            } else {
                out.push(b)
            }
        }
    }
    out.push(b'\'');
    String::from_utf8(out).unwrap()
}
pub fn main() {
    unsafe extern "C" {
        fn signal(sig: i32, handler: usize) -> usize;
    }
    // Darwin signals a closed stdout pipe to the caller, as for other CLI filters.
    unsafe {
        signal(13, 0);
    }

    let args: Vec<_> = env::args_os().skip(1).map(|a| a.into_vec()).collect();
    let refs: Vec<_> = args.iter().map(Vec::as_slice).collect();
    let loc = ["LC_ALL", "LC_CTYPE", "LANG"]
        .iter()
        .find_map(|k| env::var(k).ok().filter(|v| !v.is_empty()))
        .unwrap_or_default()
        .to_ascii_lowercase();
    let utf8 = loc.contains("utf-8") || loc.contains("utf8");
    let action = parse(&refs, utf8, env::var_os("POSIXLY_CORRECT").is_some());
    let cmd = env::args_os().next().unwrap_or_default();
    let name = cmd.to_string_lossy();
    let cfg_files = match action {
        Ok(Action::Run(c, f)) => (c, f),
        Ok(doc) => {
            let lang = documentation::Language::from_tag(env::var("LANG").ok().as_deref());
            let page = documentation::lookup("cut").unwrap();
            let msg = if matches!(doc, Action::Help) {
                page.render(&[], false, lang).unwrap()
            } else {
                page.version(lang)
            };
            if io::stdout()
                .write_all(msg.as_bytes())
                .and_then(|_| io::stdout().flush())
                .is_err()
            {
                std::process::exit(1)
            }
            return;
        }
        Err(e) => {
            eprintln!("{name}: {e}\nTry '{name} --help' for more information.");
            std::process::exit(1)
        }
    };
    let (cfg, files) = cfg_files;
    let mut output = io::BufWriter::new(io::stdout().lock());
    let mut failed = false;
    for f in files {
        let path = Path::new(OsStr::from_bytes(f));
        let r = if f == b"-" {
            process(io::stdin().lock(), &mut output, &cfg)
        } else {
            File::open(path).and_then(|fd| process(fd, &mut output, &cfg))
        };
        if let Err(e) = r {
            if e.kind() == io::ErrorKind::BrokenPipe {
                std::process::exit(141)
            }
            let message = e.to_string();
            let reason = message.split(" (os error ").next().unwrap_or(&message);
            eprintln!("{name}: {}: {reason}", filename(f, utf8));
            failed = true
        }
    }
    if let Err(e) = output.flush() {
        eprintln!("{name}: write error: {e}");
        failed = true
    }
    if failed {
        std::process::exit(1)
    }
}
