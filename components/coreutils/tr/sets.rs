use super::tables::NAMES;
use alloc::{format, string::String, vec::Vec};

#[derive(Clone, Copy, PartialEq)]
pub(super) enum Group {
    Byte,
    Class(usize),
    Equivalent,
    Fill,
}

/// Un nombre de répétitions, même de plusieurs milliards, tient dans une entrée.
pub(super) struct Run {
    pub byte: u8,
    pub count: u64,
    pub group: Group,
    pub first: bool,
}

impl Run {
    pub fn new(byte: u8, count: u64) -> Self {
        Self {
            byte,
            count,
            group: Group::Byte,
            first: true,
        }
    }
}

fn printable(b: u8, classes: &[u16; 256]) -> Vec<u8> {
    if classes[b as usize] & (1 << 7) != 0 {
        return alloc::vec![b];
    }
    format!("\\{b:03o}").into_bytes()
}

pub(super) fn quoted(bytes: &[u8], utf8: bool) -> String {
    let mut out = String::from(if utf8 { "‘" } else { "'" });
    let mut rest = bytes;
    while let Some((&b, tail)) = rest.split_first() {
        if utf8 && b >= 128 {
            let n = b.leading_ones() as usize;
            if let Some(s) = rest.get(..n).and_then(|x| core::str::from_utf8(x).ok()) {
                out.push_str(s);
                rest = &rest[n..];
                continue;
            }
        }
        match b {
            b'\\' => out.push_str("\\\\"),
            b'\'' if !utf8 => out.push_str("\\'"),
            7..=13 => {
                out.push('\\');
                out.push(b"abtnvfr"[(b - 7) as usize] as char);
            }
            32..=126 => out.push(b as char),
            _ => out.push_str(&format!("\\{b:03o}")),
        }
        rest = tail;
    }
    out.push(if utf8 { '’' } else { '\'' });
    out
}

#[derive(Copy, Clone)]
struct Atom {
    b: u8,
    escaped: bool,
}
impl Atom {
    fn is(self, b: u8) -> bool {
        self.b == b && !self.escaped
    }
}

fn decode(raw: &[u8], warn: &mut impl FnMut(String)) -> Vec<Atom> {
    let mut atoms = Vec::with_capacity(raw.len());
    let mut iter = raw.iter().copied().peekable();
    while let Some(mut b) = iter.next() {
        let escaped = b == b'\\';
        if escaped {
            let Some(next) = iter.next() else {
                warn("warning: an unescaped backslash at end of string is not portable".into());
                atoms.push(Atom { b, escaped: false });
                break;
            };
            b = match next {
                b'0'..=b'7' => {
                    let mut value = next - b'0';
                    for _ in 0..2 {
                        let Some(&digit @ b'0'..=b'7') = iter.peek() else {
                            break;
                        };
                        let Some(v) = value.checked_mul(8) else {
                            let d = digit as char;
                            warn(format!(
                                "warning: the ambiguous octal escape \\{value:o}{d} is being\n\tinterpreted as the 2-byte sequence \\0{value:o}, {d}"
                            ));
                            break;
                        };
                        value = v + (digit - b'0');
                        iter.next();
                    }
                    value
                }
                b'a' => 7,
                b'b' => 8,
                b't' => 9,
                b'n' => 10,
                b'v' => 11,
                b'f' => 12,
                b'r' => 13,
                _ => next,
            };
        }
        atoms.push(Atom { b, escaped });
    }
    atoms
}

fn descriptive(value: &[u8], classes: &[u16; 256]) -> Vec<u8> {
    value
        .iter()
        .flat_map(|&b| match b {
            7..=13 => format!("\\{}", b"abtnvfr"[(b - 7) as usize] as char).into_bytes(),
            _ => printable(b, classes),
        })
        .collect()
}

pub(super) fn parse(
    raw: &[u8],
    classes: &[u16; 256],
    utf8: bool,
    warn: &mut impl FnMut(String),
) -> Result<Vec<Run>, Vec<u8>> {
    let atoms = decode(raw, warn);
    let mut input = atoms.as_slice();
    let mut out = Vec::new();
    while !input.is_empty() {
        let byte = input[0].b;
        if input.len() >= 3 && input[0].is(b'[') {
            let mark = input[1];
            let digits = &input[3..];
            let rep = input[2]
                .is(b'*')
                .then(|| digits.iter().position(|a| a.escaped || a.is(b']')))
                .flatten()
                .filter(|&i| digits[i].is(b']'));
            if (mark.is(b':') || mark.is(b'='))
                && let Some(n) = input[2..]
                    .windows(2)
                    .position(|w| w[0].is(mark.b) && w[1].is(b']'))
            {
                let name: Vec<_> = input[2..2 + n].iter().map(|a| a.b).collect();
                if n == 0 {
                    return Err(if mark.b == b':' {
                        "missing character class name '[::]'"
                    } else {
                        "missing equivalence class character '[==]'"
                    }
                    .into());
                }
                if mark.b == b':'
                    && let Some(class) = NAMES.iter().position(|&s| s == name)
                {
                    let mut first = true;
                    for b in 0..=255u8 {
                        if classes[b as usize] & (1 << class) != 0 {
                            out.push(Run {
                                byte: b,
                                count: 1,
                                group: Group::Class(class),
                                first,
                            });
                            first = false;
                        }
                    }
                    input = &input[n + 4..];
                    continue;
                }
                if mark.b == b'=' && n == 1 {
                    out.push(Run {
                        byte: name[0],
                        count: 1,
                        group: Group::Equivalent,
                        first: true,
                    });
                    input = &input[n + 4..];
                    continue;
                }
                if !rep.is_some_and(|i| digits[..i].iter().all(|a| a.b.is_ascii_digit())) {
                    let desc = descriptive(&name, classes);
                    return Err(if mark.b == b':' {
                        format!("invalid character class {}", quoted(&desc, utf8)).into_bytes()
                    } else {
                        [
                            &desc[..],
                            b": equivalence class operand must be a single character",
                        ]
                        .concat()
                    });
                }
            }
            if let Some(n) = rep {
                let bytes: Vec<_> = digits[..n].iter().map(|a| a.b).collect();
                let radix = if bytes.first() == Some(&b'0') { 8 } else { 10 };
                let count = if bytes.is_empty() {
                    0
                } else {
                    core::str::from_utf8(&bytes)
                        .ok()
                        .and_then(|s| {
                            u64::from_str_radix(
                                s.trim_start_matches([' ', '\t', '\r', '\n', '\x0b', '\x0c']),
                                radix,
                            )
                            .ok()
                        })
                        .filter(|&v| v < u64::MAX)
                        .ok_or_else(|| {
                            format!(
                                "invalid repeat count {} in [c*n] construct",
                                quoted(&descriptive(&bytes, classes), utf8)
                            )
                        })?
                };
                let mut run = Run::new(mark.b, count);
                if count == 0 {
                    run.group = Group::Fill;
                }
                out.push(run);
                input = &input[n + 4..];
                continue;
            }
        }
        if input.len() >= 3 && input[1].is(b'-') {
            let last = input[2].b;
            if byte > last {
                return Err([
                    &b"range-endpoints of '"[..],
                    &printable(byte, classes),
                    b"-",
                    &printable(last, classes),
                    b"' are in reverse collating sequence order",
                ]
                .concat());
            }
            out.extend((byte..=last).map(|b| Run::new(b, 1)));
            input = &input[3..];
        } else {
            out.push(Run::new(byte, 1));
            input = &input[1..];
        }
    }
    Ok(out)
}
