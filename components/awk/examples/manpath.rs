//! Sélection d'un répertoire de manuel pour man(1) Darwin avec la règle ARC-026.
extern crate alloc;
use std::{env, path::PathBuf, process::ExitCode};
#[path = "../../libraries/documentation_locale.rs"]
mod documentation_locale;

fn main() -> ExitCode {
    let locale = ["LC_ALL", "LC_MESSAGES", "LANG"]
        .into_iter()
        .filter_map(|key| env::var(key).ok())
        .find(|s| !s.is_empty());
    let lang = documentation_locale::Language::from_tag(locale.as_deref());
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("doc/man");
    path.push(lang.select("", "fr"));
    if path.join("man1/awk.1").is_file() {
        println!("{}", path.display());
        ExitCode::SUCCESS
    } else {
        eprintln!(
            "awk: {}",
            lang.select("manual not found", "manuel introuvable")
        );
        ExitCode::FAILURE
    }
}
