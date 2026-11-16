//! Reconnaissance stricte de l'argv copié dans le noyau, avant publication du fils.
fn argument<'a>(bytes: &mut &'a [u8]) -> Option<&'a [u8]> {
    let (length, data) = bytes.split_at_checked(2)?;
    let size = u16::from_le_bytes(length.try_into().ok()?);
    let (value, rest) = data.split_at_checked(usize::from(size))?;
    *bytes = rest;
    Some(value)
}

pub(super) fn eligible(
    catalogued: bool,
    endpoints: (u64, u64),
    streams: u64,
    status: u64,
    bytes: &[u8],
) -> bool {
    use crate::terminal::{ERROR, OUTPUT};
    if !catalogued || endpoints != (0, 0) || streams != u64::from(OUTPUT | ERROR) || status != 0 {
        return false;
    }
    let Some((&count, mut rest)) = bytes.split_first() else {
        return false;
    };
    if !matches!(argument(&mut rest), Some(b"--help" | b"--version")) {
        return false;
    }
    match count {
        1 => rest.is_empty(),
        3 => {
            argument(&mut rest) == Some(b"--")
                && matches!(argument(&mut rest), Some(b"nsh-doc=en" | b"nsh-doc=fr"))
                && rest.is_empty()
        }
        _ => false,
    }
}
