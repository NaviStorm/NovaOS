//! Nouveau contrat expérimental NXADMIN1, limité au profil QEMU.
//! Décoder un handle ne prouve aucun droit : NovaAuth vérifie la portée, le
//! titulaire, la session et la tête à CHAQUE demande. Aucun Grant dans le bloc.
use super::{ARG_MAX, Invalid, Launch, decode_arguments};
const MAGIC: &[u8; 8] = b"NXADMIN1";
pub const HEADER: usize = 80;

pub struct AdministrativeLaunch<'a> {
    pub streams: Launch<'a>,
    endpoint: u64,
}
impl<'a> AdministrativeLaunch<'a> {
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
        if flags & !7 != 0 || word(4) != 0 || word(5) != 0 || word(8) == 0 || word(9) != 0 {
            return Err(Invalid::Format);
        }
        let body = page.get(HEADER..HEADER + len).ok_or(Invalid::Format)?;
        Ok(Self {
            streams: Launch {
                argv: decode_arguments(body)?,
                utf8: flags & 1 != 0,
                french: flags & 2 != 0,
                posix_options: flags & 4 != 0,
                stdin: word(1),
                stdout: word(2),
                stderr: word(3),
                files: (0, 0),
            },
            endpoint: word(8),
        })
    }
    /// Référence locale de transport, sans droit métier implicite. Le runtime
    /// refuse les messages malformés ; le serveur vérifie les capacités réelles.
    pub fn endpoint(&self) -> u64 {
        self.endpoint
    }
}
#[cfg(target_os = "none")]
impl AdministrativeLaunch<'static> {
    pub fn current() -> nova_cli::runtime::Result<Self> {
        use nova_cli::runtime as rt;
        let page = unsafe { core::slice::from_raw_parts(0x401000 as *const u8, 4096) };
        Self::decode(page).map_err(|e| match e {
            Invalid::Memory => rt::NO_MEMORY,
            Invalid::Limit => rt::LIMIT,
            Invalid::Format => rt::BAD_ARGUMENT,
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn block(args: &[u8]) -> alloc::vec::Vec<u8> {
        let mut out = alloc::vec![0;HEADER];
        out[..8].copy_from_slice(MAGIC);
        out[48..56].copy_from_slice(&(args.len() as u64).to_le_bytes());
        out[64] = 17;
        out.extend_from_slice(args);
        out
    }
    #[test]
    fn distinct_contract_preserves_exact_arguments_without_file_capabilities() {
        let page = block(b"\x03\0\0\x03\0a b\x02\0-x");
        let decoded = AdministrativeLaunch::decode(&page).unwrap();
        assert_eq!(decoded.streams.argv, ["", "a b", "-x"]);
        assert_eq!(decoded.endpoint(), 17);
        assert_eq!(decoded.streams.files, (0, 0));
        assert!(Launch::decode(&page).is_err());
        let mut old = page.clone();
        old[..8].copy_from_slice(b"NXEXEC01");
        assert!(AdministrativeLaunch::decode(&old).is_err());
    }
    #[test]
    fn all_reserved_slots_and_unexpected_privileges_are_rejected() {
        let page = block(b"\0");
        for offset in (32..48).chain(72..80) {
            let mut p = page.clone();
            p[offset] = 1;
            assert!(AdministrativeLaunch::decode(&p).is_err());
        }
        let mut p = page.clone();
        p[64] = 0;
        assert!(AdministrativeLaunch::decode(&p).is_err());
        for bit in 3..64 {
            let mut p = page.clone();
            p[56..64].copy_from_slice(&(1u64 << bit).to_le_bytes());
            assert!(AdministrativeLaunch::decode(&p).is_err());
        }
        for len in 0..HEADER {
            assert!(AdministrativeLaunch::decode(&page[..len]).is_err());
        }
    }
    #[test]
    fn same_argument_bounds_and_utf8_validation_as_legacy() {
        for args in [&b""[..], b"\x21", b"\x01", b"\0x", b"\x01\x01\0\xff"] {
            assert!(AdministrativeLaunch::decode(&block(args)).is_err());
        }
        let mut p = block(b"\0");
        p[48..56].copy_from_slice(&2561u64.to_le_bytes());
        assert!(matches!(
            AdministrativeLaunch::decode(&p),
            Err(Invalid::Limit)
        ));
        let page = block(b"\x01\x02\0fr");
        assert_eq!(
            AdministrativeLaunch::decode(&page).unwrap().streams.argv,
            ["fr"]
        );
    }
}
