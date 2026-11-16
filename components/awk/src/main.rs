use std::env;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::process;

use awk_rs::{Interpreter, Lexer, Parser};

/// Decode AWK escape sequences in a command-line assignment value.
///
/// POSIX requires `-v x='a\tb'` to be processed exactly like a string literal,
/// so this mirrors the lexer's string escape handling.
fn unescape_assignment(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars().peekable();

    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }

        match chars.peek().copied() {
            Some('n') => {
                chars.next();
                out.push('\n');
            }
            Some('t') => {
                chars.next();
                out.push('\t');
            }
            Some('r') => {
                chars.next();
                out.push('\r');
            }
            Some('b') => {
                chars.next();
                out.push('\x08');
            }
            Some('f') => {
                chars.next();
                out.push('\x0C');
            }
            Some('a') => {
                chars.next();
                out.push('\x07');
            }
            Some('v') => {
                chars.next();
                out.push('\x0B');
            }
            Some('\\') => {
                chars.next();
                out.push('\\');
            }
            Some('"') => {
                chars.next();
                out.push('"');
            }
            Some('/') => {
                chars.next();
                out.push('/');
            }
            Some(d) if d.is_digit(8) => {
                // Octal escape: \NNN (1-3 octal digits)
                let mut octal = String::new();
                while octal.len() < 3 {
                    match chars.peek().copied() {
                        Some(d) if d.is_digit(8) => {
                            octal.push(d);
                            chars.next();
                        }
                        _ => break,
                    }
                }
                match u8::from_str_radix(&octal, 8) {
                    Ok(b) => out.push(b as char),
                    Err(_) => {
                        out.push('\\');
                        out.push_str(&octal);
                    }
                }
            }
            Some('x') => {
                // Hex escape: \x followed by 1-2 hex digits (gawk extension),
                // mirroring the lexer's string-literal handling.
                chars.next(); // consume 'x'
                let mut hex = String::new();
                while hex.len() < 2 {
                    match chars.peek().copied() {
                        Some(d) if d.is_ascii_hexdigit() => {
                            hex.push(d);
                            chars.next();
                        }
                        _ => break,
                    }
                }
                match u8::from_str_radix(&hex, 16) {
                    Ok(b) => out.push(b as char),
                    Err(_) => {
                        out.push('x');
                        out.push_str(&hex);
                    }
                }
            }
            Some(other) => {
                // Unknown escape: use the character itself
                chars.next();
                out.push(other);
            }
            None => out.push('\\'),
        }
    }

    out
}

extern crate alloc;
mod darwin;
#[path = "../../libraries/documentation_locale.rs"]
mod documentation_locale;
use documentation_locale::Language;

fn language() -> Language {
    let locale = ["LC_ALL", "LC_MESSAGES", "LANG"]
        .into_iter()
        .filter_map(|k| env::var(k).ok())
        .find(|s| !s.is_empty());
    Language::from_tag(locale.as_deref())
}

fn main() {
    match run() {
        Ok(status) => process::exit(status),
        Err(e) => {
            eprintln!("awk: {e}");
            let syntax = matches!(
                e.downcast_ref::<awk_rs::Error>(),
                Some(awk_rs::Error::Lexer { .. } | awk_rs::Error::Parser { .. })
            );
            process::exit(if syntax { 1 } else { 2 });
        }
    }
}

enum Source {
    Text(String),
    File(String),
    Include(String),
}

/// Named input remains unopened until the interpreter actually reads it.
struct Operand {
    name: String,
    stream: Option<Box<dyn BufRead>>,
}
impl Operand {
    fn stream(&mut self) -> io::Result<&mut dyn BufRead> {
        if self.stream.is_none() {
            self.stream = Some(if self.name == "-" {
                Box::new(BufReader::new(io::stdin()))
            } else {
                Box::new(BufReader::new(File::open(&self.name)?))
            });
        }
        Ok(self.stream.as_mut().unwrap().as_mut())
    }
}
impl Read for Operand {
    fn read(&mut self, b: &mut [u8]) -> io::Result<usize> {
        self.stream()?.read(b)
    }
}
impl BufRead for Operand {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        self.stream()?.fill_buf()
    }
    fn consume(&mut self, n: usize) {
        self.stream
            .as_mut()
            .expect("fill_buf before consume")
            .consume(n)
    }
}

fn run() -> Result<i32, Box<dyn std::error::Error>> {
    let lang = language();
    let mut args = env::args_os()
        .skip(1)
        .map(|s| s.into_string().map_err(|_| "non-UTF-8 argument"))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .peekable();
    let mut sources = Vec::new();
    let mut variables = Vec::new();
    let mut sandbox = false;
    let mut exec = false;
    let mut bytes = ["LC_ALL", "LC_CTYPE", "LANG"]
        .into_iter()
        .filter_map(|key| env::var(key).ok())
        .find(|v| !v.is_empty())
        .is_none_or(|v| v == "C" || v == "POSIX");
    while let Some(opt) = args.peek() {
        if !opt.starts_with('-') || opt == "-" {
            break;
        }
        let opt = args.next().unwrap();
        if opt == "--" {
            break;
        }
        let (flag, attached) = if opt.starts_with("--") {
            opt.split_once('=')
                .map_or((opt.as_str(), None), |(a, b)| (a, Some(b)))
        } else {
            let end = 1 + opt[1..].chars().next().unwrap().len_utf8();
            (&opt[..end], (end < opt.len()).then(|| &opt[end..]))
        };
        if !matches!(
            flag,
            "-F" | "--field-separator"
                | "-v"
                | "--assign"
                | "-e"
                | "--source"
                | "-f"
                | "--file"
                | "-i"
                | "--include"
                | "-E"
                | "--exec"
        ) && attached.is_some()
        {
            return Err(format!("unsupported option form {opt}").into());
        }
        let mut value = || {
            attached
                .map(str::to_owned)
                .or_else(|| args.next())
                .ok_or_else(|| format!("missing argument for {flag}"))
        };
        match flag {
            "-h" | "--help" => {
                println!(
                    "{}",
                    lang.select(
                        include_str!("../doc/help.en"),
                        include_str!("../doc/help.fr")
                    )
                    .trim_end()
                );
                return Ok(0);
            }
            "-V" | "--version" => {
                println!(
                    "{}",
                    lang.select(
                        "NovaOS awk 0.1.0 (Darwin development build)",
                        "NovaOS awk 0.1.0 (version de développement Darwin)"
                    )
                );
                return Ok(0);
            }
            "-C" | "--copyright" => {
                print!("{}", include_str!("../LICENSE-MIT"));
                return Ok(0);
            }
            "-S" | "--sandbox" => sandbox = true,
            "-b" | "--characters-as-bytes" => bytes = true,
            "-F" | "--field-separator" => {
                variables.push(("FS".into(), unescape_assignment(&value()?)))
            }
            "-v" | "--assign" => {
                let a = value()?;
                let (k, v) = a.split_once('=').ok_or("expected name=value")?;
                if !valid_name(k) {
                    return Err("invalid variable name".into());
                }
                variables.push((k.to_string(), unescape_assignment(v)));
            }
            "-i" | "--include" => sources.push(Source::Include(value()?)),
            "-e" | "--source" => sources.push(Source::Text(value()?)),
            "-f" | "--file" | "-E" | "--exec" => {
                sources.push(Source::File(value()?));
                if matches!(flag, "-E" | "--exec") {
                    exec = true;
                    break;
                }
            }
            _ => return Err(format!("unsupported option {opt}").into()),
        }
    }
    if sources.iter().all(|s| matches!(s, Source::Include(_))) {
        sources.push(Source::Text(args.next().ok_or("missing AWK program")?));
    }
    let mut code = String::new();
    let mut includes = std::collections::BTreeSet::new();
    for source in sources {
        match source {
            Source::Text(text) => code.push_str(&text),
            Source::File(path) if path == "-" => {
                io::stdin().read_to_string(&mut code)?;
            }
            Source::Include(name) => {
                let path = program_path(&name)?.canonicalize()?;
                if !includes.insert(path.clone()) {
                    continue;
                }
                code.push_str(&std::fs::read_to_string(path)?);
            }
            Source::File(name) => code.push_str(&std::fs::read_to_string(program_path(&name)?)?),
        }
        code.push('\n');
    }
    if bytes && !code.is_ascii() {
        code = code.bytes().map(char::from).collect();
    }
    let program = Parser::new(Lexer::new(&code).tokenize()?).parse()?;
    let mut awk = Interpreter::new(&program);
    awk.set_host(
        Box::new(darwin::Darwin),
        env::vars_os().filter_map(|(k, v)| Some((k.into_string().ok()?, v.into_string().ok()?))),
    );
    awk.sandbox = sandbox;
    awk.bytes = bytes;
    for (key, value) in variables {
        awk.set_variable(&key, &value)?;
    }
    let mut operands = args.collect::<Vec<_>>();
    awk.set_args(
        std::iter::once("awk".into())
            .chain(operands.iter().cloned())
            .collect(),
    );
    let assignment =
        move |name: &str| !exec && name.split_once('=').is_some_and(|(k, _)| valid_name(k));
    if operands.iter().all(|arg| assignment(arg)) {
        operands.push("-".into());
    }
    let inputs = operands.into_iter().map(move |name| {
        if assignment(&name) {
            let (k, v) = name.split_once('=').unwrap();
            awk_rs::interpreter::Source::Assign(k.to_owned(), unescape_assignment(v))
        } else {
            awk_rs::interpreter::Source::Reader(
                name.clone(),
                Box::new(Operand { name, stream: None }),
            )
        }
    });
    let mut output = io::BufWriter::new(io::stdout().lock());
    let result = awk.run_sources(inputs, &mut output);
    output.flush()?;
    Ok(result?)
}

fn valid_name(name: &str) -> bool {
    let mut it = name.bytes();
    it.next()
        .is_some_and(|b| b == b'_' || b.is_ascii_alphabetic())
        && it.all(|b| b == b'_' || b.is_ascii_alphanumeric())
}

fn program_path(name: &str) -> io::Result<std::path::PathBuf> {
    use std::path::{Path, PathBuf};
    let configured = env::var_os("AWKPATH")
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| ".".into());
    let dirs: Vec<PathBuf> = if name.contains('/') {
        vec![PathBuf::new()]
    } else {
        env::split_paths(&configured).collect()
    };
    for suffix in ["", ".awk"] {
        for dir in &dirs {
            let candidate = dir.join(format!("{name}{suffix}"));
            match candidate.metadata() {
                Ok(_) => return Ok(candidate),
                Err(e) if e.kind() == io::ErrorKind::NotFound => (),
                Err(e) => return Err(e),
            }
        }
    }
    Err(io::Error::new(
        io::ErrorKind::NotFound,
        Path::new(name).display().to_string(),
    ))
}
