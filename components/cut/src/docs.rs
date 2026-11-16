use alloc::{format, string::String};
pub fn text(help: bool, french: bool) -> String {
    if !help {
        return format!("cut (NovaOS) {}\n", include_str!("../VERSION").trim());
    }
    let (title, options, example) = if french {
        (
            "Extraire des octets, des caractères ou des champs.",
            include_str!("../options.fr"),
            "Exemple",
        )
    } else {
        (
            "Select bytes, characters or fields.",
            include_str!("../options.en"),
            "Example",
        )
    };
    format!(
        "cut — {title}\ncut OPTION... [FILE]...\n{options}\n{example}: cut -f1,3 donnees.tsv\nman cut\n"
    )
}
