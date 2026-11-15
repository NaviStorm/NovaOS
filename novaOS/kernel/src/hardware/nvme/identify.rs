//! Champs utilisés pour reconnaître le banc et son rôle. Les identités
//! proviennent des octets DMA après une complétion vérifiée par le superviseur.
use super::{Completion, Error, PAGE};

pub(super) const VERSION: u32 = 0x0001_0400;
const VID: u16 = 0x1b36;
const READ_SERIAL: &[u8] = b"NOVA0190READ";
const WRITE_SERIAL: &[u8] = b"NOVA0190RW";
const MODEL: &[u8] = b"QEMU NVMe Ctrl";
const EUI: [u8; 8] = *b"NOVA0190";

fn padded(actual: &[u8], expected: &[u8]) -> bool {
    actual.starts_with(expected) && actual[expected.len()..].iter().all(|&b| b == b' ')
}

fn accepts_size(range: u8, exponent: u8) -> bool {
    (range & 0xf) <= exponent && range >> 4 >= exponent
}

pub(super) fn controller(
    data: &[u8; PAGE],
    phase2_media: bool,
    info: &mut Completion,
) -> Result<(), Error> {
    let vid = u16::from_le_bytes([data[0], data[1]]);
    let version = u32::from_le_bytes(data[80..84].try_into().unwrap());
    let namespaces = u32::from_le_bytes(data[516..520].try_into().unwrap());
    if vid != VID
        || version != VERSION
        || !padded(
            &data[4..24],
            if phase2_media {
                WRITE_SERIAL
            } else {
                READ_SERIAL
            },
        )
        || !padded(&data[24..64], MODEL)
        || !accepts_size(data[512], 6)
        || !accepts_size(data[513], 4)
        || namespaces == 0
    {
        return Err(Error::Identity);
    }
    // Pour MPSMIN=0, le plus petit MDTS non nul permet déjà deux pages.
    // MDTS=0 n'impose pas de maximum. Le profil utilise toujours une page.
    info.vid = vid;
    info.version = version;
    info.serial.copy_from_slice(&data[4..24]);
    info.model.copy_from_slice(&data[24..64]);
    info.mdts = data[77];
    info.vwc = data[525];
    Ok(())
}

pub(super) fn list(data: &[u8; PAGE]) -> Result<(), Error> {
    if data[..4] != 1u32.to_le_bytes() || data[4..].iter().any(|&b| b != 0) {
        return Err(Error::Namespace);
    }
    Ok(())
}

pub(super) fn namespace(data: &[u8; PAGE], info: &mut Completion) -> Result<(), Error> {
    let sectors = u64::from_le_bytes(data[..8].try_into().unwrap());
    let capacity = u64::from_le_bytes(data[8..16].try_into().unwrap());
    let used = u64::from_le_bytes(data[16..24].try_into().unwrap());
    let format = usize::from(data[26] & 15);
    if sectors == 0
        || capacity != sectors
        || used > capacity
        || data[26] & !15 != 0
        || format > usize::from(data[25])
        || data[29] != 0
        || data[30] != 0
        || data[120..128] != EUI
    {
        return Err(Error::Namespace);
    }
    let off = 128 + 4 * format;
    if data[off..off + 2] != [0, 0] || !matches!(data[off + 2], 9 | 12) {
        return Err(Error::Geometry);
    }
    let block_size = 1u32 << data[off + 2];
    sectors
        .checked_mul(u64::from(block_size))
        .ok_or(Error::Geometry)?;
    info.nsid = 1;
    info.block_size = block_size;
    info.sectors = sectors;
    info.eui64.copy_from_slice(&data[120..128]);
    Ok(())
}
