//! Analyse commune au shell (sélection des capacités) et aux exécutables.
use crate::{command::Program, runtime as rt};
use alloc::vec::Vec;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Color {
    Never,
    Auto,
    Always,
}

pub struct Options<'a> {
    flags: u64,
    pub files: Vec<&'a str>,
    pub count: u64,
    pub bytes: bool,
    pub color: Color,
    pub help: bool,
    pub version: bool,
}
fn bit(c: u8) -> u64 {
    if c.is_ascii_lowercase() {
        1 << (c - b'a')
    } else {
        1 << (26 + c - b'A')
    }
}

impl Options<'_> {
    pub fn has(&self, c: u8) -> bool {
        self.flags & bit(c) != 0
    }
}

pub fn usage(p: Program) -> &'static str {
    use Program::*;
    match p {
        Ls => "ls [-aAlhrRSt1dF] [--color[=always|auto|never]] [chemin...]",
        Cat => "cat [--] fichier...",
        Head => "head [-qv] [-n N | -c N] fichier...",
        Tail => "tail [-qv] [-n N | -c N] fichier...",
        Wc => "wc [-lwcmL] fichier...",
        Cp => "cp [-invRr] source destination",
        Mv => "mv [-inv] source destination",
        Touch => "touch [-amc] fichier...",
        More => "more fichier...",
        Mkdir => "mkdir [--] répertoire",
        Rm => "rm [--] fichier",
        Pwd => "pwd [-LP]",
        Probe => "@probe scénario",
    }
}
fn allowed(p: Program) -> &'static [u8] {
    use Program::*;
    match p {
        Ls => b"aAlhrRSt1dF",
        Head | Tail => b"ncvq",
        Wc => b"lwcmL",
        Cp => b"invRr",
        Mv => b"inv",
        Touch => b"amc",
        Pwd => b"LP",
        _ => b"",
    }
}
fn named(p: Program, text: &str) -> Option<u8> {
    use Program::*;
    Some(match (p, text) {
        (Ls, "all") => b'a',
        (Ls, "almost-all") => b'A',
        (Ls, "human-readable") => b'h',
        (Ls | Cp, "recursive") => b'R',
        (Ls, "reverse") => b'r',
        (Ls, "directory") => b'd',
        (Ls, "classify") => b'F',
        (Head | Tail | Wc, "lines") => {
            if p == Wc {
                b'l'
            } else {
                b'n'
            }
        }
        (Head | Tail | Wc, "bytes") => b'c',
        (Head | Tail, "quiet" | "silent") => b'q',
        (Head | Tail | Cp | Mv, "verbose") => b'v',
        (Cp | Mv, "interactive") => b'i',
        (Cp | Mv, "no-clobber") => b'n',
        (Touch, "no-create") => b'c',
        (Wc, "words") => b'w',
        (Wc, "chars") => b'm',
        (Wc, "max-line-length") => b'L',
        (Pwd, "logical") => b'L',
        (Pwd, "physical") => b'P',
        _ => return None,
    })
}
pub fn parse<'a>(p: Program, args: &[&'a str]) -> rt::Result<Options<'a>> {
    let mut o = Options {
        flags: 0,
        files: Vec::new(),
        count: 10,
        bytes: false,
        color: Color::Never,
        help: false,
        version: false,
    };
    let mut end = false;
    let mut args = args.iter().copied();
    while let Some(arg) = args.next() {
        if end || arg == "-" || !arg.starts_with('-') {
            o.files.push(arg);
            continue;
        }
        if arg == "--" {
            end = true;
            continue;
        }
        let long = arg.strip_prefix("--");
        let (chars, value);
        let short;
        if let Some(raw) = long {
            let (name, v) = raw
                .split_once('=')
                .map_or((raw, None), |(k, v)| (k, Some(v)));
            if name == "color" && p == Program::Ls {
                o.color = match v {
                    None | Some("always") => Color::Always,
                    Some("auto") => Color::Auto,
                    Some("never") => Color::Never,
                    _ => return Err(rt::BAD_ARGUMENT),
                };
                continue;
            }
            if matches!(name, "help" | "version") && v.is_none() {
                o.help |= name == "help";
                o.version |= name == "version";
                continue;
            }
            short = [named(p, name).ok_or(rt::BAD_ARGUMENT)?];
            chars = &short[..];
            value = v;
        } else {
            chars = &arg.as_bytes()[1..];
            value = None;
        }
        for (i, &c) in chars.iter().enumerate() {
            if !allowed(p).contains(&c) {
                return Err(rt::BAD_ARGUMENT);
            }
            if matches!(p, Program::Head | Program::Tail) && matches!(c, b'n' | b'c') {
                let digits = if let Some(v) = value {
                    v
                } else if i + 1 < chars.len() {
                    core::str::from_utf8(&chars[i + 1..]).map_err(|_| rt::BAD_ARGUMENT)?
                } else {
                    args.next().ok_or(rt::BAD_ARGUMENT)?
                };
                if digits.is_empty() || !digits.bytes().all(|v| v.is_ascii_digit()) {
                    return Err(rt::BAD_ARGUMENT);
                }
                o.count = digits.parse().map_err(|_| rt::BAD_ARGUMENT)?;
                o.bytes = c == b'c';
                break;
            }
            if value.is_some() {
                return Err(rt::BAD_ARGUMENT);
            }
            if c != b'1' {
                o.flags |= bit(c);
            }
            if matches!(p, Program::Head | Program::Tail) && matches!(c, b'q' | b'v') {
                o.flags &= !bit(if c == b'q' { b'v' } else { b'q' });
            }
            if matches!(p, Program::Cp | Program::Mv) && matches!(c, b'i' | b'n') {
                o.flags &= !bit(if c == b'i' { b'n' } else { b'i' });
            }
            if p == Program::Ls && matches!(c, b'a' | b'A') {
                o.flags &= !bit(if c == b'a' { b'A' } else { b'a' });
            }
            if p == Program::Ls && matches!(c, b't' | b'S') {
                o.flags &= !bit(if c == b't' { b'S' } else { b't' });
            }
        }
    }
    Ok(o)
}
