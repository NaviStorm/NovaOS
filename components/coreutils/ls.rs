#![no_std]
#![no_main]
extern crate alloc;
mod common;
use alloc::{format, string::String, vec::Vec};
use nova_cli::{
    command::{Operand, Program},
    console,
    directory::{Entry, entries},
    fileapi::Client,
    options::{Color, Options},
    protocol as w, runtime as rt, tty,
};

fn modified(a: &w::Frame) -> Option<(i64, u32)> {
    if w::length(a) != 73 {
        return None;
    }
    let p = w::PAYLOAD + 25 + 12;
    let seconds = i64::from_le_bytes(a[p..p + 8].try_into().unwrap());
    let nanos = u32::from_le_bytes(a[p + 8..p + 12].try_into().unwrap());
    (nanos < 1_000_000_000).then_some((seconds, nanos))
}

fn date(t: Option<(i64, u32)>) -> String {
    let Some((sec, ns)) = t else {
        return "inconnue".into();
    };
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
        "{year:04}-{month:02}-{day:02} {:02}:{:02}:{:02}.{ns:09}Z",
        s / 3600,
        s / 60 % 60,
        s % 60
    )
}
fn size(n: u64, human: bool) -> String {
    if human {
        for (shift, unit) in [
            (60, "E"),
            (50, "P"),
            (40, "T"),
            (30, "G"),
            (20, "M"),
            (10, "K"),
        ] {
            let scale = 1u64 << shift;
            if n >= scale {
                let tenths = (u128::from(n) * 10).div_ceil(u128::from(scale));
                if tenths < 100 {
                    return format!("{}.{:01}{unit}", tenths / 10, tenths % 10);
                }
                return format!("{}{unit}", n.div_ceil(scale));
            }
        }
    }
    format!("{n}")
}

fn compare(a: &Entry, b: &Entry, o: &Options) -> core::cmp::Ordering {
    // Une date inconnue reste après les dates connues dans les deux sens du tri.
    if o.has(b't') && modified(&a.stat).is_some() != modified(&b.stat).is_some() {
        return modified(&b.stat)
            .is_some()
            .cmp(&modified(&a.stat).is_some());
    }
    let rank = if o.has(b't') {
        modified(&b.stat).cmp(&modified(&a.stat))
    } else if o.has(b'S') {
        w::word(&b.stat, w::PAYLOAD + 16).cmp(&w::word(&a.stat, w::PAYLOAD + 16))
    } else {
        core::cmp::Ordering::Equal
    };
    let result = rank.then_with(|| a.name.as_bytes().cmp(b.name.as_bytes()));
    if o.has(b'r') {
        result.reverse()
    } else {
        result
    }
}

fn show(e: &Entry, o: &Options) -> rt::Result<()> {
    let dir = e.stat[w::PAYLOAD + 24] == 2;
    let kind = if dir { "répertoire" } else { "fichier" };
    let p9 = w::length(&e.stat) == 73;
    if o.has(b'l') && p9 {
        console::write(
            tty::OUTPUT,
            &format!(
                "{kind}  {}  {}  ",
                size(w::word(&e.stat, w::PAYLOAD + 16), o.has(b'h')),
                date(modified(&e.stat))
            ),
        )?;
    } else if !p9 {
        console::write(tty::OUTPUT, &format!("{kind} "))?
    }
    let color = o.color != Color::Never && dir;
    console::printable(
        if color {
            tty::DIRECTORY_COLOR
        } else {
            tty::OUTPUT
        },
        &e.name,
    )?;
    if dir && o.has(b'F') {
        console::write(tty::OUTPUT, "/")?
    }
    if !p9 && o.has(b'l') {
        console::write(
            tty::OUTPUT,
            &format!(
                "  id={} taille={} mtime=inconnue",
                w::word(&e.stat, w::PAYLOAD),
                size(w::word(&e.stat, w::PAYLOAD + 16), o.has(b'h'))
            ),
        )?;
    }
    console::write(tty::OUTPUT, "\n")
}
fn walk(
    c: &mut Client,
    root: u64,
    o: &Options,
    label: &str,
    path: &mut Vec<String>,
) -> rt::Result<()> {
    let rows = nova_cli::directory::within(c, root, path, w::TRAVERSE | w::ENUMERATE, |c, dir| {
        let mut rows = entries(c, dir)?;
        if !o.has(b'a') && !o.has(b'A') {
            rows.retain(|e| !e.name.starts_with('.'));
        }
        if o.has(b'a') && !o.has(b'A') {
            rows.push(Entry {
                name: ".".into(),
                stat: c.simple(w::STAT, dir)?,
            });
        }
        rows.sort_unstable_by(|a, b| compare(a, b, o));
        for e in &rows {
            show(e, o)?
        }
        Ok(rows)
    })?;
    if o.has(b'R') {
        for e in rows
            .into_iter()
            .filter(|e| e.name != "." && e.stat[w::PAYLOAD + 24] == 2)
        {
            let name = format!("{}/{}", label.trim_end_matches('/'), e.name);
            console::printable(tty::OUTPUT, &format!("\n{name}:\n"))?;
            path.push(e.name);
            walk(c, root, o, &name, path)?;
            path.pop();
        }
    }
    Ok(())
}
fn list(c: &mut Client, op: Operand) -> rt::Result<()> {
    let Operand::Batch(o, _) = op else {
        return Err(rt::BAD_ARGUMENT);
    };
    let default = ["."];
    let paths = if o.files.is_empty() {
        &default[..]
    } else {
        &o.files
    };
    let mut files = Vec::new();
    let mut dirs = Vec::new();
    for (i, &path) in paths.iter().enumerate() {
        let h = i as u64 + 1;
        let st = c.simple(w::STAT, h)?;
        if o.has(b't') && w::length(&st) != 73 {
            return Err(w::UNSUPPORTED);
        }
        let entry = Entry {
            name: path.into(),
            stat: st,
        };
        if o.has(b'd') || st[w::PAYLOAD + 24] != 2 {
            files.push(entry);
        } else {
            dirs.push((h, entry));
        }
    }
    files.sort_unstable_by(|a, b| compare(a, b, &o));
    dirs.sort_unstable_by(|(_, a), (_, b)| compare(a, b, &o));
    for e in files {
        show(&e, &o)?
    }
    for (dir, entry) in dirs {
        let name = entry.name.as_str();
        if paths.len() > 1 || o.has(b'R') {
            console::printable(tty::OUTPUT, &format!("{name}:\n"))?
        }
        walk(c, dir, &o, name, &mut Vec::new())?;
    }
    Ok(())
}
// Sûreté : entrée Rust de cet ELF utilisateur, appelée depuis _start.
#[unsafe(no_mangle)]
extern "C" fn run() -> ! {
    common::run(Program::Ls, list)
}
