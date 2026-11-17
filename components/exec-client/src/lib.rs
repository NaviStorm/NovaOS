//! Bloc de démarrage des ELF installés et flux d’octets natifs.
#![no_std]
extern crate alloc;
use alloc::vec::Vec;

const MAGIC: &[u8; 8] = b"NXEXEC01";
const HEADER: usize = 64;
const ARG_MAX: usize = 2560;
const ARG_COUNT: usize = 32;

pub struct Launch<'a> {
    pub argv: Vec<&'a str>,
    pub utf8: bool,
    pub french: bool,
    pub posix_options: bool,
    stdin: u64,
    stdout: u64,
    stderr: u64,
    files: (u64, u64),
}

#[derive(Debug, PartialEq, Eq)]
pub enum Invalid {
    Format,
    Limit,
    Memory,
}

impl<'a> Launch<'a> {
    pub fn decode(page: &'a [u8]) -> Result<Self, Invalid> {
        let header = page.get(..HEADER).ok_or(Invalid::Format)?;
        if &header[..8] != MAGIC {
            return Err(Invalid::Format);
        }
        let word = |i: usize| u64::from_le_bytes(header[i * 8..i * 8 + 8].try_into().unwrap());
        let len = usize::try_from(word(6)).map_err(|_| Invalid::Limit)?;
        if len > ARG_MAX {
            return Err(Invalid::Limit);
        }
        let flags = word(7);
        if flags & !7 != 0 || (word(4) == 0) != (word(5) == 0) {
            return Err(Invalid::Format);
        }
        let body = page.get(HEADER..HEADER + len).ok_or(Invalid::Format)?;
        let (&argc, mut rest) = body.split_first().ok_or(Invalid::Format)?;
        if usize::from(argc) > ARG_COUNT {
            return Err(Invalid::Limit);
        }
        let mut argv = Vec::new();
        argv.try_reserve_exact(usize::from(argc))
            .map_err(|_| Invalid::Memory)?;
        for _ in 0..argc {
            let (size, more) = rest.split_at_checked(2).ok_or(Invalid::Format)?;
            let n = usize::from(u16::from_le_bytes(size.try_into().unwrap()));
            let (arg, more) = more.split_at_checked(n).ok_or(Invalid::Format)?;
            argv.push(core::str::from_utf8(arg).map_err(|_| Invalid::Format)?);
            rest = more;
        }
        if !rest.is_empty() {
            return Err(Invalid::Format);
        }
        Ok(Self {
            argv,
            utf8: flags & 1 != 0,
            french: flags & 2 != 0,
            posix_options: flags & 4 != 0,
            stdin: word(1),
            stdout: word(2),
            stderr: word(3),
            files: (word(4), word(5)),
        })
    }
}
#[cfg(target_os = "none")]
mod io;

#[cfg(test)]
mod tests {
    use super::*;
    fn packet(raw: &[u8]) -> Vec<u8> {
        let mut page = alloc::vec![0;HEADER];
        page[..8].copy_from_slice(MAGIC);
        page[48..56].copy_from_slice(&(raw.len() as u64).to_le_bytes());
        page.extend_from_slice(raw);
        page
    }
    #[test]
    fn arguments_are_only_data() {
        let p = packet(b"\x02\x05\x00-f1,3\x0b\x00donnees.tsv");
        let v = Launch::decode(&p).unwrap();
        assert_eq!(v.argv, ["-f1,3", "donnees.tsv"]);
        assert_eq!((v.stdin, v.stdout, v.stderr, v.files), (0, 0, 0, (0, 0)));
    }
    #[test]
    fn refuse_incomplete_or_unnegotiated_inputs() {
        for raw in [&b""[..], b"\x01", b"\x00x", b"\x01\x01\x00\xff", b"\x21"] {
            assert!(Launch::decode(&packet(raw)).is_err());
        }
        let mut page = packet(b"\0");
        page[32] = 1;
        assert!(Launch::decode(&page).is_err());
        page[40] = 2;
        assert!(Launch::decode(&page).is_ok());
        page[56] = 8;
        assert!(Launch::decode(&page).is_err());
        page[56] = 0;
        page[0] = 0;
        assert!(Launch::decode(&page).is_err());
    }
}
