use alloc::{format, vec::Vec};

/// Représentation des octets d’un chemin dans un diagnostic en locale C.
pub fn escaped(input: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    for &b in input {
        let repr: &[u8] = match b {
            b'\n' => b"\\n",
            b'\t' => b"\\t",
            b'\r' => b"\\r",
            7 => b"\\a",
            8 => b"\\b",
            11 => b"\\v",
            12 => b"\\f",
            b'\\' => b"\\\\",
            b'\'' => b"\\'",
            32..=126 => {
                out.push(b);
                continue;
            }
            _ => {
                out.extend_from_slice(format!("\\{b:03o}").as_bytes());
                continue;
            }
        };
        out.extend_from_slice(repr);
    }
    out
}
