use super::*;
use alloc::collections::BTreeMap;
fn join(p: &[u8], name: &[u8]) -> Vec<u8> {
    if p.ends_with(b"/") {
        [p, name].concat()
    } else {
        [p, b"/", name].concat()
    }
}
fn message(a: &[u8], mid: &[u8], b: &[u8], end: &[u8]) -> Vec<u8> {
    [a, mid, b, end].concat()
}
fn quoted(s: &[u8]) -> Vec<u8> {
    if !s.is_empty()
        && s.iter()
            .all(|c| c.is_ascii_alphanumeric() || b"%+,-./:@_".contains(c))
    {
        return s.to_vec();
    }
    let mut v = vec![b'\''];
    for &c in s {
        if c == b'\'' {
            v.extend(b"'\\''")
        } else {
            v.push(c)
        }
    }
    v.push(b'\'');
    v
}
fn display_name(s: &[u8]) -> Vec<u8> {
    if s.iter()
        .all(|c| *c > 32 && *c != 127 && *c != b'"' && *c != 92)
    {
        return s.to_vec();
    }
    let mut out = vec![b'"'];
    for &c in s {
        match c {
            b'\t' => out.extend(b"\\t"),
            b'\n' => out.extend(b"\\n"),
            b'\r' => out.extend(b"\\r"),
            b'"' | b'\\' => {
                out.push(92);
                out.push(c)
            }
            0..=31 | 127 => out.extend(format!("\\{c:03o}").bytes()),
            _ => out.push(c),
        }
    }
    out.push(b'"');
    out
}
fn title(o: &Options, names: [&[u8]; 2]) -> Vec<u8> {
    let mut s = b"diff".to_vec();
    for a in &o.switches {
        s.push(b' ');
        s.extend(quoted(a))
    }
    for a in names {
        s.push(b' ');
        s.extend(display_name(a))
    }
    s
}
fn strip_cr(v: &mut Vec<u8>) {
    let mut i = 0;
    for j in 0..v.len() {
        if v[j] == b'\r' && v.get(j + 1) == Some(&b'\n') {
            continue;
        }
        v[i] = v[j];
        i += 1
    }
    v.truncate(i)
}
fn file(
    p: &mut impl Platform,
    o: &Options,
    paths: [&[u8]; 2],
    m: [Option<&Meta>; 2],
    nested: bool,
) -> Result<u8, String> {
    let labels: Vec<_> = o.all("label").collect();
    let names = [0, 1].map(|i| labels.get(i).copied().unwrap_or(paths[i]));
    let identical = paths[0] == paths[1]
        || m[0]
            .and_then(|m| m.id)
            .is_some_and(|id| Some(id) == m[1].and_then(|m| m.id));
    let need_common = matches!(o.style, "custom" | "side-by-side") && !o.has("brief");
    let mut rc = 0;
    let mut out = vec![];
    if !identical || need_common {
        let mut a = if m[0].is_some() {
            p.read(paths[0])?
        } else {
            vec![]
        };
        let mut b = if identical {
            a.clone()
        } else if m[1].is_some() {
            p.read(paths[1])?
        } else {
            vec![]
        };
        if o.has("strip-trailing-cr") {
            strip_cr(&mut a);
            strip_cr(&mut b)
        }
        let binary = !o.has("text") && [&a, &b].iter().any(|s| s[..s.len().min(4096)].contains(&0));
        if a != b || need_common {
            if binary
                || o.has("brief")
                    && o.ignore.is_empty()
                    && ![
                        "ignore-case",
                        "ignore-blank-lines",
                        "ignore-all-space",
                        "ignore-space-change",
                        "ignore-trailing-space",
                        "ignore-tab-expansion",
                    ]
                    .iter()
                    .any(|k| o.has(k))
            {
                rc = u8::from(a != b);
                if rc == 1 && !o.has("brief") {
                    out.extend(b"Binary files ");
                    out.extend(message(names[0], b" and ", names[1], b" differ\n"))
                }
            } else {
                let t = engine::Text::new(&a, &b, o);
                rc = u8::from(t.changes.iter().any(|h| !t.ignored(h, o)));
                if !o.has("brief") {
                    let headers = [0, 1].map(|i| {
                        if labels.len() > i {
                            return names[i].to_vec();
                        }
                        let stamp = m[i]
                            .map(|m| m.stamp.as_slice())
                            .filter(|s| !s.is_empty())
                            .unwrap_or(if o.style == "context" {
                                b"Thu Jan  1 00:00:00 1970"
                            } else {
                                b"1970-01-01 00:00:00.000000000 +0000"
                            });
                        [display_name(names[i]).as_slice(), b"\t", stamp].concat()
                    });
                    render::render(&t, o, [&headers[0], &headers[1]], &mut out);
                    if matches!(o.style, "ed" | "forward-ed") {
                        for (i, s) in [&a, &b].iter().enumerate() {
                            if !s.is_empty() && !s.ends_with(b"\n") {
                                p.emit(
                                    true,
                                    &[
                                        b"diff: ".as_slice(),
                                        names[i],
                                        b": No newline at end of file\n\n",
                                    ]
                                    .concat(),
                                )?;
                                rc = 2
                            }
                        }
                    }
                }
            }
        }
    }
    if !out.is_empty() {
        let title = title(o, names);
        if o.has("paginate") {
            out = p.paginate(&title, &out)?
        } else if nested && !out.starts_with(b"Binary files ") {
            p.emit(false, &title)?;
            p.emit(false, b"\n")?
        }
        p.emit(false, &out)?
    }
    if rc == 1 && o.has("brief") || rc == 0 && o.has("report-identical-files") {
        let end = if rc == 0 {
            b" are identical\n".as_slice()
        } else {
            b" differ\n"
        };
        p.emit(
            false,
            &[b"Files ", names[0], b" and ", names[1], end].concat(),
        )?
    }
    Ok(rc)
}

fn glob(pattern: &[u8], text: &[u8]) -> bool {
    // Un seul point de reprise suffit pour le dernier astérisque rencontré.
    let (mut i, mut j) = (0, 0);
    let mut star = None;
    while j < text.len() {
        if pattern.get(i) == Some(&b'*') {
            i += 1;
            star = Some((i, j));
            continue;
        }
        let next = match pattern.get(i) {
            Some(b'?') => Some(i + 1),
            Some(b'\\') if pattern.get(i + 1) == text.get(j) => Some(i + 2),
            Some(b'[') => {
                let mut at = i + 1;
                let neg = matches!(pattern.get(at), Some(b'!' | b'^'));
                at += usize::from(neg);
                let mut ok = false;
                while at < pattern.len() && (pattern[at] != b']' || at == i + 1 + usize::from(neg))
                {
                    let lo = pattern[at];
                    at += 1;
                    if pattern.get(at) == Some(&b'-')
                        && pattern.get(at + 1).is_some_and(|&b| b != b']')
                    {
                        ok |= (lo..=pattern[at + 1]).contains(&text[j]);
                        at += 2
                    } else {
                        ok |= lo == text[j]
                    }
                }
                if pattern.get(at) == Some(&b']') {
                    (ok != neg).then_some(at + 1)
                } else {
                    (text[j] == b'[').then_some(i + 1)
                }
            }
            Some(c) if *c == text[j] => Some(i + 1),
            _ => None,
        };
        if let Some(end) = next {
            i = end;
            j += 1
        } else if let Some((p, t)) = star.filter(|(_, t)| *t < text.len()) {
            i = p;
            j = t + 1;
            star = Some((p, j))
        } else {
            return false;
        }
    }
    pattern[i..].iter().all(|&b| b == b'*')
}
struct Walk<'a, 'b, P> {
    p: &'a mut P,
    o: &'a Options<'b>,
    exclude: Vec<Vec<u8>>,
    parents: Vec<[Option<(u64, u64)>; 2]>,
}
impl<P: Platform> Walk<'_, '_, P> {
    fn pair(&mut self, names: [&[u8]; 2], nested: bool, absent: [bool; 2]) -> Result<u8, String> {
        let o = self.o;
        let mut m = [None, None];
        let mut bad = false;
        for i in 0..2 {
            if absent[i] {
                continue;
            }
            match self.p.stat(names[i], o) {
                Ok(s) => m[i] = s,
                Err(e) => {
                    self.p.emit(true, format!("diff: {e}\n").as_bytes())?;
                    bad = true
                }
            }
        }
        if bad {
            return Ok(2);
        }
        let missing = [m[0].is_none(), m[1].is_none()];
        for i in 0..2 {
            if missing[i]
                && (missing[1 - i]
                    || !(o.has("new-file") || i == 0 && o.has("unidirectional-new-file")))
            {
                self.p.emit(
                    true,
                    &[
                        b"diff: ".as_slice(),
                        names[i],
                        b": No such file or directory\n",
                    ]
                    .concat(),
                )?;
                bad = true
            }
        }
        if bad {
            return Ok(2);
        }
        let kinds = [0, 1].map(|i| m[i].as_ref().or(m[1 - i].as_ref()).unwrap().kind);
        if kinds.contains(&Kind::Dir) {
            if kinds[0] != kinds[1] && o.has("-no-directory") {
                return Err(format!(
                    "{}: Is a directory",
                    txt(names[usize::from(kinds[1] == Kind::Dir)])
                ));
            }
            if kinds[0] != kinds[1] {
                let d = usize::from(kinds[1] == Kind::Dir);
                if !nested {
                    if names[1 - d] == b"-" {
                        return Err("cannot compare '-' to a directory".into());
                    }
                    let base = names[1 - d]
                        .rsplit(|b| *b == b'/')
                        .next()
                        .unwrap_or_default();
                    let name = if names[d] == b"-" {
                        self.p.stdin_child(base)?
                    } else {
                        join(names[d], base)
                    };
                    let mut n = names;
                    n[d] = &name;
                    // Si l'enfant est encore un répertoire, signaler la différence de type.
                    return self.pair(n, false, [false; 2]);
                }
                let kind = |i: usize| {
                    if kinds[i] == Kind::Dir {
                        "a directory"
                    } else {
                        "a regular file"
                    }
                };
                self.p.emit(
                    false,
                    format!(
                        "File {} is {} while file {} is {}\n",
                        txt(names[0]),
                        kind(0),
                        txt(names[1]),
                        kind(1)
                    )
                    .as_bytes(),
                )?;
                return Ok(1);
            }
            if o.style == "custom" {
                return Err("-D option not supported with directories".into());
            }
            if nested && !o.has("recursive") {
                self.p.emit(
                    false,
                    &[
                        b"Common subdirectories: ",
                        names[0],
                        b" and ",
                        names[1],
                        b"\n",
                    ]
                    .concat(),
                )?;
                return Ok(0);
            }
            let ids = [0, 1].map(|i| m[i].as_ref().and_then(|v| v.id));
            for i in 0..2 {
                if ids[i].is_some() && self.parents.iter().any(|a| a[i] == ids[i]) {
                    return Err(format!("{}: recursive directory loop", txt(names[i])));
                }
            }
            self.parents.push(ids);
            let result = self.directory(names, missing);
            self.parents.pop();
            return result;
        }
        if kinds.contains(&Kind::Link) {
            if kinds[0] != kinds[1] {
                self.p.emit(
                    false,
                    format!(
                        "File {} is {} while file {} is {}\n",
                        txt(names[0]),
                        if kinds[0] == Kind::Link {
                            "a symbolic link"
                        } else {
                            "a regular file"
                        },
                        txt(names[1]),
                        if kinds[1] == Kind::Link {
                            "a symbolic link"
                        } else {
                            "a regular file"
                        }
                    )
                    .as_bytes(),
                )?;
                return Ok(1);
            }
            let a = self.p.link(names[0])?;
            let b = self.p.link(names[1])?;
            if a != b {
                self.p.emit(
                    false,
                    format!(
                        "Symbolic links '{}' -> '{}' and '{}' -> '{}' differ\n",
                        txt(names[0]),
                        txt(&a),
                        txt(names[1]),
                        txt(&b)
                    )
                    .as_bytes(),
                )?
            } else if o.has("report-identical-files") {
                self.p.emit(
                    false,
                    &[
                        b"Files ".as_slice(),
                        names[0],
                        b" and ",
                        names[1],
                        b" are identical\n",
                    ]
                    .concat(),
                )?
            }
            return Ok(u8::from(a != b));
        }
        file(self.p, o, names, [m[0].as_ref(), m[1].as_ref()], nested)
    }
    fn directory(&mut self, paths: [&[u8]; 2], missing: [bool; 2]) -> Result<u8, String> {
        let o = self.o;
        let mut names: BTreeMap<Vec<u8>, [Option<Vec<u8>>; 2]> = BTreeMap::new();
        for i in 0..2 {
            if missing[i] {
                continue;
            }
            for name in self.p.list(paths[i])? {
                if self.exclude.iter().any(|p| glob(p, &name))
                    || self.parents.len() == 1
                        && o.val("starting-file").is_some_and(|s| s > name.as_slice())
                {
                    continue;
                }
                let key = if o.has("ignore-file-name-case") {
                    name.to_ascii_lowercase()
                } else {
                    name.clone()
                };
                names.entry(key).or_default()[i] = Some(name)
            }
        }
        let mut rc = 0;
        for (_, pair) in names {
            let names = [0, 1].map(|i| {
                pair[i]
                    .as_ref()
                    .or(pair[1 - i].as_ref())
                    .unwrap()
                    .as_slice()
            });
            if let Some(i) = pair.iter().position(Option::is_none)
                && !(o.has("new-file") || i == 0 && o.has("unidirectional-new-file"))
            {
                self.p.emit(
                    false,
                    &[b"Only in ", paths[1 - i], b": ", names[1 - i], b"\n"].concat(),
                )?;
                rc = rc.max(1);
                continue;
            }
            let a = join(paths[0], names[0]);
            let b = join(paths[1], names[1]);
            match self.pair([&a, &b], true, [pair[0].is_none(), pair[1].is_none()]) {
                Ok(r) => rc = rc.max(r),
                Err(e) => {
                    self.p.emit(true, format!("diff: {e}\n").as_bytes())?;
                    rc = 2
                }
            }
        }
        Ok(rc)
    }
}
pub fn run(p: &mut impl Platform, o: &Options) -> Result<u8, String> {
    let mut exclude: Vec<_> = o.all("exclude").map(<[u8]>::to_vec).collect();
    for file in o.all("exclude-from") {
        for s in p.read(file)?.split_inclusive(|c| *c == b'\n') {
            exclude.push(engine::body(s).to_vec())
        }
    }
    let mut walk = Walk {
        p,
        o,
        exclude,
        parents: vec![],
    };
    let mut rc = 0;
    let pairs: Vec<_> = if let Some(a) = o.val("from-file") {
        o.files.iter().map(|&b| [a, b]).collect()
    } else if let Some(b) = o.val("to-file") {
        o.files.iter().map(|&a| [a, b]).collect()
    } else {
        vec![[o.files[0], o.files[1]]]
    };
    for pair in pairs {
        match walk.pair(pair, false, [false; 2]) {
            Ok(code) => rc = rc.max(code),
            Err(e) => {
                walk.p.emit(true, format!("diff: {e}\n").as_bytes())?;
                rc = 2
            }
        }
    }
    Ok(rc)
}
