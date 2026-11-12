//! Complétion littérale dans le répertoire détenu par la session.
use super::{aliases::Aliases, builtins::COMMANDS, paths};
use alloc::{string::String, vec::Vec};
use nova_cli::{console, directory, fileapi::Client, protocol as w, runtime as rt, tty};

struct Prefix {
    text: String,
    quote: Option<char>,
    command: bool,
}

fn prefix(line: &str) -> Option<Prefix> {
    let mut text = String::new();
    let mut quote = None;
    let mut escape = false;
    let mut command = true;
    let mut started = false;
    for ch in line.chars() {
        if escape {
            text.push(ch);
            escape = false;
        } else if ch == '\\' && quote != Some('\'') {
            escape = true;
        } else if quote == Some(ch) {
            quote = None;
        } else if quote.is_none() && matches!(ch, '\'' | '"') {
            quote = Some(ch);
        } else if quote.is_none() && matches!(ch, ' ' | ';' | '|' | '&') {
            if ch != ' ' {
                command = true;
            } else if started {
                command = false;
            }
            text.clear();
            started = false;
            continue;
        } else {
            if ch == '#' && !started && quote.is_none() {
                return None;
            }
            text.push(ch);
        }
        started = true;
    }
    (!escape).then_some(Prefix {
        text,
        quote,
        command,
    })
}

pub fn complete(
    fs: &mut Client,
    aliases: &Aliases,
    line: &mut String,
    pos: &mut usize,
    list: bool,
) -> rt::Result<bool> {
    if line[*pos..].starts_with(|c: char| !matches!(c, ' ' | ';' | '&' | '|')) {
        return Ok(false);
    }
    let Some(p) = prefix(&line[..*pos]) else {
        return Ok(false);
    };
    let mut names = Vec::new();
    let (parent, leaf) = p.text.rsplit_once('/').unwrap_or((".", &p.text));
    if p.command && !p.text.contains('/') {
        for name in COMMANDS.split_whitespace().chain(aliases.names()) {
            if name.starts_with(leaf) {
                if names.len() == 128 {
                    return Err(rt::LIMIT);
                }
                names.push(String::from(name));
            }
        }
    } else {
        let parent = paths::resolve(fs, if parent.is_empty() { "/" } else { parent })?;
        let h = fs.directory(0, parent.as_bytes(), w::ENUMERATE | w::TRAVERSE)?;
        let listing = directory::visit_names(fs, h, 128, |fs, name| {
            if name.starts_with(leaf)
                && (!name.starts_with('.') || leaf.starts_with('.'))
                && name.chars().all(console::safe)
            {
                let stat = fs.at(w::LOOKUP_AT, h, name.as_bytes(), 0)?;
                let mut candidate = String::from(name);
                if stat[w::PAYLOAD + 24] == 2 {
                    candidate.push('/');
                }
                names.try_reserve(1).map_err(|_| rt::NO_MEMORY)?;
                names.push(candidate);
            }
            Ok(())
        });
        fs.simple(w::DIR_CLOSE, h)?;
        listing?;
    }
    names.sort_unstable();
    names.dedup();
    let Some(first) = names.first() else {
        return Ok(false);
    };
    let last = names.last().unwrap();
    let common: String = first
        .chars()
        .zip(last.chars())
        .take_while(|(a, b)| a == b)
        .map(|(a, _)| a)
        .collect();
    let mut suffix = String::new();
    for c in common[leaf.len()..].chars() {
        if p.quote == Some('\'') && c == '\'' {
            suffix.push_str("'\\''");
            continue;
        }
        let protect = match p.quote {
            Some('"') => matches!(c, '"' | '\\' | '$' | '`'),
            Some(_) => false,
            None => !c.is_alphanumeric() && !matches!(c, '.' | '/' | '_' | '-'),
        };
        if protect {
            suffix.push('\\');
        }
        suffix.push(c);
    }
    if names.len() == 1 && !first.ends_with('/') {
        if let Some(q) = p.quote {
            suffix.push(q);
        }
    }
    if !suffix.is_empty() && line.len() + suffix.len() <= tty::LINE_MAX {
        line.insert_str(*pos, &suffix);
        *pos += suffix.len();
        return Ok(true);
    }
    if list && names.len() > 1 {
        console::write(tty::OUTPUT, "\n")?;
        for name in names {
            console::write(tty::OUTPUT, &name)?;
            console::write(tty::OUTPUT, "\n")?;
        }
        return Ok(true);
    }
    Ok(false)
}
