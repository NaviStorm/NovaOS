/// Emprunte le préfixe original, séparateurs intérieurs compris.
pub fn parent(name: &[u8]) -> &[u8] {
    let root: &[u8] = if name.starts_with(b"/") { b"/" } else { b"." };
    let Some(last) = name.iter().rposition(|&b| b != b'/') else {
        return root;
    };
    let Some(slash) = name[..last].iter().rposition(|&b| b == b'/') else {
        return root;
    };
    match name[..slash].iter().rposition(|&b| b != b'/') {
        Some(end) => &name[..=end],
        None => root,
    }
}
