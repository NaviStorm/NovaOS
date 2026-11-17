#![cfg_attr(target_os = "none", no_std)]
#![no_main]
extern crate alloc;
#[cfg(target_os = "macos")]
mod darwin;
mod engine;
mod float;
#[cfg(all(target_os = "none", not(feature = "sonde")))]
mod native;
mod number;
mod printable;
mod quoting;
#[cfg(all(target_os = "none", feature = "sonde"))]
mod sonde;

use alloc::vec::Vec;
#[cfg(target_os = "macos")]
#[path = "../../libraries/documentation_locale.rs"]
mod language;
pub const VERSION: &str = concat!("printf (NovaOS) ", env!("CARGO_PKG_VERSION"), "\n");
pub const EN: &str = include_str!("../help.en");
pub const FR: &str = include_str!("../help.fr");

#[derive(Clone, Copy)]
pub struct Locale<'a> {
    decimal: &'a [u8],
    separator: &'a [u8],
    groups: &'a [u8],
    // Decode one local character; return code point, length and printability.
    decode: fn(&[u8]) -> Option<(u32, usize, bool)>,
    encode: fn(char, &mut Vec<u8>) -> bool,
    curved_quotes: bool,
    french: bool,
    posix: bool,
}
pub fn utf8(bytes: &[u8]) -> Option<(u32, usize, bool)> {
    let n = match *bytes.first()? {
        0..=127 => 1,
        194..=223 => 2,
        224..=239 => 3,
        240..=244 => 4,
        _ => return None,
    };
    let ch = core::str::from_utf8(bytes.get(..n)?).ok()?.chars().next()?;
    let code = ch as u32;
    let count = printable::BOUNDARIES.partition_point(|&boundary| boundary <= code);
    Some((code, n, count % 2 == 1))
}
pub fn ascii(bytes: &[u8]) -> Option<(u32, usize, bool)> {
    let &b = bytes.first()?;
    b.is_ascii()
        .then_some((b as u32, 1, b == 32 || b.is_ascii_graphic()))
}
pub fn encode_utf8(ch: char, out: &mut Vec<u8>) -> bool {
    out.extend(ch.encode_utf8(&mut [0; 4]).as_bytes());
    true
}
pub fn encode_ascii(ch: char, out: &mut Vec<u8>) -> bool {
    if ch.is_ascii() {
        out.push(ch as u8);
        true
    } else {
        false
    }
}
impl Default for Locale<'static> {
    fn default() -> Self {
        Self {
            decimal: b".",
            separator: b"",
            groups: b"",
            decode: ascii,
            encode: encode_ascii,
            curved_quotes: false,
            french: false,
            posix: false,
        }
    }
}
pub trait Output {
    fn write(&mut self, err: bool, bytes: &[u8]) -> Result<(), ()>;
}
