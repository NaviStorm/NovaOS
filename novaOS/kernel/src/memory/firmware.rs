//! Lectures temporaires du BSP avant lancement des AP et du premier processus.
//! L'adresse virtuelle n'est ni conservée ni communiquée à l'appelant.
use super::*;
const SCRATCH: u64 = IO_BASE + 0x0200_0000;

#[path = "firmware_pages.rs"]
mod firmware_pages;

fn page_authorized(m: &Memory, page: u64, accepts: impl Fn(EntryType) -> bool) -> bool {
    firmware_pages::authorized(
        page,
        PAGE,
        m.frames
            .regions()
            .map(|e| (e.base, e.length, accepts(e.entry_type))),
    )
}

fn read_page<T>(m: &mut Memory, page: u64, attr: u64, read: impl FnOnce(usize) -> T) -> Result<T> {
    if page > PHYS_MASK
        || page % PAGE != 0
        || crate::smp::ONLINE.load(core::sync::atomic::Ordering::Acquire) != 1
    {
        return Err(Error::Geometry);
    }
    let leaf = paging::leaf(page, Access::ReadOnly, attr);
    m.space.insert(&mut m.frames, SCRATCH, leaf)?;
    paging::publish();
    paging::invalidate(SCRATCH);
    let value = read(SCRATCH as usize);
    if m.space.remove(&mut m.frames, SCRATCH)? != page {
        return Err(Error::Verification);
    }
    Ok(value)
}

pub(crate) fn copy_acpi(mut address: u64, mut out: &mut [u8]) -> bool {
    while !out.is_empty() {
        let page = address & !(PAGE - 1);
        let offset = (address - page) as usize;
        let n = out.len().min(PAGE as usize - offset);
        if page > PHYS_MASK
            || crate::heap::with_pages(|m| {
                if !page_authorized(m, page, |kind| {
                    matches!(kind, EntryType::ACPI_RECLAIMABLE | EntryType::ACPI_NVS)
                }) {
                    return Err(Error::Reserved);
                }
                read_page(m, page, m.cache, |va| {
                    // Sûreté : la page firmware entière est autorisée et mappée RO/NX.
                    // La plage de destination privée ne recouvre pas le firmware.
                    unsafe {
                        core::ptr::copy_nonoverlapping(
                            (va + offset) as *const u8,
                            out.as_mut_ptr(),
                            n,
                        )
                    }
                })
            })
            .is_err()
        {
            return false;
        }
        address += n as u64;
        out = &mut out[n..];
    }
    true
}

#[cfg(target_arch = "aarch64")]
pub(crate) fn virt_config_dword(address: u64) -> Option<u32> {
    // Fenêtre du profil QEMU virt haut autorisé indépendamment des pointeurs MCFG.
    const ECAM: u64 = 0x0040_1000_0000;
    if !(ECAM..ECAM + (256 << 20)).contains(&address) || address & 3 != 0 {
        return None;
    }
    let page = address & !(PAGE - 1);
    let off = (address - page) as usize;
    crate::heap::with_pages(|m| {
        // Une fenêtre de configuration chevauchant RAM/firmware/framebuffer est refusée.
        for e in m.frames.regions() {
            let end = e.base.checked_add(e.length).ok_or(Error::Geometry)?;
            if e.base < page + PAGE && page < end && e.entry_type != EntryType::RESERVED {
                return Err(Error::Reserved);
            }
        }
        read_page(m, page, paging::device_cache()?, |va| {
            // Sûreté : dword aligné à l'intérieur d'une seule page ECAM Device RO.
            unsafe { ((va + off) as *const u32).read_volatile() }
        })
    })
    .ok()
}

/// Pointeur HHDM rendu par la requête DTB, valable uniquement avant le handoff.
/// Vérifier chaque page avant la copie, y compris si le DTB en couvre plusieurs.
#[cfg(target_arch = "aarch64")]
pub(crate) fn copy_loader_blob(address: usize, out: &mut [u8]) -> Result<()> {
    crate::heap::with_pages(|m| {
        let physical = (address as u64)
            .checked_sub(m.frames.info.offset)
            .ok_or(Error::Geometry)?;
        let end = physical
            .checked_add(out.len() as u64)
            .ok_or(Error::Geometry)?;
        if end > PHYS_MASK + PAGE || address.checked_add(out.len()).is_none() {
            return Err(Error::Geometry);
        }
        for page in ((physical & !(PAGE - 1))..end).step_by(PAGE as usize) {
            if !page_authorized(m, page, |kind| kind == EntryType::BOOTLOADER_RECLAIMABLE) {
                return Err(Error::Reserved);
            }
            m.space.direct_alias(&m.frames, page)?;
        }
        // Sûreté : les alias HHDM ont été vérifiés, les cadres du chargeur sont
        // encore réservés ; le tampon appartient au tas privé du noyau.
        unsafe {
            core::ptr::copy_nonoverlapping(address as *const u8, out.as_mut_ptr(), out.len())
        };
        Ok(())
    })
}
