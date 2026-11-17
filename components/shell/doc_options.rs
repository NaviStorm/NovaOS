//! Traitement local avant la résolution des chemins et le choix des capacités.
use nova_cli::{command::Program, console, documentation, options, runtime as rt, tty};

#[path = "../env/src/option_names.rs"]
mod env_names;

/// Seuls les arguments précédant la commande et les affectations appartiennent à env.
/// Une chaîne -S doit être interprétée par l’utilitaire avec son environnement.
fn env_document<'a>(args: &[&'a str]) -> Result<Option<bool>, &'a str> {
    let mut words = args.iter().copied();
    while let Some(word) = words.next() {
        if word == "--" || word == "-" || !word.starts_with('-') {
            break;
        }
        if let Some(raw) = word.strip_prefix("--") {
            let (name, attached) = raw
                .split_once('=')
                .map_or((raw, None), |(k, v)| (k, Some(v)));
            let mut choices = env_names::matching(name.as_bytes());
            let Some(&(canonical, id)) = choices.next() else {
                return Err(word);
            };
            if canonical != name.as_bytes() && choices.next().is_some() {
                return Err(word);
            }
            if env_names::required(id) {
                if attached.or_else(|| words.next()).is_none() {
                    return Err(word);
                }
            } else if attached.is_some() && !b"bdg".contains(&id) {
                return Err(word);
            }
            match id {
                b'h' | b'V' => return Ok(Some(id == b'h')),
                b'S' | b'b' | b'd' | b'g' => return Ok(None),
                _ => (),
            }
        } else {
            for (i, id) in word.bytes().enumerate().skip(1) {
                if !env_names::short(id) {
                    return Err(word);
                }
                if env_names::required(id) {
                    if i + 1 == word.len() && words.next().is_none() {
                        return Err(word);
                    }
                    if id == b'S' {
                        return Ok(None);
                    }
                    break;
                }
            }
        }
    }
    Ok(None)
}

fn invalid(option: &str) -> rt::Result<Option<u64>> {
    console::printable(
        tty::ERROR,
        &alloc::format!("argument invalide, option ou valeur non prise en charge : {option}\n"),
    )?;
    Ok(Some(rt::BAD_ARGUMENT))
}

pub fn answer(name: &str, args: &[&str], lang: documentation::Language) -> rt::Result<Option<u64>> {
    let Some(page) = documentation::lookup(name) else {
        return Ok(None);
    };
    let end = args.iter().position(|a| *a == "--").unwrap_or(args.len());
    let flags = &args[..end];
    let (help, version, topic) = if name == "env" {
        match env_document(args) {
            Ok(Some(help)) => (help, !help, &args[..0]),
            Ok(None) => return Ok(None),
            Err(arg) => return invalid(arg),
        }
    } else if let Some(p) = Program::named(name, false) {
        let parsed = match options::parse(p, args) {
            Ok(o) => o,
            Err(_) => return invalid(options::invalid_option(p, args).unwrap_or(name)),
        };
        (parsed.help, parsed.version, &args[..0])
    } else {
        // Le texte qui suit -c appartient au langage de scripts.
        if name == "nsh" && args.first() == Some(&"-c") {
            return Ok(None);
        }
        // Les arguments d’une expression complète sont des données, même si un
        // chemin ou une chaîne porte le nom d’une option documentaire.
        if name == "[" && args.last() == Some(&"]") {
            return Ok(None);
        }
        if name == "test"
            && matches!(
                args,
                ["-n" | "-z" | "-e" | "-f" | "-d", _]
                    | [
                        _,
                        "=" | "==" | "!=" | "-lt" | "-le" | "-eq" | "-ne" | "-ge" | "-gt",
                        _
                    ]
            )
        {
            return Ok(None);
        }
        if (name == "echo" || name == "test" && args.len() == 1)
            && args.first().is_some_and(|word| word.parse::<i64>().is_ok())
        {
            return Ok(None);
        }
        // Après le premier opérande d’echo, tous les mots sont du texte.
        let flags = if name == "echo" {
            &flags[..flags.len().min(1)]
        } else {
            flags
        };
        for &arg in flags {
            let journal_option = name == "journal"
                && matches!(
                    arg,
                    "--level" | "--boot" | "--limit" | "--service" | "--follow"
                );
            let literal_dash = arg == "-" && matches!(name, "echo" | "test");
            if arg.starts_with('-')
                && !literal_dash
                && !matches!(arg, "--help" | "--version")
                && !journal_option
            {
                return invalid(arg);
            }
        }
        let alias = matches!(name, "service" | "journal") && args == ["help"];
        let topic = if alias || page.topics.is_empty() {
            &args[..0]
        } else {
            &flags[..flags
                .iter()
                .position(|a| a.starts_with('-'))
                .unwrap_or(flags.len())]
        };
        (
            alias || flags.contains(&"--help"),
            flags.contains(&"--version"),
            topic,
        )
    };
    let text = if help {
        let Some(text) = page.render(topic, false, lang) else {
            return invalid("sous-commande inconnue");
        };
        text
    } else if version {
        page.version(lang)
    } else {
        return Ok(None);
    };
    console::write(tty::OUTPUT, &text)?;
    Ok(Some(0))
}
