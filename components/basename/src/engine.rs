/// Aucune résolution de chemin. La racine des deux plateformes est « / ».
fn name<'a>(path: &'a [u8], suffix: &[u8]) -> &'a [u8] {
    let Some(end) = path.iter().rposition(|&b| b != b'/') else {
        return &path[..path.len().min(1)];
    };
    let leaf = path[..=end].rsplit(|&b| b == b'/').next().unwrap();
    leaf.strip_suffix(suffix)
        .filter(|stem| !stem.is_empty())
        .unwrap_or(leaf)
}

pub fn record<'a>(path: &'a [u8], suffix: &[u8], zero: bool) -> (&'a [u8], u8) {
    (name(path, suffix), if zero { 0 } else { b'\n' })
}
