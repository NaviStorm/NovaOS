//! Traitement local avant la résolution des chemins et le choix des capacités.
use nova_cli::{command::Program, console, documentation, options, runtime as rt, tty};

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
    if name == "seq" {
        use nova_cli::seq_options::{Options, parse};
        let words: alloc::vec::Vec<_> = args.iter().map(|s| s.as_bytes()).collect();
        let text = match parse(&words) {
            Ok(Options::Help) => page.render(&[], false, lang).ok_or(rt::BAD_ARGUMENT)?,
            Ok(Options::Version) => page.version(lang),
            Ok(Options::Numbers { .. }) => return Ok(None),
            Err(message) => {
                console::printable(
                    tty::ERROR,
                    &alloc::format!(
                        "seq: {}\n",
                        alloc::string::String::from_utf8_lossy(&message.0)
                    ),
                )?;
                return Ok(Some(1));
            }
        };
        console::write(tty::OUTPUT, &text)?;
        return Ok(Some(0));
    }
    let end = args.iter().position(|a| *a == "--").unwrap_or(args.len());
    let flags = &args[..end];
    let (help, version, topic) = if let Some(p) = Program::named(name, false) {
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
