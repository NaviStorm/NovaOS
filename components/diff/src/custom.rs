use super::*;
use core::ops::Range;
use engine::{Text, body};

struct Group<'a, 'b> {
    text: &'a Text<'b>,
    r: [Range<usize>; 2],
    o: &'a Options<'a>,
}
fn number(k: u8, r: &[Range<usize>; 2]) -> Option<usize> {
    let span = &r[k.is_ascii_uppercase() as usize];
    Some(match k.to_ascii_lowercase() {
        b'f' => span.start + 1,
        b'l' => span.end,
        b'n' => span.len(),
        b'e' => span.start,
        b'm' => span.end + 1,
        _ => return None,
    })
}
fn field(fmt: &[u8], i: &mut usize, r: &[Range<usize>; 2]) -> Option<usize> {
    let c = *fmt.get(*i)?;
    if let Some(n) = number(c, r) {
        *i += 1;
        return Some(n);
    }
    let begin = *i;
    while fmt.get(*i).is_some_and(u8::is_ascii_digit) {
        *i += 1
    }
    core::str::from_utf8(&fmt[begin..*i]).ok()?.parse().ok()
}
impl Group<'_, '_> {
    fn format(&self, fmt: &[u8], line: Option<(&[u8], usize)>, out: &mut Vec<u8>) {
        let mut i = 0;
        while i < fmt.len() {
            if fmt[i] != b'%' || i + 1 == fmt.len() {
                out.push(fmt[i]);
                i += 1;
                continue;
            }
            let begin = i;
            i += 1;
            let c = fmt[i];
            i += 1;
            match c {
                b'%' => out.push(b'%'),
                b'L' | b'l' if line.is_some() => {
                    let (s, _) = line.unwrap();
                    if self.o.has("expand-tabs") {
                        out.extend(render::column(body(s), self.o.tab, usize::MAX, true).0);
                        if c == b'L' && s.ends_with(b"\n") {
                            out.push(b'\n')
                        }
                    } else {
                        out.extend(if c == b'L' { s } else { body(s) })
                    }
                }
                b'<' | b'>' | b'=' if line.is_none() => {
                    let side = usize::from(c == b'>');
                    let key = match c {
                        b'<' => "old-line-format",
                        b'>' => "new-line-format",
                        _ => "unchanged-line-format",
                    };
                    let fmt = self
                        .o
                        .val(key)
                        .or(self.o.val("line-format"))
                        .unwrap_or(b"%l\n");
                    for n in self.r[side].clone() {
                        self.format(fmt, Some((self.text.rows[side][n], n + 1)), out)
                    }
                }
                b'c' if fmt.get(i) == Some(&b'\'') => {
                    let start = i;
                    i += 1;
                    let v = if fmt.get(i) == Some(&b'\\') {
                        i += 1;
                        let at = i;
                        while i < at + 3 && fmt.get(i).is_some_and(|c| matches!(c, b'0'..=b'7')) {
                            i += 1
                        }
                        core::str::from_utf8(&fmt[at..i])
                            .ok()
                            .and_then(|s| u8::from_str_radix(s, 8).ok())
                    } else {
                        let v = fmt.get(i).copied();
                        i += usize::from(v.is_some());
                        v
                    };
                    if let Some(v) = v
                        && fmt.get(i) == Some(&b'\'')
                    {
                        out.push(v);
                        i += 1
                    } else {
                        out.extend(b"%c");
                        i = start
                    }
                }
                b'(' if line.is_none() => {
                    let start = i;
                    let a = field(fmt, &mut i, &self.r);
                    let eq = fmt.get(i) == Some(&b'=');
                    i += usize::from(eq);
                    let b = field(fmt, &mut i, &self.r);
                    if a.is_none() || b.is_none() || !eq || fmt.get(i) != Some(&b'?') {
                        out.extend(b"%(");
                        i = start;
                        continue;
                    }
                    i += 1;
                    let yes = i;
                    let mut colon = None;
                    let mut level = 0;
                    while i < fmt.len() {
                        match fmt[i] {
                            b'%' if fmt.get(i + 1) == Some(&b'(') => {
                                level += 1;
                                i += 1
                            }
                            b')' if level == 0 => break,
                            b')' => level -= 1,
                            b':' if level == 0 && colon.is_none() => colon = Some(i),
                            _ => {}
                        }
                        i += 1
                    }
                    if let Some(mid) = colon
                        && i < fmt.len()
                    {
                        self.format(
                            if a == b {
                                &fmt[yes..mid]
                            } else {
                                &fmt[mid + 1..i]
                            },
                            None,
                            out,
                        );
                        i += 1
                    } else {
                        out.extend(b"%(");
                        i = start
                    }
                }
                _ => {
                    i = begin + 1;
                    let left = fmt.get(i) == Some(&b'-');
                    i += usize::from(left);
                    let zero = fmt.get(i) == Some(&b'0');
                    let start = i;
                    while fmt.get(i).is_some_and(u8::is_ascii_digit) {
                        i += 1
                    }
                    let width = txt(&fmt[start..i])
                        .parse::<usize>()
                        .unwrap_or(0)
                        .min(1 << 20);
                    let mut prec = None;
                    if fmt.get(i) == Some(&b'.') {
                        i += 1;
                        let start = i;
                        while fmt.get(i).is_some_and(u8::is_ascii_digit) {
                            i += 1
                        }
                        prec = Some(
                            txt(&fmt[start..i])
                                .parse::<usize>()
                                .unwrap_or(0)
                                .min(1 << 20),
                        )
                    }
                    let radix = fmt.get(i).copied();
                    let tag = fmt.get(i + 1).copied().unwrap_or(0);
                    let n = if let Some((_, n)) = line {
                        (tag == b'n').then_some(n)
                    } else {
                        number(tag, &self.r)
                    };
                    if matches!(radix, Some(b'd' | b'o' | b'x' | b'X'))
                        && let Some(n) = n
                    {
                        let digits = match radix {
                            Some(b'o') => format!("{n:o}"),
                            Some(b'x') => format!("{n:x}"),
                            Some(b'X') => format!("{n:X}"),
                            _ => format!("{n}"),
                        };
                        let digits = if n == 0 && prec == Some(0) {
                            ""
                        } else {
                            &digits
                        };
                        let zeros = prec.unwrap_or(0).saturating_sub(digits.len());
                        let pad = width.saturating_sub(zeros + digits.len());
                        if !left {
                            out.resize(
                                out.len() + pad,
                                if zero && prec.is_none() { b'0' } else { b' ' },
                            )
                        }
                        out.resize(out.len() + zeros, b'0');
                        out.extend(digits.as_bytes());
                        if left {
                            out.resize(out.len() + pad, b' ')
                        }
                        i += 2
                    } else {
                        out.push(b'%');
                        i = begin + 1
                    }
                }
            }
        }
    }
}
pub fn render(text: &Text, o: &Options, out: &mut Vec<u8>) {
    let ifdef = o.val("ifdef").map(txt);
    let (mut a, mut b) = (0, 0);
    let mut group = |r: [Range<usize>; 2], kind: &str| {
        let g = Group { text, r, o };
        let default = match (kind, &ifdef) {
            ("unchanged", _) => "%=".into(),
            ("old", Some(k)) => format!("#ifndef {k}\n%<#endif /* ! {k} */\n"),
            ("new", Some(k)) => format!("#ifdef {k}\n%>#endif /* {k} */\n"),
            ("changed", Some(k)) => format!("#ifndef {k}\n%<#else /* {k} */\n%>#endif /* {k} */\n"),
            ("old", None) => "%<".into(),
            ("new", None) => "%>".into(),
            _ => "%<%>".into(),
        };
        if kind == "changed" && o.val("changed-group-format").is_none() && ifdef.is_none() {
            g.format(o.val("old-group-format").unwrap_or(b"%<"), None, out);
            g.format(o.val("new-group-format").unwrap_or(b"%>"), None, out)
        } else {
            let fmt = o
                .val(&format!("{kind}-group-format"))
                .or_else(|| {
                    matches!(kind, "old" | "new")
                        .then(|| o.val("changed-group-format"))
                        .flatten()
                })
                .unwrap_or(default.as_bytes());
            g.format(fmt, None, out)
        }
    };
    for h in &text.changes {
        if h.a.start > a {
            group([a..h.a.start, b..h.b.start], "unchanged")
        }
        group(
            [h.a.clone(), h.b.clone()],
            if text.ignored(h, o) {
                "unchanged"
            } else if h.a.is_empty() {
                "new"
            } else if h.b.is_empty() {
                "old"
            } else {
                "changed"
            },
        );
        a = h.a.end;
        b = h.b.end;
    }
    if a < text.rows[0].len() {
        group([a..text.rows[0].len(), b..text.rows[1].len()], "unchanged")
    }
}
