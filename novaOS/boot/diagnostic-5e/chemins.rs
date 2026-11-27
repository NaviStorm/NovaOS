//! Descriptions en RAM déjà publiées par UEFI. Le protocole PCI I/O sert seulement
//! de critère de recherche de handles. Aucune de ses méthodes n'est appelée.
use crate::firmware::Map;
use alloc::vec::Vec;
use core::{mem, ptr};
use r_efi::{
    efi,
    protocols::{device_path, loaded_image, pci_io},
};
use sha2::{Digest, Sha256};

fn visit(
    bs: &efi::BootServices,
    handle: efi::Handle,
    map: &Map,
    mut node: impl FnMut(&[u8]) -> bool,
) -> bool {
    let mut raw = ptr::null_mut();
    // Les handles viennent du service LocateHandle ou du LoadedImage courant.
    let s = unsafe { (bs.handle_protocol)(handle, &mut { device_path::PROTOCOL_GUID }, &mut raw) };
    if s != efi::Status::SUCCESS {
        return false;
    }
    let mut addr = raw as u64;
    let origin = addr;
    let storage = |k| matches!(k, efi::BOOT_SERVICES_DATA | efi::LOADER_DATA);
    let mut copy = [0u8; 1024];
    for _ in 0..64 {
        if !map.covers(addr, 4, storage) {
            return false;
        }
        let hdr = unsafe { ptr::read_unaligned(addr as *const [u8; 4]) };
        let n = usize::from(u16::from_le_bytes([hdr[2], hdr[3]]));
        if !(4..=copy.len()).contains(&n)
            || addr - origin + n as u64 > 4096
            || !map.covers(addr, n, storage)
        {
            return false;
        }
        unsafe {
            ptr::copy_nonoverlapping(addr as *const u8, copy.as_mut_ptr(), n);
        }
        if copy[..4] != hdr {
            return false;
        }
        if hdr[0] == 0x7f {
            return hdr[1] == 0xff && n == 4;
        }
        if !node(&copy[..n]) {
            return false;
        }
        addr += n as u64;
    }
    false
}
pub fn usb(bs: &efi::BootServices, image: &loaded_image::Protocol, map: &Map) -> bool {
    let mut found = false;
    let valid = visit(bs, image.device_handle, map, |n| match (n[0], n[1]) {
        (1, 1) if !found && n.len() == 6 => true,
        (2, 1) if !found && n.len() == 12 => true,
        (2, 2) if !found && n.len() >= 16 => true,
        (3, 5) if n.len() == 6 => {
            found = true;
            true
        }
        (4, 1) if found && n.len() == 42 => true,
        _ => false,
    });
    valid && found
}

#[derive(Default, Clone, Copy)]
#[repr(C)]
pub struct Location {
    pub depth: u8,
    pub device: u8,
    pub function: u8,
    pub reserved: u8,
    pub digest: [u8; 32],
}

pub fn pci(bs: &efi::BootServices, map: &Map, max: usize) -> Result<Vec<Location>, u64> {
    let mut handles = [ptr::null_mut(); 64];
    let mut bytes = mem::size_of_val(&handles);
    let s = unsafe {
        (bs.locate_handle)(
            efi::BY_PROTOCOL,
            &mut { pci_io::PROTOCOL_GUID },
            ptr::null_mut(),
            &mut bytes,
            handles.as_mut_ptr(),
        )
    };
    if s == efi::Status::NOT_FOUND {
        return Ok(Vec::new());
    }
    let width = mem::size_of::<efi::Handle>();
    if s != efi::Status::SUCCESS
        || !bytes.is_multiple_of(width)
        || bytes / width > max.min(handles.len())
    {
        return Err(15);
    }
    let mut locations = Vec::new();
    locations
        .try_reserve_exact(bytes / width)
        .map_err(|_| 15u64)?;
    for &handle in &handles[..bytes / width] {
        let mut loc = Location::default();
        let mut ends_with_pci = false;
        let mut hasher = Sha256::new();
        let valid = visit(bs, handle, map, |n| {
            hasher.update(n);
            ends_with_pci = n[0..2] == [1, 1];
            if ends_with_pci {
                if n.len() != 6 || n[4] > 7 || n[5] > 31 {
                    return false;
                }
                loc.depth += 1;
                loc.function = n[4];
                loc.device = n[5];
            }
            true
        });
        if !valid || !ends_with_pci {
            return Err(15);
        }
        loc.digest = hasher.finalize().into();
        locations.push(loc);
    }
    locations.sort_unstable_by_key(|l| l.digest);
    if locations.windows(2).any(|p| p[0].digest == p[1].digest) {
        return Err(15);
    }
    Ok(locations)
}
