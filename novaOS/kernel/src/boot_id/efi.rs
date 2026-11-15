//! Passage de relais du chargeur EFI de confiance. Le GUID qualifie un contrat
//! entre composants privilégiés ; il ne constitue pas une signature du chargeur.
const GUID: [u8; 16] = [
    0xd6, 0x92, 0xfd, 0xe8, 0x62, 0xfb, 0xa3, 0x4a, 0xa6, 0x67, 0x03, 0x74, 0xba, 0xa5, 0x01, 0xd5,
];
const SYSTEM_SIZE: usize = 120; // EFI_SYSTEM_TABLE, UEFI 2.x, pointeurs 64 bits
const MAX_TABLES: u64 = 256;
const PAGE: u64 = 4096;
const RUNTIME_WB: u64 = (1 << 63) | 8;

fn le32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}

fn le64(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap())
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = u32::MAX;
    for &byte in bytes {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & 0u32.wrapping_sub(crc & 1));
        }
    }
    !crc
}

fn map_geometry(version: u32, stride: usize, len: usize) -> bool {
    version == 1
        && (40..=256).contains(&stride)
        && stride % 8 == 0
        && (stride..=64 * 1024).contains(&len)
        && len % stride == 0
}

/// Écarte aussi un descripteur contradictoire ou une plage physique mal formée.
/// Un objet doit être tenu en entier dans un seul descripteur RAM runtime WB.
fn runtime_object(map: &[u8], stride: usize, address: u64, size: usize) -> bool {
    let Some(end) = address.checked_add(size as u64) else {
        return false;
    };
    if address == 0 || address % 8 != 0 || size == 0 {
        return false;
    }
    let mut found = false;
    for desc in map.chunks_exact(stride) {
        let start = le64(desc, 8);
        let Some(limit) = le64(desc, 24)
            .checked_mul(PAGE)
            .and_then(|n| start.checked_add(n))
        else {
            return false;
        };
        if start % PAGE != 0 || start == limit {
            return false;
        }
        if address < limit && start < end {
            if found
                || start > address
                || end > limit
                || le32(desc, 0) != 6
                || le64(desc, 32) & RUNTIME_WB != RUNTIME_WB
            {
                return false;
            }
            found = true;
        }
    }
    found
}

fn decode(system: u64, mut copy: impl FnMut(u64, &mut [u8]) -> bool) -> Option<[u64; 2]> {
    let mut header = [0u8; SYSTEM_SIZE];
    if !copy(system, &mut header)
        || &header[..8] != b"IBI SYST"
        || le32(&header, 8) >> 16 != 2
        || le32(&header, 12) != SYSTEM_SIZE as u32
        || le32(&header, 20) != 0
        || le64(&header, 96) != 0
    {
        return None;
    }
    let crc = le32(&header, 16);
    header[16..20].fill(0);
    if crc != crc32(&header) {
        return None;
    }

    let count = le64(&header, 104);
    let tables = le64(&header, 112);
    if !(1..=MAX_TABLES).contains(&count) || tables == 0 || tables % 8 != 0 {
        return None;
    }
    tables.checked_add(count * 24)?;
    let mut candidate = None;
    for i in 0..count {
        let mut table = [0u8; 24];
        if !copy(tables + 24 * i, &mut table) {
            return None;
        }
        if table[..16] == GUID && candidate.replace(le64(&table, 16)).is_some() {
            return None;
        }
    }
    let mut payload = [0u8; 32];
    if !copy(candidate?, &mut payload)
        || &payload[..8] != b"NVBOOT01"
        || le32(&payload, 8) != 1
        || le32(&payload, 12) != 32
    {
        return None;
    }
    let words = [le64(&payload, 16), le64(&payload, 24)];
    (words != [0, 0]).then_some(words)
}

#[cfg(not(test))]
pub(super) fn read() -> Option<[u64; 2]> {
    use crate::memory::firmware;
    let (system, info) = crate::boot::efi_tables()?;
    let stride = usize::try_from(info.desc_size()).ok()?;
    let len = usize::try_from(info.memmap_size()).ok()?;
    if !map_geometry(info.desc_version(), stride, len) {
        return None;
    }
    let mut map = alloc::vec::Vec::new();
    map.try_reserve_exact(len).ok()?;
    map.resize(len, 0);
    firmware::copy_loader_blob(info.memmap() as usize, &mut map).ok()?;
    decode(system, |address, out| {
        runtime_object(&map, stride, address, out.len()) && firmware::copy_efi_runtime(address, out)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    const SYSTEM: usize = 0x1000;
    const CONFIG: usize = 0x2000;
    const PAYLOAD: usize = 0x3000;

    fn fixture() -> Vec<u8> {
        let mut ram = vec![0; PAYLOAD + 32];
        let header = &mut ram[SYSTEM..SYSTEM + SYSTEM_SIZE];
        header[..8].copy_from_slice(b"IBI SYST");
        header[8..12].copy_from_slice(&0x0002_0064u32.to_le_bytes());
        header[12..16].copy_from_slice(&(SYSTEM_SIZE as u32).to_le_bytes());
        header[104..112].copy_from_slice(&2u64.to_le_bytes());
        header[112..120].copy_from_slice(&(CONFIG as u64).to_le_bytes());
        let crc = crc32(header);
        header[16..20].copy_from_slice(&crc.to_le_bytes());
        ram[CONFIG..CONFIG + 16].copy_from_slice(&GUID);
        ram[CONFIG + 16..CONFIG + 24].copy_from_slice(&(PAYLOAD as u64).to_le_bytes());
        ram[PAYLOAD..PAYLOAD + 8].copy_from_slice(b"NVBOOT01");
        ram[PAYLOAD + 8..PAYLOAD + 12].copy_from_slice(&1u32.to_le_bytes());
        ram[PAYLOAD + 12..PAYLOAD + 16].copy_from_slice(&32u32.to_le_bytes());
        ram[PAYLOAD + 16..PAYLOAD + 24].copy_from_slice(&42u64.to_le_bytes());
        ram[PAYLOAD + 24..PAYLOAD + 32].copy_from_slice(&85u64.to_le_bytes());
        ram
    }

    fn parse(ram: &[u8]) -> Option<[u64; 2]> {
        decode(SYSTEM as u64, |at, out| {
            let pos = at as usize;
            if pos % 8 != 0 {
                return false;
            }
            let Some(bytes) = ram.get(pos..).and_then(|s| s.get(..out.len())) else {
                return false;
            };
            out.copy_from_slice(bytes);
            true
        })
    }

    #[test]
    fn systeme_efi_copie_unique_et_controles() {
        let ram = fixture();
        assert_eq!(crc32(b"123456789"), 0xcbf43926);
        assert_eq!(parse(&ram), Some([42, 85]));
        for offset in [
            SYSTEM,
            SYSTEM + 16,
            SYSTEM + 104,
            CONFIG,
            CONFIG + 16,
            PAYLOAD,
            PAYLOAD + 8,
            PAYLOAD + 12,
        ] {
            let mut changed = ram.clone();
            changed[offset] ^= 1;
            assert_eq!(parse(&changed), None, "octet {offset}");
        }
        assert_eq!(parse(&ram[..ram.len() - 1]), None);
        let mut changed = ram.clone();
        changed.copy_within(CONFIG..CONFIG + 24, CONFIG + 24);
        assert_eq!(parse(&changed), None);
        changed = ram;
        changed.copy_within(PAYLOAD + 16..PAYLOAD + 24, PAYLOAD + 24);
        assert_eq!(parse(&changed), Some([42, 42]));
        changed[PAYLOAD + 16..PAYLOAD + 32].fill(0);
        assert_eq!(parse(&changed), None);
    }

    #[test]
    fn seules_les_plages_runtime_completes_sont_lisibles() {
        let mut desc = [0u8; 48];
        desc[..4].copy_from_slice(&6u32.to_le_bytes());
        desc[8..16].copy_from_slice(&PAGE.to_le_bytes());
        desc[24..32].copy_from_slice(&2u64.to_le_bytes());
        desc[32..40].copy_from_slice(&RUNTIME_WB.to_le_bytes());
        assert!(map_geometry(1, 48, 48));
        for (v, s, l) in [
            (2, 48, 48),
            (1, 32, 32),
            (1, 41, 41),
            (1, 48, 47),
            (1, 48, 65568),
        ] {
            assert!(!map_geometry(v, s, l));
        }
        assert!(runtime_object(&desc, 48, PAGE, 8192));
        for (at, size) in [
            (0, 8),
            (PAGE, 0),
            (PAGE + 1, 8),
            (PAGE, 8193),
            (u64::MAX - 7, 32),
        ] {
            assert!(!runtime_object(&desc, 48, at, size));
        }
        let mut conflict = desc;
        conflict[..4].copy_from_slice(&11u32.to_le_bytes()); // EfiMemoryMappedIO
        assert!(!runtime_object(&[desc, conflict].concat(), 48, PAGE, 32));
        assert!(!runtime_object(&[conflict, desc].concat(), 48, PAGE, 32));
        desc[32..40].fill(0);
        assert!(!runtime_object(&desc, 48, PAGE, 32));
    }
}
