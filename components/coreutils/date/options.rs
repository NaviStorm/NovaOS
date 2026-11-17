use alloc::{format, string::String, vec::Vec};

pub const DEFAULT: &str = "%a %b %e %H:%M:%S %Z %Y";
const EMAIL: &str = "%a, %d %b %Y %H:%M:%S %z";
const HELP: &str = "\nTry 'date --help' for more information.";
const NAMES: &[(&str, char)] = &[
    ("date", 'd'),
    ("debug", 'D'),
    ("file", 'f'),
    ("iso-8601", 'I'),
    ("reference", 'r'),
    ("resolution", 'p'),
    ("rfc-email", 'R'),
    ("rfc-822", 'R'),
    ("rfc-2822", 'R'),
    ("rfc-3339", 't'),
    ("set", 's'),
    ("uct", 'u'),
    ("utc", 'u'),
    ("universal", 'u'),
    ("help", 'h'),
    ("version", 'v'),
];

pub enum Action<'a> {
    Help,
    Version,
    Run(Options<'a>),
}
#[derive(Default)]
pub struct Options<'a> {
    pub date: Option<&'a str>,
    pub file: Option<&'a str>,
    pub reference: Option<&'a str>,
    pub set: Option<&'a str>,
    pub numeric: Option<&'a str>,
    pub resolution: bool,
    pub debug: bool,
    pub utc: bool,
    pub repeated: Option<char>,
    pub format: Option<&'a str>,
    pub invariant: bool,
}
impl Options<'_> {
    pub fn template(&self) -> &str {
        self.format
            .unwrap_or(if self.resolution { "%s.%N" } else { DEFAULT })
    }
}
fn standard(flag: char, value: Option<&str>) -> Result<&'static str, String> {
    let iso = flag == 'I';
    let specs = if iso {
        &["hours", "minutes", "date", "seconds", "ns"][..]
    } else {
        &["date", "seconds", "ns"][..]
    };
    let v = value.unwrap_or("date");
    match specs
        .iter()
        .copied()
        .find(|s| !v.is_empty() && s.starts_with(v))
    {
        Some("date") => Ok("%Y-%m-%d"),
        Some("hours") => Ok("%Y-%m-%dT%H%:z"),
        Some("minutes") => Ok("%Y-%m-%dT%H:%M%:z"),
        Some("seconds") if iso => Ok("%Y-%m-%dT%H:%M:%S%:z"),
        Some("seconds") => Ok("%Y-%m-%d %H:%M:%S%:z"),
        Some("ns") if iso => Ok("%Y-%m-%dT%H:%M:%S,%N%:z"),
        Some("ns") => Ok("%Y-%m-%d %H:%M:%S.%N%:z"),
        _ => {
            let option = if iso { "iso-8601" } else { "rfc-3339" };
            let kind = if v.is_empty() { "ambiguous" } else { "invalid" };
            let mut msg = format!("{kind} argument '{v}' for '--{option}'\nValid arguments are:");
            for name in specs {
                msg.push_str(&format!("\n  - '{name}'"));
            }
            Err(msg + HELP)
        }
    }
}
fn long(name: &str, argument: &str) -> Result<char, String> {
    if let Some((_, c)) = NAMES.iter().find(|x| x.0 == name) {
        return Ok(*c);
    }
    let matches: Vec<_> = NAMES.iter().filter(|x| x.0.starts_with(name)).collect();
    if let Some(first) = matches.first() {
        if matches.iter().all(|x| x.1 == first.1) {
            return Ok(first.1);
        }
        let mut msg = format!("option '{argument}' is ambiguous; possibilities:");
        for (i, (n, c)) in matches.iter().enumerate() {
            if i == 0 || matches[0].1 != *c {
                msg.push_str(&format!(" '--{n}'"));
            }
        }
        Err(msg + HELP)
    } else {
        Err(format!("unrecognized option '{argument}'{HELP}"))
    }
}

pub fn parse<'a>(args: &[&'a str]) -> Result<Action<'a>, String> {
    let mut opts = Options::default();
    let mut operands = Vec::new();
    let mut it = args.iter().copied();
    while let Some(arg) = it.next() {
        if arg == "--" {
            operands.extend(it);
            break;
        }
        if arg == "-" || !arg.starts_with('-') {
            operands.push(arg);
            continue;
        }
        let mut cluster = arg[1..].char_indices().peekable();
        while let Some((_, short)) = cluster.next() {
            let is_long = arg.starts_with("--");
            let (flag, mut value, label) = if is_long {
                let (name, value) = arg[2..]
                    .split_once('=')
                    .map_or((&arg[2..], None), |(k, v)| (k, Some(v)));
                let flag = long(name, arg)?;
                (
                    flag,
                    value,
                    format!(
                        "'--{}'",
                        NAMES
                            .iter()
                            .find(|(n, c)| n.starts_with(name) && *c == flag)
                            .unwrap()
                            .0
                    ),
                )
            } else {
                if !"dfIrRsu".contains(short) {
                    return Err(format!("invalid option -- '{short}'{HELP}"));
                }
                (short, None, format!("'{short}'"))
            };
            if "dfrstI".contains(flag) {
                if !is_long && let Some((i, _)) = cluster.peek() {
                    value = Some(&arg[1 + *i..]);
                }
                if flag != 'I' && value.is_none() {
                    value = Some(it.next().ok_or_else(|| {
                        if is_long {
                            format!("option {label} requires an argument{HELP}")
                        } else {
                            format!("option requires an argument -- {label}{HELP}")
                        }
                    })?);
                }
            } else if value.is_some() {
                return Err(format!("option {label} doesn't allow an argument{HELP}"));
            }
            match flag {
                'd' => {
                    if opts.date.is_some() {
                        opts.repeated = Some('d')
                    }
                    opts.date = value
                }
                's' => {
                    if opts.set.is_some() {
                        opts.repeated = Some('s')
                    }
                    opts.set = value
                }
                'f' => opts.file = value,
                'r' => opts.reference = value,
                'D' => opts.debug = true,
                'u' => opts.utc = true,
                'p' => opts.resolution = true,
                'R' => {
                    opts.format = Some(EMAIL);
                    opts.invariant = true
                }
                'I' | 't' => {
                    opts.format = Some(standard(flag, value)?);
                    opts.invariant = true
                }
                'h' => return Ok(Action::Help),
                'v' => return Ok(Action::Version),
                _ => unreachable!(),
            }
            if is_long || "dfIrs".contains(flag) {
                break;
            }
        }
    }
    let sources = [
        opts.date.is_some(),
        opts.file.is_some(),
        opts.reference.is_some(),
        opts.resolution,
    ]
    .into_iter()
    .filter(|x| *x)
    .count();
    if sources > 1 {
        return Err(format!(
            "the options to specify dates for printing are mutually exclusive{HELP}"
        ));
    }
    if sources != 0 && opts.set.is_some() {
        return Err(format!(
            "the options to print and set the time may not be used together{HELP}"
        ));
    }
    if operands.len() > 1 {
        return Err(format!("extra operand '{}'{}", operands[1], HELP));
    }
    if let Some(&word) = operands.first() {
        if let Some(fmt) = word.strip_prefix('+') {
            if opts.format.replace(fmt).is_some() {
                return Err("multiple output formats specified".into());
            }
        } else if sources != 0 || opts.set.is_some() {
            return Err(format!(
                "the argument '{word}' lacks a leading '+';\nwhen using an option to specify date(s), any non-option\nargument must be a format string beginning with '+'{HELP}"
            ));
        } else {
            opts.numeric = Some(word)
        }
    }
    Ok(Action::Run(opts))
}
