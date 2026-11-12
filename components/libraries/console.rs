//! Flux natifs. Le terminal valide les scalaires et sépare l'édition des sorties ordinaires.
use crate::{runtime as rt, tty};
pub fn write(port: u8, mut text: &str) -> rt::Result<()> {
    while !text.is_empty() {
        let mut n = text.len().min(rt::DEBUG_MAX);
        while !text.is_char_boundary(n) {
            n -= 1
        }
        rt::value(rt::call(
            tty::CONSOLE,
            port.into(),
            text.as_ptr() as u64,
            n as u64,
        ))?;
        text = &text[n..];
    }
    Ok(())
}
pub fn edit(action: u64) -> rt::Result<()> {
    rt::value(rt::call(tty::CONSOLE, tty::EDIT.into(), action, 0)).map(|_| ())
}
pub use crate::tty::printable as safe;
pub fn printable(port: u8, text: &str) -> rt::Result<()> {
    let mut start = 0;
    for (i, c) in text.char_indices() {
        if c != '\n' && !safe(c) {
            write(port, &text[start..i])?;
            write(port, &alloc::format!("\\u{{{:x}}}", c as u32))?;
            start = i + c.len_utf8();
        }
    }
    write(port, &text[start..])
}
pub fn reason(e: u64) -> &'static str {
    use crate::protocol as w;
    match e {
        rt::DENIED => "accès refusé",
        rt::REVOKED | rt::CLOSED => "capacité révoquée",
        rt::LIMIT => "limite atteinte",
        rt::BAD_ARGUMENT => "argument invalide",
        rt::NO_MEMORY => "mémoire insuffisante",
        w::UNSUPPORTED => "opération indisponible pour ce profil",
        w::EXISTS => "existe déjà",
        w::NOT_FOUND => "introuvable",
        w::IS_DIR => "est un répertoire",
        w::NOT_EMPTY => "répertoire non vide",
        w::NOT_DIR => "répertoire attendu",
        w::UNCERTAIN => "publication incertaine",
        w::IO => "erreur d’E/S",
        w::BUSY => "objet occupé",
        _ => "échec de l’opération",
    }
}
