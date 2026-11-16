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
        for &arg in flags {
            let journal_option = name == "journal"
                && matches!(
                    arg,
                    "--level" | "--boot" | "--limit" | "--service" | "--follow"
                );
            if arg.starts_with('-') && !matches!(arg, "--help" | "--version") && !journal_option {
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
