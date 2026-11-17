use crate::quoting::{Style, quote};
use crate::signals::Signals;
use alloc::{collections::VecDeque, vec::Vec};
pub type Word = Vec<u8>;
pub type Entries = Vec<Word>;
pub fn cat(parts: &[&[u8]]) -> Word {
    parts.concat()
}
pub fn quoted(w: &[u8]) -> Word {
    #[cfg(target_os = "none")]
    let utf8 = false;
    #[cfg(not(target_os = "none"))]
    let utf8 = crate::host::utf8();
    quote(w, Style::Locale, utf8)
}
pub struct Error {
    pub message: Word,
    pub usage: bool,
    pub getopt: bool,
    pub shebang: bool,
    pub code: u8,
}
impl Error {
    pub fn new(message: Word) -> Self {
        Self {
            message,
            usage: false,
            getopt: false,
            shebang: false,
            code: 125,
        }
    }
    fn option(message: Word) -> Self {
        Self {
            getopt: true,
            ..Self::usage(message)
        }
    }
    pub fn usage(message: Word) -> Self {
        Self {
            usage: true,
            ..Self::new(message)
        }
    }
}
pub fn pair(entry: &[u8]) -> Option<(&[u8], &[u8])> {
    let pos = entry.iter().position(|b| *b == b'=')?;
    Some((&entry[..pos], &entry[pos + 1..]))
}
pub fn value<'a>(entries: &'a Entries, key: &[u8]) -> Option<&'a [u8]> {
    entries
        .iter()
        .filter_map(|e| pair(e))
        .find(|(n, _)| *n == key)
        .map(|(_, v)| v)
}
#[derive(Default)]
pub struct Plan {
    pub empty: bool,
    pub zero: bool,
    pub debug: bool,
    pub doc: Option<bool>,
    pub file: Option<Word>,
    pub cwd: Option<Word>,
    pub argv0: Option<Word>,
    unset: Entries,
    assign: Entries,
    pub command: Entries,
    pub signals: Signals,
}
const OPTIONS: &[(&[u8], u8)] = &[
    (b"argv0", b'a'),
    (b"ignore-environment", b'i'),
    (b"env0-from", b'F'),
    (b"null", b'0'),
    (b"unset", b'u'),
    (b"chdir", b'C'),
    (b"default-signal", b'd'),
    (b"ignore-signal", b'g'),
    (b"block-signal", b'b'),
    (b"list-signal-handling", b'l'),
    (b"debug", b'v'),
    (b"split-string", b'S'),
    (b"help", b'h'),
    (b"version", b'V'),
];

fn ascii_quotes(text: &[u8]) -> Word {
    cat(&[b"'", text, b"'"])
}
fn long_option(token: &[u8]) -> Result<(&'static [u8], u8, Option<&[u8]>), Error> {
    let (prefix, arg) = pair(&token[2..]).map_or((&token[2..], None), |(k, v)| (k, Some(v)));
    let mut matching = OPTIONS
        .iter()
        .filter(|(n, _)| !prefix.is_empty() && n.starts_with(prefix));
    let &(name, id) = matching
        .next()
        .ok_or_else(|| Error::option(cat(&[b"unrecognized option ", &ascii_quotes(token)])))?;
    if prefix != name && matching.next().is_some() {
        let mut msg = cat(&[
            b"option ",
            &ascii_quotes(token),
            b" is ambiguous; possibilities:",
        ]);
        for &(n, _) in OPTIONS.iter().filter(|(n, _)| n.starts_with(prefix)) {
            msg.extend(cat(&[b" '--", n, b"'"]));
        }
        return Err(Error::option(msg));
    }
    Ok((name, id, arg))
}

pub fn parse(
    args: Entries,
    inherited: Option<&Entries>,
    trace: &mut impl FnMut(&[u8]),
) -> Result<Plan, Error> {
    let mut pending = VecDeque::from(args);
    let mut p = Plan::default();
    let mut splits = 0;
    while pending
        .front()
        .is_some_and(|w| w.starts_with(b"-") && w.len() > 1)
    {
        let token = pending.pop_front().unwrap();
        if token == b"--" {
            break;
        }
        let mut pos = 1;
        while pos < token.len() {
            let (id, mut arg, spelling) = if token.starts_with(b"--") {
                let (name, id, value) = long_option(&token)?;
                pos = token.len();
                (id, value.map(<[u8]>::to_vec), cat(&[b"--", name]))
            } else {
                let id = token[pos];
                pos += 1;
                if !b"aCiSuv0".contains(&id) {
                    if id.is_ascii_whitespace() || id == 11 {
                        return Err(Error {
                            shebang: true,
                            ..Error::usage(cat(&[b"invalid option -- ", &quoted(&[id])]))
                        });
                    }
                    return Err(Error::option(cat(&[
                        b"invalid option -- ",
                        &ascii_quotes(&[id]),
                    ])));
                }
                let value = if b"aCSu".contains(&id) && pos < token.len() {
                    let a = token[pos..].to_vec();
                    pos = token.len();
                    Some(a)
                } else {
                    None
                };
                (id, value, alloc::vec![b'-', id])
            };
            if b"aCFSu".contains(&id) && arg.is_none() {
                arg = Some(pending.pop_front().ok_or_else(|| {
                    Error::option(if spelling.starts_with(b"--") {
                        cat(&[
                            b"option ",
                            &ascii_quotes(&spelling),
                            b" requires an argument",
                        ])
                    } else {
                        cat(&[b"option requires an argument -- ", &ascii_quotes(&[id])])
                    })
                })?);
            } else if arg.is_some() && !b"aCFSudgb".contains(&id) {
                return Err(Error::option(cat(&[
                    b"option ",
                    &ascii_quotes(&spelling),
                    b" doesn't allow an argument",
                ])));
            }
            match id {
                b'i' => p.empty = true,
                b'0' => p.zero = true,
                b'v' => p.debug = true,
                b'l' => p.signals.list = true,
                b'a' => p.argv0 = arg,
                b'C' => p.cwd = arg,
                b'F' => p.file = arg,
                b'u' => p.unset.push(arg.unwrap()),
                b'd' | b'g' | b'b' => p.signals.option(id, arg.as_deref())?,
                b'h' | b'V' => {
                    p.doc = Some(id == b'h');
                    return Ok(p);
                }
                b'S' => {
                    splits += 1;
                    if splits > 1024 {
                        return Err(Error::new(b"split-string expansion limit".to_vec()));
                    }
                    let raw = arg.unwrap();
                    let words = crate::split::words(&raw, inherited, |line| {
                        if p.debug {
                            trace(&line)
                        }
                    })?;
                    if p.debug && !words.is_empty() {
                        trace(&cat(&[b"split -S:  ", &quoted(&raw), b"\n"]));
                        for (i, w) in words.iter().enumerate() {
                            trace(&cat(&[
                                if i == 0 {
                                    b" into:     "
                                } else {
                                    b"     &     "
                                },
                                &quoted(w),
                                b"\n",
                            ]));
                        }
                    }
                    for w in words.into_iter().rev() {
                        pending.push_front(w)
                    }
                }
                _ => unreachable!(),
            }
        }
    }
    if pending.front().is_some_and(|a| a == b"-") {
        pending.pop_front();
        p.empty = true;
    }
    while pending.front().is_some_and(|a| a.contains(&b'=')) {
        p.assign.push(pending.pop_front().unwrap());
    }
    p.command = pending.into();
    Ok(p)
}

impl Plan {
    pub fn validate(&self) -> Result<(), Error> {
        let msg = if !self.command.is_empty() && self.zero {
            b"cannot specify --null (-0) with command".as_slice()
        } else if self.command.is_empty() && self.cwd.is_some() {
            b"must specify command with --chdir (-C)"
        } else if self.command.is_empty() && self.argv0.is_some() {
            b"must specify command with --argv0 (-a)"
        } else {
            return Ok(());
        };
        Err(Error::usage(msg.to_vec()))
    }
    pub fn environment(
        &self,
        mut entries: Entries,
        read: impl FnOnce(&[u8]) -> Result<Word, Error>,
        trace: &mut impl FnMut(&[u8]),
    ) -> Result<Entries, Error> {
        let mut log = |label: &[u8], w: &[u8]| {
            if self.debug {
                trace(&cat(&[label, &quoted(w), b"\n"]))
            }
        };
        if self.empty {
            entries.clear();
        }
        let mut append = self.empty;
        if let Some(f) = &self.file {
            let bytes = read(f)?;
            append |= !bytes.is_empty();
            if !bytes.is_empty() && bytes.last() != Some(&0) {
                return Err(Error::new(cat(&[
                    &quote(f, Style::ShellEscape, false),
                    b": file must end with a NUL byte",
                ])));
            }
            for e in bytes
                .split_inclusive(|b| *b == 0)
                .map(|x| &x[..x.len() - 1])
            {
                if self.empty {
                    entries.push(e.to_vec())
                } else {
                    log(
                        if pair(e).is_some() {
                            b"setting:   "
                        } else {
                            b"adding:    "
                        },
                        e,
                    );
                    insert(&mut entries, e, true);
                }
            }
        }
        if !self.empty || self.file.is_some() {
            for k in &self.unset {
                log(b"unsetting: ", k);
                if k.is_empty() || k.contains(&b'=') {
                    return Err(Error::new(cat(&[
                        b"cannot unset ",
                        &quoted(k),
                        b": Invalid argument",
                    ])));
                }
                entries.retain(|e| pair(e).is_none_or(|(name, _)| name != k));
            }
        }
        for e in &self.assign {
            log(b"setting:   ", e);
            insert(&mut entries, e, append);
        }
        Ok(entries)
    }
}
fn insert(entries: &mut Entries, e: &[u8], append: bool) {
    if let Some((name, _)) = pair(e) {
        if let Some(slot) = entries
            .iter_mut()
            .find(|s| pair(s).is_some_and(|(n, _)| n == name))
        {
            slot.clear();
            slot.extend_from_slice(e);
            return;
        }
    }
    if append {
        entries.push(e.to_vec())
    } else {
        entries.insert(0, e.to_vec())
    }
}
pub fn display(e: &[u8], style: Style, utf8: bool) -> Word {
    if style == Style::Literal {
        return e.to_vec();
    }
    if let Some((n, v)) = pair(e) {
        let mut line = quote(n, style, utf8);
        line.push(b'=');
        if !v.is_empty() {
            line.extend(quote(v, style, utf8))
        }
        line
    } else {
        quote(e, style, utf8)
    }
}
