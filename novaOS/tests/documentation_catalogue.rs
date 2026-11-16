//! Vérifie les données compilées et le parseur réel hors de la VM.
extern crate alloc;
extern crate self as nova_cli;
#[path = "../../components/shell/doc_options.rs"]
mod doc_options;
pub mod tty {
    pub const OUTPUT: u64 = 1;
    pub const ERROR: u64 = 2;
}
pub mod console {
    use std::cell::RefCell;
    thread_local! { pub static LOG: RefCell<String> = RefCell::new(String::new()); }
    pub fn write(_: u64, s: &str) -> crate::runtime::Result<()> {
        LOG.with(|log| log.borrow_mut().push_str(s));
        Ok(())
    }
    pub use write as printable;
}
#[allow(dead_code)]
#[path = "../../components/libraries/command.rs"]
mod command;
#[path = "../../components/libraries/documentation.rs"]
mod documentation;
#[allow(dead_code)]
#[path = "../../components/libraries/options.rs"]
mod options;
mod runtime {
    pub const BAD_ARGUMENT: u64 = 3;
    pub type Result<T> = core::result::Result<T, u64>;
}

#[test]
fn catalogue_et_rendu() {
    for lang in [
        documentation::Language::English,
        documentation::Language::French,
    ] {
        let mut names = std::collections::BTreeSet::new();
        for page in documentation::PAGES {
            assert!(names.insert(page.name));
            let help = page.render(&[], false, lang).unwrap();
            let manual = page.render(&[], true, lang).unwrap();
            assert_eq!(manual, page.render(&[], true, lang).unwrap());
            for field in [page.syntax, page.arguments, page.examples, page.description] {
                let field = field.get(lang);
                assert!(!field.is_empty() && help.contains(field) && manual.contains(field));
            }
            for field in [page.rights, page.effects, page.limits] {
                assert!(manual.contains(field.get(lang)));
            }
            assert!(manual.contains(page.version(lang).trim()));
            for (syntax, desc) in page.topics {
                let syntax = syntax.get(lang);
                let path: Vec<_> = syntax
                    .split_whitespace()
                    .take_while(|w| w.bytes().all(|b| b.is_ascii_lowercase()))
                    .collect();
                assert!(
                    help.contains(syntax)
                        && page
                            .render(&path, true, lang)
                            .unwrap()
                            .contains(desc.get(lang))
                );
            }
            assert!(page.render(&["inconnue"], true, lang).is_none());
        }
        assert_eq!(
            names.into_iter().collect::<Vec<_>>(),
            [
                "[", "alias", "break", "cat", "cd", "clear", "continue", "cp", "echo", "exit",
                "export", "false", "head", "help", "history", "journal", "ls", "man", "mkdir",
                "more", "mv", "nsh", "ps", "pwd", "return", "rm", "rmdir", "service", "tail",
                "test", "top", "touch", "true", "unalias", "uniq", "unset", "vi", "wc"
            ]
        );
    }
}

#[test]
fn grammaire_des_utilitaires() {
    for lang in [
        documentation::Language::English,
        documentation::Language::French,
    ] {
        for page in documentation::PAGES {
            let name = page.name;
            let Some(p) = command::Program::named(name, false) else {
                continue;
            };
            for x in page
                .arguments
                .get(lang)
                .split(|c: char| !(c.is_ascii_alphanumeric() || c == '-'))
                .filter(|x| x.starts_with("--") && x.len() > 2)
            {
                let extra = if matches!(x,"--width" | "--skip-fields" | "--skip-chars" | "--check-chars")
                    || matches!(name, "head" | "tail") && matches!(x, "--lines" | "--bytes")
                {
                    vec![x, "8"]
                } else {
                    vec![x]
                };
                assert!(options::parse(p, &extra).is_ok(), "{name} {x}");
            }
            assert!(options::parse(p, &["--inconnue"]).is_err());
            let literals = if name == "uniq" { &["--", "--help", "--version"][..] } else { &["--", "--help", "--version", "--inconnue"][..] };
            let o = options::parse(p, literals).unwrap();
            assert_eq!(o.files, literals[1..]);
            assert!(!o.help && !o.version);
        }
    }
}

#[test]
fn aiguillage_sans_aucune_interface_metier() {
    for lang in [
        documentation::Language::English,
        documentation::Language::French,
    ] {
        for page in documentation::PAGES {
            for (option, output) in [
                ("--help", page.render(&[], false, lang).unwrap()),
                ("--version", page.version(lang)),
            ] {
                console::LOG.with(|log| log.borrow_mut().clear());
                assert_eq!(doc_options::answer(page.name, &[option], lang), Ok(Some(0)));
                console::LOG.with(|log| assert_eq!(*log.borrow(), output));
            }
            assert_eq!(
                doc_options::answer(page.name, &["--inconnue"], lang),
                Ok(Some(3))
            );
            assert_eq!(
                doc_options::answer(page.name, &["--", "--inconnue"], lang),
                Ok(None)
            );
        }
    }
}

#[test]
fn etiquettes_bcp47() {
    use documentation::Language::{self, English, French};
    for tag in [
        "fr",
        "FR",
        "fr-FR",
        "fr-CA",
        "fr-BE",
        "fr-CH",
        "fr-zzzz",
        "fr-Zzzz",
        "fr-1694acad",
        "Fr-Latn-419",
        "fr-1996",
        "fr-u-ca-gregory",
        "fr-x-a",
        "fr-Latn-CA-1996-x-demo",
    ] {
        assert_eq!(Language::from_tag(Some(tag)), French, "{tag}");
    }
    for tag in [
        "",
        "frr",
        "français",
        "en-FR",
        "en-US",
        "de-DE",
        "es-ES",
        "zz",
        "fr--CA",
        "fr-zzzzz",
        "fr-Latn-CA-Zzzzz",
        "fr-Abcd",
        "fr-999",
        "fr-abc",
        "fr_CA",
        "fr.UTF-8",
        "fr-",
        "fr-x",
        "fr-x-",
        "fr-CA-FR",
        "fr-a",
        "fr-1996-1996",
        "fr-a-test-A-text",
        "fr-abcdefghij",
        "fr-ÇÀ",
        " fr",
    ] {
        assert_eq!(Language::from_tag(Some(tag)), English, "{tag}");
    }
    assert_eq!(Language::from_tag(None), English);
}

#[path = "../kernel/src/sched/process/ipc/document_launch.rs"]
mod document_launch;
#[allow(dead_code)]
#[path = "../abi/terminal.rs"]
mod terminal;

#[test]
fn admission_documentaire_stricte() {
    fn argv(args: &[&str]) -> Vec<u8> {
        let mut data = vec![args.len() as u8];
        for arg in args {
            data.extend_from_slice(&(arg.len() as u16).to_le_bytes());
            data.extend_from_slice(arg.as_bytes());
        }
        data
    }
    let ports = u64::from(terminal::OUTPUT | terminal::ERROR);
    let allowed = |bytes: &[u8]| document_launch::eligible(true, (0, 0), ports, 0, bytes);
    for option in ["--help", "--version"] {
        assert!(allowed(&argv(&[option])));
        for tag in ["nsh-doc=fr", "nsh-doc=en"] {
            let args = [option, "--", tag];
            let wire = argv(&args);
            assert!(allowed(&wire));
            assert_eq!(
                documentation::child_language(&args),
                documentation::Language::from_tag(tag.strip_prefix("nsh-doc="))
            );
            for n in 0..wire.len() {
                assert!(!allowed(&wire[..n]));
            }
            let mut extra = wire.clone();
            extra.push(0);
            assert!(!allowed(&extra));
            for (catalogued, handles, streams, prep) in [
                (false, (0, 0), ports, 0),
                (true, (1, 2), ports, 0),
                (true, (0, 0), ports | 1, 0),
                (true, (0, 0), ports, 5),
            ] {
                assert!(!document_launch::eligible(
                    catalogued, handles, streams, prep, &wire
                ));
            }
        }
    }
    for bad in [
        vec!["--", "--help"],
        vec!["--help", "--", "path"],
        vec!["-f", "--", "nsh-doc=fr"],
        vec!["--help", "--", "nsh-doc=fr", "/voisin"],
    ] {
        assert!(!allowed(&argv(&bad)));
    }
}
