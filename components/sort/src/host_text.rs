use std::ffi::{CString, OsStr, OsString};
use std::os::unix::ffi::{OsStrExt, OsStringExt};

// The shared option parser uses UTF-8. Escape undecodable pathname bytes while
// it separates options and operands, then restore them at the OS boundary.
const ESC: char = '\u{e000}';
const BASE: u32 = 0xe100;

pub fn argument(value: OsString) -> String {
    let mut result = String::new();
    for chunk in value.as_bytes().utf8_chunks() {
        for c in chunk.valid().chars() {
            if c == ESC {
                result.push(ESC);
            }
            result.push(c);
        }
        for &byte in chunk.invalid() {
            result.push(ESC);
            result.push(char::from_u32(BASE + u32::from(byte)).unwrap());
        }
    }
    result
}

pub fn native(value: &str) -> OsString {
    let mut bytes = Vec::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        if c == ESC {
            let escaped = chars.next().expect("escape introduced by argument");
            if escaped != ESC {
                bytes.push((escaped as u32 - BASE) as u8);
                continue;
            }
        }
        bytes.extend_from_slice(c.encode_utf8(&mut [0; 4]).as_bytes());
    }
    OsString::from_vec(bytes)
}

pub fn diagnostic(value: &OsStr) -> String {
    if let Some(s) = value.to_str() {
        return s.to_owned();
    }
    let mut parts = String::from("'");
    for chunk in value.as_bytes().utf8_chunks() {
        parts.push_str(&chunk.valid().replace('\'', "'\\''"));
        if !chunk.invalid().is_empty() {
            parts.push_str("'$'");
            for b in chunk.invalid() {
                parts.push_str(&format!("\\{b:03o}"));
            }
            parts.push('\'');
            parts.push('\'');
        }
    }
    if parts.ends_with("''") {
        parts.pop();
    } else {
        parts.push('\'');
    }
    parts
}

pub fn collation_key(input: &[u8]) -> Vec<u8> {
    let mut key = Vec::new();
    for field in input.split_inclusive(|&b| b == 0) {
        let part = CString::new(field.strip_suffix(&[0]).unwrap_or(field)).unwrap();
        // strxfrm writes a C string, including its terminator when capacity permits.
        let mut scratch = [0i8; 1];
        let n = unsafe { libc::strxfrm(scratch.as_mut_ptr(), part.as_ptr(), 1) };
        let at = key.len();
        key.resize(at + n + 1, 0);
        unsafe {
            libc::strxfrm(key[at..].as_mut_ptr().cast(), part.as_ptr(), n + 1);
        }
    }
    key
}

pub fn columns(bytes: &[u8], multibyte: bool) -> usize {
    if !multibyte {
        return bytes.iter().filter(|&&b| b != 0).count();
    }
    unsafe extern "C" {
        fn wcwidth(c: libc::wchar_t) -> libc::c_int;
    }
    bytes
        .utf8_chunks()
        .map(|part| {
            part.invalid().len()
                + part
                    .valid()
                    .chars()
                    .map(|c| {
                        let width = unsafe { wcwidth(c as libc::wchar_t) };
                        if width < 0 { 1 } else { width as usize }
                    })
                    .sum::<usize>()
        })
        .sum()
}
