use super::*;
use alloc::collections::BTreeMap;
use posix_regex::{PosixRegex, PosixRegexBuilder};

// Nom long, alias court, arité (0, 1 ou 2 pour une valeur longue facultative).
pub const SPEC: &str = "\
binary .0 brief q0 changed-group-format .1 color .2 context C2 ed e0 exclude x1 exclude-from X1 expand-tabs t0 forward-ed f0 from-file .1 help .0 horizon-lines .1 ifdef D1 ignore-all-space w0 ignore-blank-lines B0 ignore-case i0 ignore-file-name-case .0 ignore-matching-lines I1 ignore-space-change b0 ignore-tab-expansion E0 ignore-trailing-space Z0 inhibit-hunk-merge .0 initial-tab T0 label L1 left-column .0 line-format .1 minimal d0 new-file N0 new-group-format .1 new-line-format .1 no-dereference .0 no-ignore-file-name-case .0 normal .0 old-group-format .1 old-line-format .1 paginate l0 palette .1 rcs n0 recursive r0 report-identical-files s0 sdiff-merge-assist .0 show-c-function p0 show-function-line F1 side-by-side y0 speed-large-files H0 starting-file S1 strip-trailing-cr .0 suppress-blank-empty .0 suppress-common-lines .0 tabsize .1 text a0 to-file .1 unchanged-group-format .1 unchanged-line-format .1 unidirectional-new-file P0 unified U2 version v0 width W1 -no-directory .0 -presume-output-tty .0";
fn specs() -> impl Iterator<Item = (&'static str, u8, u8)> {
    let mut i = SPEC.split_whitespace();
    core::iter::from_fn(move || {
        let k = i.next()?;
        let v = i.next()?.as_bytes();
        Some((k, v[0], v[1] - b'0'))
    })
}
pub fn txt(b: &[u8]) -> alloc::borrow::Cow<'_, str> {
    String::from_utf8_lossy(b)
}
pub struct Options<'a> {
    pub vals: BTreeMap<&'static str, Vec<&'a [u8]>>,
    pub files: Vec<&'a [u8]>,
    pub switches: Vec<&'a [u8]>,
    pub style: &'static str,
    pub context: usize,
    pub width: usize,
    pub tab: usize,
    pub ignore: Vec<PosixRegex<'static>>,
    pub functions: Vec<PosixRegex<'static>>,
    pub utf8: bool,
    pub doc: Option<&'static str>,
    pub tty: bool,
}
impl<'a> Options<'a> {
    pub fn has(&self, k: &str) -> bool {
        self.vals.contains_key(k)
    }
    pub fn val(&self, k: &str) -> Option<&'a [u8]> {
        self.vals.get(k)?.last().copied()
    }
    pub fn all(&self, k: &str) -> impl Iterator<Item = &'a [u8]> + '_ {
        self.vals.get(k).into_iter().flatten().copied()
    }
    fn put(&mut self, k: &'static str, v: &'a [u8], inv: &[u8]) -> Result<(), String> {
        let style = match k {
            "ifdef" | "line-format" => "custom",
            "sdiff-merge-assist" => "side-by-side",
            "ed" | "forward-ed" | "context" | "unified" | "normal" | "rcs" | "side-by-side" => k,
            _ if k.ends_with("-format") => "custom",
            _ => "",
        };
        if !style.is_empty() {
            if !self.style.is_empty() && style != self.style {
                return Err(hint("conflicting output style options", inv));
            }
            self.style = style;
        }
        let num = |what: &str, min: usize| -> Result<usize, String> {
            core::str::from_utf8(v)
                .ok()
                .and_then(|s| s.trim_start().parse::<usize>().ok())
                .filter(|n| *n >= min && *n <= isize::MAX as usize)
                .ok_or_else(|| hint(&format!("invalid {what} '{}'", txt(v)), inv))
        };
        match k {
            "help" | "version" => self.doc = Some(k),
            "context" | "unified" if !v.is_empty() => self.context = num("context length", 0)?,
            "width" | "tabsize" => {
                let n = num(if k == "width" { "width" } else { "tabsize" }, 1)?;
                let old = if k == "width" { self.width } else { self.tab };
                if self.has(k) && n != old {
                    return Err(format!("conflicting {k} options"));
                }
                if k == "width" {
                    self.width = n
                } else {
                    self.tab = n
                }
            }
            "horizon-lines" => {
                num("horizon length", 0)?;
            }
            "label" if self.all("label").count() == 2 => {
                return Err("too many file label options".into());
            }
            "ignore-matching-lines" | "show-function-line" | "show-c-function" => {
                let p = if k == "show-c-function" {
                    b"^[[:alpha:]$_]"
                } else {
                    v
                };
                if p.iter().rev().take_while(|&&c| c == 92).count() % 2 == 1 {
                    return Err(format!("'{}': Trailing backslash", txt(p)));
                }
                let re = PosixRegexBuilder::new(p)
                    .with_default_classes()
                    .compile()
                    .map_err(|_| format!("'{}': Invalid regular expression", txt(p)))?;
                if k == "ignore-matching-lines" {
                    self.ignore.push(re)
                } else {
                    self.functions.push(re)
                }
            }
            "color" if !matches!(v, b"auto" | b"always" | b"never") => {
                return Err(hint(&format!("invalid color '{}'", txt(v)), inv));
            }
            "ignore-file-name-case" => {
                self.vals.remove("no-ignore-file-name-case");
            }
            "no-ignore-file-name-case" => {
                self.vals.remove("ignore-file-name-case");
            }
            _ => {}
        }
        if matches!(k, "from-file" | "to-file" | "starting-file" | "ifdef")
            || k.ends_with("-format")
        {
            if self.val(k).is_some_and(|old| old != v) {
                let name = match k {
                    "starting-file" => "-S".into(),
                    "ifdef" => "-D".into(),
                    _ => format!("--{k}"),
                };
                let value = if k == "ifdef" {
                    format!("#ifndef {}\\n%<#endif /* ! {} */\\n", txt(v), txt(v))
                } else {
                    txt(v).into_owned()
                };
                return Err(hint(
                    &format!("conflicting {name} option value '{value}'"),
                    inv,
                ));
            }
        }
        self.vals.entry(k).or_default().push(v);
        Ok(())
    }
}
fn hint(msg: &str, name: &[u8]) -> String {
    format!(
        "{msg}\ndiff: Try '{} --help' for more information.",
        txt(name)
    )
}
pub fn parse<'a>(args: &[&'a [u8]], inv: &[u8], posix: bool) -> Result<Options<'a>, String> {
    let mut o = Options {
        vals: BTreeMap::new(),
        files: vec![],
        switches: vec![],
        style: "",
        context: 3,
        width: 130,
        tab: 8,
        functions: vec![],
        ignore: vec![],
        doc: None,
        tty: false,
        utf8: false,
    };
    let mut end = false;
    let mut it = args.iter().copied();
    while let Some(arg) = it.next() {
        if end || arg == b"-" || !arg.starts_with(b"-") {
            o.files.push(arg);
            end |= posix;
            continue;
        }
        o.switches.push(arg);
        if arg == b"--" {
            end = true;
            continue;
        }
        if let Some(rest) = arg.strip_prefix(b"--") {
            let mut kv = rest.splitn(2, |b| *b == b'=');
            let name = kv.next().unwrap();
            let inline = kv.next();
            let mut choices = specs().filter(|(k, _, _)| k.as_bytes().starts_with(name));
            let entry = specs()
                .find(|(k, _, _)| k.as_bytes() == name)
                .or_else(|| choices.next());
            let Some((k, _, arity)) = entry else {
                return Err(hint(&format!("unrecognized option '{}'", txt(arg)), inv));
            };
            if k.as_bytes() != name && choices.next().is_some() {
                let list = specs()
                    .filter(|(k, _, _)| k.as_bytes().starts_with(name))
                    .map(|(k, _, _)| format!("'--{k}'"))
                    .collect::<Vec<_>>()
                    .join(" ");
                return Err(hint(
                    &format!("option '{}' is ambiguous; possibilities: {list}", txt(arg)),
                    inv,
                ));
            }
            if arity == 0 && inline.is_some() {
                return Err(hint(
                    &format!("option '--{k}' doesn't allow an argument"),
                    inv,
                ));
            }
            let val = if arity == 1 && inline.is_none() {
                let x = it
                    .next()
                    .ok_or_else(|| hint(&format!("option '--{k}' requires an argument"), inv))?;
                o.switches.push(x);
                x
            } else {
                inline.unwrap_or(b"")
            };
            o.put(
                k,
                if k == "color" && inline.is_none() {
                    b"auto"
                } else {
                    val
                },
                inv,
            )?;
        } else {
            let mut j = 1;
            while j < arg.len() {
                let c = arg[j];
                j += 1;
                if c.is_ascii_digit() {
                    let s = j - 1;
                    while j < arg.len() && arg[j].is_ascii_digit() {
                        j += 1
                    }
                    o.context = txt(&arg[s..j]).parse().unwrap_or(usize::MAX / 4);
                    continue;
                }
                if c == b'h' {
                    continue;
                }
                if c == b'c' || c == b'u' {
                    o.put(if c == b'c' { "context" } else { "unified" }, b"", inv)?;
                    continue;
                }
                let Some((k, _, arity)) = specs().find(|(_, s, _)| *s == c && c != b'.') else {
                    return Err(hint(&format!("invalid option -- '{}'", char::from(c)), inv));
                };
                let val = if arity == 0 {
                    b"".as_slice()
                } else if j < arg.len() {
                    let v = &arg[j..];
                    j = arg.len();
                    v
                } else {
                    let v = it.next().ok_or_else(|| {
                        hint(
                            &format!("option requires an argument -- '{}'", char::from(c)),
                            inv,
                        )
                    })?;
                    o.switches.push(v);
                    v
                };
                o.put(k, val, inv)?;
                if o.doc.is_some() {
                    break;
                }
            }
        }
        if o.doc.is_some() {
            return Ok(o);
        }
    }
    if o.style.is_empty() {
        o.style = if o.has("show-c-function") {
            "context"
        } else {
            "normal"
        }
    }
    if o.has("from-file") && o.has("to-file") {
        return Err("--from-file and --to-file both specified".into());
    }
    if !o.has("from-file") && !o.has("to-file") && o.files.len() != 2 {
        let m = if o.files.len() < 2 {
            format!(
                "missing operand after '{}'",
                txt(args.last().copied().unwrap_or(inv))
            )
        } else {
            format!("extra operand '{}'", txt(o.files[2]))
        };
        return Err(hint(&m, inv));
    }
    Ok(o)
}
