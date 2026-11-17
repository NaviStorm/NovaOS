use super::*;
use core::ops::Range;
use engine::{Change, Text, body};
use unicode_width::UnicodeWidthChar;

fn range(r: &Range<usize>, sep: char, unified: bool) -> String {
    if unified && r.len() != 1 {
        return format!("{},{}", r.start + usize::from(!r.is_empty()), r.len());
    }
    match r.len() {
        0 => format!("{}", r.start),
        1 => format!("{}", r.end),
        _ => format!("{}{sep}{}", r.start + 1, r.end),
    }
}
fn append(out: &mut Vec<u8>, s: &str) {
    out.extend(s.as_bytes())
}
fn paint(o: &Options, out: &mut Vec<u8>, key: &str, s: &[u8]) {
    let enable = match o.val("color") {
        Some(b"always" | b"yes" | b"force") => true,
        Some(b"" | b"auto" | b"tty" | b"if-tty") => o.tty,
        _ => false,
    };
    let default = match key {
        "de" => "31",
        "ad" => "32",
        "ln" => "36",
        "hd" => "1",
        _ => "",
    };
    let mut color = default.as_bytes();
    if let Some(p) = o.val("palette") {
        for opt in p.split(|c| *c == b':') {
            if let Some(v) = opt
                .strip_prefix(key.as_bytes())
                .and_then(|v| v.strip_prefix(b"="))
            {
                color = v
            }
        }
    }
    if enable && !color.is_empty() {
        out.extend(b"\x1b[");
        out.extend(color);
        out.push(b'm');
        out.extend(s);
        out.extend(b"\x1b[0m")
    } else {
        out.extend(s)
    }
}
fn header(o: &Options, out: &mut Vec<u8>, key: &str, bytes: &[u8]) {
    paint(o, out, key, bytes);
    out.push(b'\n')
}
pub fn column(line: &[u8], size: usize, limit: usize, expand: bool) -> (Vec<u8>, usize) {
    let mut buf = Vec::new();
    let mut col = 0usize;
    let mut i = 0;
    while i < line.len() {
        let c = line[i];
        if c == b'\t' {
            let n = size - col % size;
            if n > limit.saturating_sub(col) {
                break;
            }
            if expand {
                buf.resize(buf.len() + n, b' ')
            } else {
                buf.push(c)
            }
            col += n;
            i += 1;
            continue;
        }
        let len = match c {
            0xc2..=0xdf => 2,
            0xe0..=0xef => 3,
            0xf0..=0xf4 => 4,
            _ => 1,
        };
        let ch = line
            .get(i..i + len)
            .and_then(|s| core::str::from_utf8(s).ok())
            .and_then(|s| s.chars().next());
        if c == 8 && col == 0 && expand {
            i += 1;
            continue;
        }
        let width = ch.and_then(UnicodeWidthChar::width).unwrap_or(1);
        if col.saturating_add(width) > limit {
            break;
        }
        let used = if ch.is_some() { len } else { 1 };
        buf.extend(&line[i..i + used]);
        i += used;
        col = match c {
            8 => col.saturating_sub(1),
            b'\r' => 0,
            _ => col + width,
        };
    }
    (buf, col)
}
fn line(o: &Options, out: &mut Vec<u8>, flag: u8, s: &[u8], unified: bool, color: &str) {
    let mut b = vec![];
    let empty = body(s).is_empty() && o.has("suppress-blank-empty");
    if unified {
        if flag != b' ' || !empty && !o.has("initial-tab") {
            b.push(flag)
        }
        if o.has("initial-tab") && !empty {
            b.push(b'\t')
        }
    } else {
        if flag != b' ' || !empty {
            b.push(flag)
        }
        if !empty {
            b.push(if o.has("initial-tab") { b'\t' } else { b' ' })
        }
    }
    if o.has("expand-tabs") {
        b.extend(column(body(s), o.tab, usize::MAX, true).0)
    } else {
        b.extend(body(s))
    }
    header(o, out, color, &b);
    if !s.ends_with(b"\n") {
        out.extend(b"\\ No newline at end of file\n")
    }
}
fn normal(t: &Text, o: &Options, out: &mut Vec<u8>, h: &Change) {
    let absolute = |r: &Range<usize>| (r.start + t.first_line)..(r.end + t.first_line);
    let op = if h.a.is_empty() {
        'a'
    } else if h.b.is_empty() {
        'd'
    } else {
        'c'
    };
    header(
        o,
        out,
        "ln",
        format!(
            "{}{op}{}",
            range(&absolute(&h.a), ',', false),
            range(&absolute(&h.b), ',', false)
        )
        .as_bytes(),
    );
    for s in &t.rows[0][h.a.clone()] {
        line(o, out, b'<', s, false, "de")
    }
    if op == 'c' {
        out.extend(b"---\n")
    }
    for s in &t.rows[1][h.b.clone()] {
        line(o, out, b'>', s, false, "ad")
    }
}
fn context(t: &Text, o: &Options, names: [&[u8]; 2], out: &mut Vec<u8>) {
    let u = o.style == "unified";
    let mut i = 0;
    let mut first = true;
    while i < t.changes.len() {
        let start = i;
        i += 1;
        while i < t.changes.len()
            && t.changes[i].a.start - t.changes[i - 1].a.end <= o.context.saturating_mul(2)
        {
            i += 1
        }
        let hs = &t.changes[start..i];
        if hs.iter().all(|h| t.ignored(h, o)) {
            continue;
        }
        if first {
            for (side, name) in names.iter().enumerate() {
                let mark = match (u, side) {
                    (true, 0) | (false, 1) => b"--- ",
                    (true, _) => b"+++ ",
                    _ => b"*** ",
                };
                header(o, out, "hd", &[mark.as_slice(), name].concat())
            }
            first = false
        }
        let h = &hs[0];
        let last = hs.last().unwrap();
        let spans = [
            h.a.start.saturating_sub(o.context)
                ..last.a.end.saturating_add(o.context).min(t.rows[0].len()),
            h.b.start.saturating_sub(o.context)
                ..last.b.end.saturating_add(o.context).min(t.rows[1].len()),
        ];
        let function = t.rows[0][..spans[0].start]
            .iter()
            .rev()
            .find(|s| engine::matches(&o.functions, s))
            .map(|s| {
                let s = body(s).trim_ascii_start();
                &s[..s.len().min(40)]
            });
        if u {
            let title = format!(
                "@@ -{} +{} @@",
                range(&spans[0], ',', true),
                range(&spans[1], ',', true)
            );
            let mut title = title.into_bytes();
            if let Some(f) = function {
                title.push(b' ');
                title.extend(f)
            }
            header(o, out, "ln", &title);
            let mut a = spans[0].start;
            for h in hs {
                for s in &t.rows[0][a..h.a.start] {
                    line(o, out, b' ', s, true, "")
                }
                for s in &t.rows[0][h.a.clone()] {
                    line(o, out, b'-', s, true, "de")
                }
                for s in &t.rows[1][h.b.clone()] {
                    line(o, out, b'+', s, true, "ad")
                }
                a = h.a.end;
            }
            for s in &t.rows[0][a..spans[0].end] {
                line(o, out, b' ', s, true, "")
            }
        } else {
            let mut title = b"***************".to_vec();
            if let Some(f) = function {
                title.push(b' ');
                title.extend(f)
            }
            header(o, out, "", &title);
            for (side, span) in spans.iter().enumerate() {
                let (mark, end, color) = if side == 0 {
                    ("***", "****", "de")
                } else {
                    ("---", "----", "ad")
                };
                header(
                    o,
                    out,
                    "ln",
                    format!("{mark} {} {end}", range(span, ',', false)).as_bytes(),
                );
                if hs.iter().all(|h| {
                    if side == 0 {
                        h.a.is_empty()
                    } else {
                        h.b.is_empty()
                    }
                }) {
                    continue;
                }
                let mut pos = span.start;
                for h in hs {
                    let r = if side == 0 { &h.a } else { &h.b };
                    for s in &t.rows[side][pos..r.start] {
                        line(o, out, b' ', s, false, "")
                    }
                    let symbol = if h.a.is_empty() {
                        b'+'
                    } else if h.b.is_empty() {
                        b'-'
                    } else {
                        b'!'
                    };
                    for s in &t.rows[side][r.clone()] {
                        line(o, out, symbol, s, false, color)
                    }
                    pos = r.end
                }
                for s in &t.rows[side][pos..span.end] {
                    line(o, out, b' ', s, false, "")
                }
            }
        }
    }
}
fn script(t: &Text, o: &Options, out: &mut Vec<u8>, h: &Change) {
    if o.style == "rcs" {
        if !h.a.is_empty() {
            append(out, &format!("d{} {}\n", h.a.start + 1, h.a.len()))
        }
        if !h.b.is_empty() {
            append(out, &format!("a{} {}\n", h.a.end, h.b.len()));
            for s in &t.rows[1][h.b.clone()] {
                out.extend(*s)
            }
        }
        return;
    }
    let op = if h.a.is_empty() {
        'a'
    } else if h.b.is_empty() {
        'd'
    } else {
        'c'
    };
    let ed = o.style == "ed";
    let r = range(&h.a, if ed { ',' } else { ' ' }, false);
    append(
        out,
        &if ed {
            format!("{r}{op}\n")
        } else {
            format!("{op}{r}\n")
        },
    );
    if op == 'd' {
        return;
    }
    let mut open = true;
    for n in h.b.clone() {
        let s = body(t.rows[1][n]);
        if !open {
            out.extend(b"a\n");
            open = true
        }
        if ed && s == b"." {
            out.extend(b"..\n.\ns/.//\n");
            open = false
        } else {
            out.extend(s);
            out.push(b'\n')
        }
    }
    if open {
        out.extend(b".\n")
    }
}
fn pad(out: &mut Vec<u8>, mut col: usize, to: usize, o: &Options) {
    while col < to {
        let n = o.tab - col % o.tab;
        if !o.has("expand-tabs") && n <= to - col {
            out.push(b'\t');
            col += n
        } else {
            out.push(b' ');
            col += 1
        }
    }
}
fn side(t: &Text, o: &Options, out: &mut Vec<u8>) {
    let tab = if o.has("expand-tabs") { 1 } else { o.tab };
    let off = (o.width / 2 + (tab + 3) / 2 + ((o.width & 1) & ((tab + 3) & 1))) / tab * tab;
    let half = off.saturating_sub(3).min(o.width.saturating_sub(off));
    let off = if half == 0 { o.width } else { off };
    let mut group = |a: Range<usize>, b: Range<usize>, same: bool| {
        if same && o.has("suppress-common-lines") {
            return;
        }
        if o.has("sdiff-merge-assist") {
            append(
                out,
                &format!("{}{},{}\n", if same { 'i' } else { 'c' }, a.len(), b.len()),
            )
        }
        for n in 0..a.len().max(b.len()) {
            let left = t.rows[0].get(a.start + n).copied().filter(|_| n < a.len());
            let right = t.rows[1].get(b.start + n).copied().filter(|_| n < b.len());
            let (s, col) = column(
                body(left.unwrap_or_default()),
                o.tab,
                half,
                o.has("expand-tabs"),
            );
            out.extend(s);
            let mut col = col;
            let mark = if same {
                if o.has("left-column") { b'(' } else { b' ' }
            } else {
                match (left, right) {
                    (None, _) => b'>',
                    (_, None) => b'<',
                    (Some(l), Some(r)) if l.ends_with(b"\n") != r.ends_with(b"\n") => {
                        if l.ends_with(b"\n") { b'/' } else { b'\\' }
                    }
                    _ => b'|',
                }
            };
            if mark != b' ' {
                let pos = (half + off - 1) / 2;
                pad(out, col, pos, o);
                out.push(mark);
                col = pos + 1
            }
            if mark != b'('
                && let Some(r) = right
            {
                let (s, _) = column(body(r), o.tab, half, o.has("expand-tabs"));
                if !s.is_empty() {
                    pad(out, col, off, o);
                    out.extend(s)
                }
            }
            if [left, right]
                .into_iter()
                .flatten()
                .any(|s| s.ends_with(b"\n"))
            {
                out.push(b'\n')
            }
        }
    };
    let (mut a, mut b) = (0, 0);
    for h in &t.changes {
        if a < h.a.start {
            group(a..h.a.start, b..h.b.start, true)
        }
        group(h.a.clone(), h.b.clone(), t.ignored(h, o));
        a = h.a.end;
        b = h.b.end
    }
    if a < t.rows[0].len() {
        group(a..t.rows[0].len(), b..t.rows[1].len(), true)
    }
}
pub fn render(t: &Text, o: &Options, names: [&[u8]; 2], out: &mut Vec<u8>) {
    match o.style {
        "unified" | "context" => context(t, o, names, out),
        "custom" => custom::render(t, o, out),
        "side-by-side" => side(t, o, out),
        "ed" => {
            for h in t.changes.iter().rev().filter(|h| !t.ignored(h, o)) {
                script(t, o, out, h)
            }
        }
        _ => {
            for h in t.changes.iter().filter(|h| !t.ignored(h, o)) {
                if o.style == "normal" {
                    normal(t, o, out, h)
                } else {
                    script(t, o, out, h)
                }
            }
        }
    }
}
