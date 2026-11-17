/// Préfixes non ambigus des deux options longues, après séparation d’un éventuel '='.
pub fn name(prefix: &[u8]) -> Option<&'static str> {
    ["help", "version"]
        .into_iter()
        .find(|word| !prefix.is_empty() && word.as_bytes().starts_with(prefix))
}
