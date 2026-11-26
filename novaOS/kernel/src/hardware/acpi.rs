//! Inventaire ACPI. Les octets proviennent exclusivement du lecteur de mémoire
//! firmware fourni par la plateforme. L'inventaire n'accorde aucun droit MMIO.
extern crate alloc;
use alloc::vec::Vec;
use core::ops::Range;

#[cfg(any(test, all(target_arch = "x86_64", feature = "vtd-lab")))]
#[path = "acpi/q35_dmar.rs"]
mod q35_dmar;
#[cfg(any(test, all(target_arch = "x86_64", feature = "vtd-lab")))]
pub(crate) use q35_dmar::Q35Dmar;

#[path = "acpi/iort.rs"]
mod iort;
#[path = "acpi/tables.rs"]
mod tables;

const HEADER: usize = 36;
const MAX_TABLES: usize = 128;
const MAX_TABLE: usize = 128 * 1024;
const TOTAL_BYTES: usize = 512 * 1024;
const MAX_ECAM: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Address,
    Checksum,
    Signature,
    Length,
    Format,
    Overlap,
    Limit,
    NoMemory,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ecam {
    /// Base du bus zéro, même lorsque first_bus est non nul.
    pub base: u64,
    pub segment: u16,
    pub first_bus: u8,
    pub last_bus: u8,
}

impl Ecam {
    fn physical(&self) -> Result<Range<u64>> {
        let start = self
            .base
            .checked_add(u64::from(self.first_bus) << 20)
            .ok_or(Error::Address)?;
        let end = self
            .base
            .checked_add((u64::from(self.last_bus) + 1) << 20)
            .ok_or(Error::Address)?;
        Ok(start..end)
    }
}

#[derive(Debug)]
pub struct Inventory {
    pub revision: u8,
    pub xsdt: bool,
    pub tables: Vec<[u8; 4]>,
    pub ecam: Vec<Ecam>,
    #[cfg(any(test, all(target_arch = "x86_64", feature = "vtd-lab")))]
    pub(crate) q35_dmar: Option<Q35Dmar>,
    #[cfg(all(target_arch = "x86_64", feature = "net-q35-lab"))]
    pub(crate) q35_net: Option<Q35Dmar>,
}

type Result<T> = core::result::Result<T, Error>;

fn field<const N: usize>(bytes: &[u8], offset: usize) -> Result<[u8; N]> {
    bytes
        .get(offset..)
        .and_then(|tail| tail.first_chunk())
        .copied()
        .ok_or(Error::Length)
}

fn u16_at(bytes: &[u8], offset: usize) -> Result<u16> {
    Ok(u16::from_le_bytes(field(bytes, offset)?))
}
fn u32_at(bytes: &[u8], offset: usize) -> Result<u32> {
    Ok(u32::from_le_bytes(field(bytes, offset)?))
}
fn u64_at(bytes: &[u8], offset: usize) -> Result<u64> {
    Ok(u64::from_le_bytes(field(bytes, offset)?))
}

fn checksum(bytes: &[u8]) -> Result<()> {
    if bytes.iter().fold(0u8, |sum, &b| sum.wrapping_add(b)) != 0 {
        return Err(Error::Checksum);
    }
    Ok(())
}

fn span(address: u64, len: usize) -> Result<Range<u64>> {
    if address == 0 {
        return Err(Error::Address);
    }
    Ok(address..address.checked_add(len as u64).ok_or(Error::Address)?)
}

fn overlaps(a: &Range<u64>, b: &Range<u64>) -> bool {
    a.start < b.end && b.start < a.end
}

fn read_at(
    read: &mut impl FnMut(u64, &mut [u8]) -> bool,
    address: u64,
    out: &mut [u8],
) -> Result<()> {
    span(address, out.len())?;
    if !read(address, out) {
        return Err(Error::Address);
    }
    Ok(())
}

fn read_table(
    read: &mut impl FnMut(u64, &mut [u8]) -> bool,
    address: u64,
    occupied: &mut Vec<Range<u64>>,
    budget: &mut usize,
) -> Result<Vec<u8>> {
    let mut header = [0; HEADER];
    read_at(read, address, &mut header)?;
    let length = u32_at(&header, 4)? as usize;
    if length < HEADER {
        return Err(Error::Length);
    }
    if length > MAX_TABLE || length > *budget {
        return Err(Error::Limit);
    }
    let range = span(address, length)?;
    if occupied.iter().any(|other| overlaps(other, &range)) {
        return Err(Error::Overlap);
    }
    occupied.try_reserve(1).map_err(|_| Error::NoMemory)?;
    occupied.push(range);
    *budget -= length;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(length)
        .map_err(|_| Error::NoMemory)?;
    bytes.resize(length, 0);
    // Prendre un cliché puis interpréter exactement les octets vérifiés.
    for (index, chunk) in bytes.chunks_mut(4096).enumerate() {
        read_at(read, address + (index * 4096) as u64, chunk)?;
    }
    if bytes[..HEADER] != header {
        return Err(Error::Format);
    }
    checksum(&bytes)?;
    Ok(bytes)
}

/// Les plages admises par `read` sont des plages firmware, jamais des registres.
/// Un appel échoué n'expose ni fenêtre MCFG ni inventaire partiel.
pub fn discover(rsdp_phys: u64, mut read: impl FnMut(u64, &mut [u8]) -> bool) -> Result<Inventory> {
    let mut legacy = [0; 20];
    read_at(&mut read, rsdp_phys, &mut legacy)?;
    if &legacy[..8] != b"RSD PTR " {
        return Err(Error::Signature);
    }
    checksum(&legacy)?;
    let revision = legacy[15];
    let mut address = u32_at(&legacy, 16)? as u64;
    let mut xsdt = false;
    let mut rsdp_length = legacy.len();
    match revision {
        0 => (),
        2.. => {
            let mut extended = [0; 36];
            read_at(&mut read, rsdp_phys, &mut extended)?;
            rsdp_length = u32_at(&extended, 20)? as usize;
            if !(36..=256).contains(&rsdp_length) {
                return Err(Error::Length);
            }
            let mut snapshot = [0; 256];
            read_at(&mut read, rsdp_phys, &mut snapshot[..rsdp_length])?;
            if snapshot[..20] != legacy || snapshot[..36] != extended {
                return Err(Error::Format);
            }
            checksum(&snapshot[..rsdp_length])?;
            let xaddress = u64_at(&snapshot, 24)?;
            if xaddress != 0 {
                address = xaddress;
                xsdt = true;
            }
        }
        _ => return Err(Error::Format),
    }
    let mut occupied = Vec::new();
    occupied.try_reserve(2).map_err(|_| Error::NoMemory)?;
    occupied.push(span(rsdp_phys, rsdp_length)?);
    let mut budget = TOTAL_BYTES;
    let root = read_table(&mut read, address, &mut occupied, &mut budget)?;
    let expected = if xsdt { b"XSDT" } else { b"RSDT" };
    if &root[..4] != expected {
        return Err(Error::Signature);
    }
    let width = if xsdt { 8 } else { 4 };
    let entries = &root[HEADER..];
    if entries.is_empty() || !entries.len().is_multiple_of(width) {
        return Err(Error::Length);
    }
    let count = entries.len() / width;
    if count > MAX_TABLES {
        return Err(Error::Limit);
    }
    let mut inventory = Inventory {
        revision,
        xsdt,
        tables: Vec::new(),
        ecam: Vec::new(),
        #[cfg(any(test, all(target_arch = "x86_64", feature = "vtd-lab")))]
        q35_dmar: None,
    #[cfg(all(target_arch = "x86_64", feature = "net-q35-lab"))]
    q35_net: None,
    };
    inventory
        .tables
        .try_reserve_exact(count)
        .map_err(|_| Error::NoMemory)?;
    for pointer in entries.chunks_exact(width) {
        let child = if xsdt {
            u64_at(pointer, 0)?
        } else {
            u32_at(pointer, 0)? as u64
        };
        let table = read_table(&mut read, child, &mut occupied, &mut budget)?;
        let sig = field(&table, 0)?;
        if !sig.iter().all(|b| (0x20..=0x7e).contains(b))
            || matches!(&sig, b"XSDT" | b"RSDT" | b"FACS" | b"DSDT")
        {
            return Err(Error::Signature);
        }
        if matches!(
            &sig,
            b"APIC" | b"MCFG" | b"FACP" | b"DMAR" | b"IORT" | b"SRAT" | b"SLIT"
        ) && inventory.tables.contains(&sig)
        {
            return Err(Error::Format);
        }
        tables::inspect(&sig, &table, &mut inventory.ecam)?;
        #[cfg(any(test, all(target_arch = "x86_64", feature = "vtd-lab")))]
        if &sig == b"DMAR" {
            inventory.q35_dmar = q35_dmar::recognize(&table);
            #[cfg(feature = "net-q35-lab")]
            { inventory.q35_net = q35_dmar::recognize_slot(&table, 7); }
        }
        inventory.tables.push(sig);
    }
    if !inventory.tables.contains(b"FACP") {
        return Err(Error::Format);
    }
    for window in &inventory.ecam {
        let aperture = window.physical()?;
        if occupied.iter().any(|r| overlaps(r, &aperture)) {
            return Err(Error::Overlap);
        }
    }
    Ok(inventory)
}

#[cfg(test)]
#[path = "acpi/tests.rs"]
mod tests;
