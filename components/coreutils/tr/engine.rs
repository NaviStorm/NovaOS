use alloc::{format, string::String, vec::Vec};
#[path = "sets.rs"]
mod sets;
#[path = "tables.rs"]
mod tables;
use sets::{Group, Run};

#[derive(Default)]
struct Options {
    complement: bool,
    delete: bool,
    squeeze: bool,
    truncate: bool,
    ascii: bool,
}

pub enum Action {
    Help,
    Version,
    Filter(Filter),
}

pub struct Filter {
    map: [u8; 256],
    delete: [bool; 256],
    squeeze: [bool; 256],
    deleting: bool,
    squeezing: bool,
    previous: Option<u8>,
}

impl Filter {
    pub fn apply(&mut self, data: &mut [u8]) -> usize {
        if !self.deleting && !self.squeezing {
            for b in data.iter_mut() {
                *b = self.map[*b as usize];
            }
            return data.len();
        }
        let mut end = 0;
        for offset in 0..data.len() {
            let input = data[offset] as usize;
            if self.delete[input] {
                continue;
            }
            let value = self.map[input];
            if self.squeeze[value as usize] && self.previous == Some(value) {
                continue;
            }
            self.previous = Some(value);
            data[end] = value;
            end += 1;
        }
        end
    }
}

const LONG: [(&str, u8); 6] = [
    ("complement", b'c'),
    ("delete", b'd'),
    ("squeeze-repeats", b's'),
    ("truncate-set1", b't'),
    ("help", b'h'),
    ("version", b'v'),
];

pub fn prepare(
    name: &str,
    mut args: &[&[u8]],
    utf8: bool,
    mut warn: impl FnMut(String),
) -> Result<Action, Vec<u8>> {
    let mut opt = Options::default();
    let usage =
        |msg: String| format!("{msg}\nTry '{name} --help' for more information.").into_bytes();
    while let Some((&arg, tail)) = args.split_first() {
        if arg == b"--" {
            args = tail;
            break;
        }
        if arg.len() < 2 || arg[0] != b'-' {
            break;
        }
        let long;
        let flags = if let Some(raw) = arg.strip_prefix(b"--") {
            let key = raw.split(|&b| b == b'=').next().unwrap();
            let show = String::from_utf8_lossy(arg);
            if key.is_empty() {
                return Err(usage(format!(
                    "option '{show}' is ambiguous; possibilities: '--complement' '--delete' '--squeeze-repeats' '--truncate-set1' '--help' '--version'"
                )));
            }
            let Some(&(option, flag)) = LONG.iter().find(|(s, _)| s.as_bytes().starts_with(key))
            else {
                return Err(usage(format!("unrecognized option '{show}'")));
            };
            if key.len() != raw.len() {
                return Err(usage(format!(
                    "option '--{option}' doesn't allow an argument"
                )));
            }
            match flag {
                b'h' => return Ok(Action::Help),
                b'v' => return Ok(Action::Version),
                _ => (),
            }
            long = [flag];
            &long[..]
        } else {
            &arg[1..]
        };
        for &flag in flags {
            match flag {
                b'c' | b'C' => opt.complement = true,
                b'd' => opt.delete = true,
                b's' => opt.squeeze = true,
                b't' => opt.truncate = true,
                b'A' => opt.ascii = true,
                _ => return Err(usage(format!("invalid option -- '{}'", flag as char))),
            }
        }
        args = tail;
    }
    let utf8 = utf8 && !opt.ascii;
    let min = if opt.delete == opt.squeeze { 2 } else { 1 };
    let max = if opt.delete && !opt.squeeze { 1 } else { 2 };
    if args.is_empty() {
        return Err(usage("missing operand".into()));
    }
    if args.len() < min {
        let why = if opt.delete {
            "both deleting and squeezing repeats"
        } else {
            "translating"
        };
        return Err(usage(format!(
            "missing operand after {}\nTwo strings must be given when {why}.",
            sets::quoted(args[0], utf8)
        )));
    }
    if args.len() > max {
        let suffix = if args.len() == 2 {
            "\nOnly one string may be given when deleting without squeezing repeats."
        } else {
            ""
        };
        return Err(usage(format!(
            "extra operand {}{suffix}",
            sets::quoted(args[max], utf8)
        )));
    }
    let wide = utf8 && !opt.ascii;
    let classes = if wide { &tables::UTF8 } else { &tables::C };
    let mut src = sets::parse(args[0], classes, utf8, &mut warn)?;
    let mut dst = match args.get(1) {
        Some(raw) => sets::parse(raw, classes, utf8, &mut warn)?,
        None => Vec::new(),
    };
    let mut source_len = length(&src)?;
    if src.iter().any(|r| r.group == Group::Fill) {
        return Err("the [c*] repeat construct may not appear in string1".into());
    }
    let has_class = src.iter().any(|r| matches!(r.group, Group::Class(_)));
    let mut domain = membership(&src);
    if opt.complement {
        domain.iter_mut().for_each(|b| *b = !*b);
        src = (0..=255u8)
            .filter(|&b| domain[b as usize])
            .map(|b| Run::new(b, 1))
            .collect();
        source_len = src.len() as u64;
    }
    let dest_len = length(&dst)?;
    let translating = !opt.delete && args.len() == 2;
    let mut fills = dst.iter_mut().filter(|r| r.group == Group::Fill);
    if let Some(fill) = fills.next() {
        fill.count = source_len.saturating_sub(dest_len);
        if fills.next().is_some() {
            return Err("only one [c*] repeat construct may appear in string2".into());
        }
        if !translating {
            return Err("the [c*] construct may appear in string2 only when translating".into());
        }
    }
    let mut filter = Filter {
        map: core::array::from_fn(|i| i as u8),
        delete: if opt.delete { domain } else { [false; 256] },
        squeeze: if opt.squeeze {
            if args.len() == 1 {
                domain
            } else {
                membership(&dst)
            }
        } else {
            [false; 256]
        },
        previous: None,
        deleting: opt.delete,
        squeezing: opt.squeeze,
    };
    if translating {
        if dst.iter().any(|r| r.group == Group::Equivalent) {
            return Err("[=c=] expressions may not appear in string2 when translating".into());
        }
        if dst
            .iter()
            .any(|r| matches!(r.group,Group::Class(i) if i!=6 && i!=10))
        {
            return Err("when translating, the only character classes that may appear in\nstring2 are 'upper' and 'lower'".into());
        }
        translate(&mut filter.map, &src, &dst, wide, &opt)?;
        if opt.complement && has_class {
            let n = length(&dst)?;
            let unique = membership(&dst).into_iter().filter(|&x| x).count();
            if unique != 1 || n > source_len || (n < source_len && opt.truncate) {
                return Err("when translating with complemented character classes,\nstring2 must map all characters in the domain to one".into());
            }
        }
    }
    Ok(Action::Filter(filter))
}

fn membership(runs: &[Run]) -> [bool; 256] {
    let mut set = [false; 256];
    for r in runs {
        if r.count > 0 {
            set[r.byte as usize] = true;
        }
    }
    set
}
fn length(runs: &[Run]) -> Result<u64, String> {
    runs.iter()
        .try_fold(0u64, |n, r| {
            n.checked_add(r.count).filter(|&v| v < u64::MAX)
        })
        .ok_or_else(|| "too many characters in set".into())
}

struct Cursor<'a> {
    runs: &'a [Run],
    offset: u64,
}
impl<'a> Cursor<'a> {
    fn front(&mut self) -> Option<&'a Run> {
        while self.runs.first().is_some_and(|r| r.count == self.offset) {
            self.runs = &self.runs[1..];
            self.offset = 0;
        }
        self.runs.first()
    }
    fn skip_class(&mut self) {
        self.runs = &self.runs[1..];
        self.offset = 0;
        let n = self.runs.iter().take_while(|r| !r.first).count();
        self.runs = &self.runs[n..];
    }
}

fn translate(
    map: &mut [u8; 256],
    src: &[Run],
    dst: &[Run],
    wide: bool,
    opt: &Options,
) -> Result<(), String> {
    let mut from = Cursor {
        runs: src,
        offset: 0,
    };
    let mut to = Cursor {
        runs: dst,
        offset: 0,
    };
    loop {
        let left = from.front();
        let right = to.front();
        if !opt.complement
            && let Some(r) = right
            && let Group::Class(target) = r.group
        {
            let Some(l) = left.filter(|l| l.first && matches!(l.group, Group::Class(6 | 10)))
            else {
                return Err("misaligned [:upper:] and/or [:lower:] construct".into());
            };
            if l.group == r.group {
                // Pour une paire identique GNU visite le premier membre et saute la classe.
                map[l.byte as usize] = r.byte;
            } else {
                let conversion = match (wide, target) {
                    (true, 6) => &tables::UTF8_LOWER,
                    (true, _) => &tables::UTF8_UPPER,
                    (false, 6) => &tables::C_LOWER,
                    (false, _) => &tables::C_UPPER,
                };
                for b in core::iter::once(l).chain(from.runs[1..].iter().take_while(|b| !b.first)) {
                    map[b.byte as usize] = conversion[b.byte as usize];
                }
            }
            from.skip_class();
            to.skip_class();
            continue;
        }
        let Some(l) = left else { return Ok(()) };
        let Some(r) = right else {
            if opt.truncate {
                return Ok(());
            }
            let Some(last) = dst.last().filter(|_| dst.iter().any(|r| r.count > 0)) else {
                return Err("when not truncating set1, string2 must be non-empty".into());
            };
            if matches!(last.group, Group::Class(_)) {
                return Err("when translating with string1 longer than string2,\nthe latter string must not end with a character class".into());
            }
            for entry in from.runs {
                map[entry.byte as usize] = last.byte;
            }
            return Ok(());
        };
        let count = (l.count - from.offset).min(r.count - to.offset);
        map[l.byte as usize] = r.byte;
        from.offset += count;
        to.offset += count;
    }
}
