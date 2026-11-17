use crate::{
    format::{self, Value as V},
    options, quoting,
};
use alloc::{format, string::String, vec::Vec};
use nova_cli::{
    console, documentation, fileapi::Client, process, protocol as w, runtime as rt, tty,
};

fn utc(sec: i64, ns: u32) -> String {
    let day = sec.div_euclid(86400) + 719468;
    let era = day.div_euclid(146097);
    let d = day.rem_euclid(146097);
    let y = (d - d / 1460 + d / 36524 - d / 146096) / 365;
    let ordinal = d - (365 * y + y / 4 - y / 100);
    let m = (5 * ordinal + 2) / 153;
    let day = ordinal - (153 * m + 2) / 5 + 1;
    let month = if m < 10 { m + 3 } else { m - 9 };
    let year = y + era * 400 + i64::from(month <= 2);
    let s = sec.rem_euclid(86400);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}:{:02}.{ns:09} +0000",
        s / 3600,
        s / 60 % 60,
        s % 60
    )
}
fn value(f: &w::Frame, k: &[u8], name: &[u8]) -> V {
    match k {
        b"n" => V::text(name),
        b"N" | b"Qn" => V::Text(quoting::quote(name, quoting::Style::ShellEscape, true)),
        b"i" => V::dec(w::word(f, w::PAYLOAD)),
        b"s" => V::dec(w::word(f, w::PAYLOAD + 16)),
        b"F" => V::text(if f[w::PAYLOAD + 24] == 2 {
            "directory"
        } else if w::word(f, w::PAYLOAD + 16) == 0 {
            "regular empty file"
        } else {
            "regular file"
        }),
        b"w" | b"W" | b"x" | b"X" | b"y" | b"Y" | b"z" | b"Z" => {
            let index = match k[0].to_ascii_lowercase() {
                b'w' => 0,
                b'y' => 1,
                b'z' => 2,
                _ => 3,
            };
            let at = w::PAYLOAD + 25 + 12 * index;
            let ns = u32::from_le_bytes(f[at + 8..at + 12].try_into().unwrap());
            if w::length(f) != 73 || ns >= 1_000_000_000 {
                return match k {
                    b"W" => V::dec(0),
                    b"w" => V::text("-"),
                    _ => V::Unknown,
                };
            }
            let sec = i64::from_le_bytes(f[at..at + 8].try_into().unwrap());
            if k[0].is_ascii_uppercase() {
                V::Epoch(sec, ns)
            } else {
                V::text(utc(sec, ns))
            }
        }
        _ => V::Unknown,
    }
}
fn command() -> rt::Result<()> {
    let args = process::arguments()?;
    let bytes: Vec<_> = args.iter().map(|s| s.as_bytes()).collect();
    let o = options::parse(&bytes, false).map_err(|e| {
        let _ = console::printable(tty::ERROR, &format!("stat: {e}\n"));
        rt::BAD_ARGUMENT
    })?;
    if let Some(help) = o.doc {
        return console::document(documentation::lookup("stat").unwrap(), &args, help);
    }
    if rt::config(3) != 0 {
        return Err(rt::config(3));
    }
    if o.fs {
        return Err(w::UNSUPPORTED);
    }
    if o.files.iter().all(|n| *n == b"-") {
        return Err(w::UNSUPPORTED);
    }
    let mut client = Client::scoped()?;
    let mut status = 0;
    let mut fmt = format::template(&o, false);
    if o.format.is_none() {
        for b in &mut fmt {
            if *b == b'\t' {
                *b = b' ';
            }
        }
    }
    for (i, name) in o.files.iter().enumerate() {
        if *name == b"-" {
            status = w::UNSUPPORTED;
            continue;
        }
        let frame = match client.simple(w::STAT, i as u64 + 1) {
            Ok(f) => f,
            Err(e) => {
                status = e;
                continue;
            }
        };
        if !matches!(w::length(&frame), 25 | 73) {
            return Err(w::CORRUPT);
        }
        let mut out = Vec::new();
        let mut warnings = Vec::new();
        let result = format::render(
            &fmt,
            o.escapes,
            o.fs,
            |k| value(&frame, k, name),
            &mut out,
            |s| warnings.push(s),
        );
        for msg in warnings {
            console::printable(tty::ERROR, &format!("stat: {msg}\n"))?
        }
        let s = core::str::from_utf8(&out).map_err(|_| w::UNSUPPORTED)?;
        if s.chars().any(|c| c != '\n' && !console::safe(c)) {
            return Err(w::UNSUPPORTED);
        }
        console::write(tty::OUTPUT, s)?;
        if let Err(e) = result {
            console::printable(tty::ERROR, &format!("stat: {e}\n"))?;
            return Err(rt::BAD_ARGUMENT);
        }
    }
    if status == 0 { Ok(()) } else { Err(status) }
}
#[unsafe(no_mangle)]
extern "C" fn run() -> ! {
    let status = command().err().unwrap_or(0);
    if status != 0 {
        let _ = console::write(tty::ERROR, &format!("stat: {}\n", console::reason(status)));
    }
    rt::exit(status)
}
