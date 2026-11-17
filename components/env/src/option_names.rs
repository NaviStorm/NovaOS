pub const OPTIONS: &[(&[u8], u8)] = &[
    (b"argv0", b'a'),
    (b"ignore-environment", b'i'),
    (b"env0-from", b'F'),
    (b"null", b'0'),
    (b"unset", b'u'),
    (b"chdir", b'C'),
    (b"default-signal", b'd'),
    (b"ignore-signal", b'g'),
    (b"block-signal", b'b'),
    (b"list-signal-handling", b'l'),
    (b"debug", b'v'),
    (b"split-string", b'S'),
    (b"help", b'h'),
    (b"version", b'V'),
];

pub fn matching(prefix: &[u8]) -> impl Iterator<Item = &'static (&'static [u8], u8)> {
    OPTIONS
        .iter()
        .filter(move |(name, _)| !prefix.is_empty() && name.starts_with(prefix))
}
pub fn required(id: u8) -> bool {
    b"aCFSu".contains(&id)
}
pub fn short(id: u8) -> bool {
    b"aCiSuv0".contains(&id)
}
