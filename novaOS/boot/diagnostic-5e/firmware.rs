use alloc::vec::Vec;
use core::{mem, ptr, slice};
use r_efi::{efi, protocols::loaded_image};

const GLOBAL: efi::Guid = efi::Guid::from_fields(
    0x8be4df61,
    0x93ca,
    0x11d2,
    0xaa,
    0x0d,
    &[0x00, 0xe0, 0x98, 0x03, 0x2b, 0x8c],
);
const MAP_BYTES: usize = 65536;
#[repr(align(8))]
struct Storage([u8; MAP_BYTES]);
static mut STORAGE: Storage = Storage([0; MAP_BYTES]);

struct Span {
    start: u64,
    end: u64,
    kind: u32,
    readable: bool,
}
pub struct Map {
    spans: Vec<Span>,
}
impl Map {
    pub(super) fn covers(&self, start: u64, count: usize, kind: impl Fn(u32) -> bool) -> bool {
        let Some(end) = start.checked_add(count as u64) else {
            return false;
        };
        if start == 0 || end > 1 << 47 || count == 0 {
            return false;
        }
        let mut cursor = start;
        for s in &self.spans[self.spans.partition_point(|s| s.end <= start)..] {
            if s.start > cursor || !s.readable || !kind(s.kind) {
                return false;
            }
            if end <= s.end {
                return true;
            }
            cursor = s.end;
        }
        false
    }
    pub fn copy_acpi(&self, addr: u64, out: &mut [u8]) -> bool {
        if !self.covers(addr, out.len(), |k| {
            matches!(k, efi::ACPI_RECLAIM_MEMORY | efi::ACPI_MEMORY_NVS)
        }) {
            return false;
        }
        unsafe {
            ptr::copy_nonoverlapping(addr as *const u8, out.as_mut_ptr(), out.len());
        }
        true
    }
}
fn raw_map(bs: &efi::BootServices) -> Result<(usize, usize, usize), u64> {
    let mut n = MAP_BYTES;
    let (mut key, mut stride, mut version) = (0, 0, 0);
    let p = unsafe { (&raw mut STORAGE.0).cast::<u8>() };
    let status =
        unsafe { (bs.get_memory_map)(&mut n, p.cast(), &mut key, &mut stride, &mut version) };
    if status != efi::Status::SUCCESS
        || version != 1
        || !(mem::size_of::<efi::MemoryDescriptor>()..=256).contains(&stride)
        || n == 0
        || n > MAP_BYTES
        || !n.is_multiple_of(stride)
    {
        return Err(3);
    }
    Ok((n, stride, key))
}
pub fn map(bs: &efi::BootServices) -> Result<Map, u64> {
    let (n, stride, _) = raw_map(bs)?;
    let raw = unsafe { slice::from_raw_parts((&raw const STORAGE.0).cast::<u8>(), n) };
    let mut spans = Vec::new();
    spans.try_reserve_exact(n / stride).map_err(|_| 3u64)?;
    for entry in raw.chunks_exact(stride) {
        let d = unsafe { ptr::read_unaligned(entry.as_ptr().cast::<efi::MemoryDescriptor>()) };
        let start = d.physical_start;
        let end = d
            .number_of_pages
            .checked_mul(4096)
            .and_then(|n| start.checked_add(n))
            .ok_or(3u64)?;
        if start >= end || start % 4096 != 0 {
            return Err(3);
        }
        spans.push(Span {
            start,
            end,
            kind: d.r#type,
            readable: d.attribute & efi::MEMORY_RP == 0,
        });
    }
    spans.sort_unstable_by_key(|s| s.start);
    if spans.windows(2).any(|p| p[0].end > p[1].start) {
        return Err(3);
    }
    Ok(Map { spans })
}
/// Dernier appel firmware du noyau de diagnostic. Une clé périmée permet une seule relecture.
#[inline(never)]
pub unsafe fn exit(bs: &efi::BootServices, handle: efi::Handle) -> bool {
    for _ in 0..2 {
        let Ok((_, _, key)) = raw_map(bs) else {
            return false;
        };
        let s = unsafe { (bs.exit_boot_services)(handle, key) };
        if s == efi::Status::SUCCESS {
            return true;
        }
        if s != efi::Status::INVALID_PARAMETER {
            return false;
        }
    }
    false
}
pub unsafe fn rsdp(st: &efi::SystemTable, map: &Map) -> Result<u64, u64> {
    let n = st.number_of_table_entries;
    let p = st.configuration_table;
    if !(1..=1024).contains(&n)
        || !(p as usize).is_multiple_of(mem::align_of::<efi::ConfigurationTable>())
        || !map.covers(
            p as u64,
            n * mem::size_of::<efi::ConfigurationTable>(),
            |k| {
                matches!(
                    k,
                    efi::BOOT_SERVICES_DATA | efi::RUNTIME_SERVICES_DATA | efi::ACPI_RECLAIM_MEMORY
                )
            },
        )
    {
        return Err(4);
    }
    let mut found = [None, None];
    for t in unsafe { slice::from_raw_parts(p, n) } {
        let i = if t.vendor_guid == efi::ACPI_20_TABLE_GUID {
            1
        } else if t.vendor_guid == efi::ACPI_10_TABLE_GUID {
            0
        } else {
            continue;
        };
        if found[i].replace(t.vendor_table as u64).is_some() {
            return Err(4);
        }
    }
    found[1].or(found[0]).ok_or(4)
}
fn flag(rt: &efi::RuntimeServices, name: &str) -> Result<Option<u8>, ()> {
    let mut wide = [0u16; 16];
    for (to, c) in wide.iter_mut().zip(name.bytes()) {
        *to = u16::from(c);
    }
    let mut byte = 0u8;
    let mut n = 1;
    let s = unsafe {
        (rt.get_variable)(
            wide.as_mut_ptr(),
            &mut { GLOBAL },
            ptr::null_mut(),
            &mut n,
            (&raw mut byte).cast(),
        )
    };
    if s == efi::Status::NOT_FOUND {
        return Ok(None);
    }
    if s == efi::Status::SUCCESS && n == 1 {
        Ok(Some(byte))
    } else {
        Err(())
    }
}

#[cfg(not(feature = "diagnostic-5h"))]
pub fn secure(rt: &efi::RuntimeServices) -> bool {
    flag(rt, "SecureBoot") == Ok(Some(1)) && flag(rt, "SetupMode") == Ok(Some(0))
}

#[cfg(feature = "diagnostic-5h")]
pub fn trust(rt: &efi::RuntimeServices) -> crate::phase5h::Trust {
    use crate::phase5h::Trust;
    match (
        flag(rt, "SecureBoot"),
        flag(rt, "SetupMode"),
        flag(rt, "AuditMode"),
    ) {
        (Ok(s), Ok(m), Ok(a)) => Trust::from_flags(s, m, a),
        _ => Trust::Unknown,
    }
}
fn load_policy(revision: u32, options: u32) -> bool {
    revision == loaded_image::REVISION && options == 0
}
pub unsafe fn image(
    bs: &efi::BootServices,
    handle: efi::Handle,
) -> Result<&loaded_image::Protocol, u64> {
    let mut p = ptr::null_mut();
    let s = unsafe { (bs.handle_protocol)(handle, &mut { loaded_image::PROTOCOL_GUID }, &mut p) };
    if s != efi::Status::SUCCESS || p.is_null() {
        return Err(2);
    }
    let image = unsafe { &*p.cast::<loaded_image::Protocol>() };
    if !load_policy(image.revision, image.load_options_size) {
        return Err(2);
    }
    Ok(image)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn options_efi_externes_refusees() {
        assert!(load_policy(loaded_image::REVISION, 0));
        for size in [1, 2, 4096, u32::MAX] {
            assert!(!load_policy(loaded_image::REVISION, size));
        }
        assert!(!load_policy(0, 0));
    }
    fn region(start: u64, end: u64, kind: u32, readable: bool) -> Span {
        Span {
            start,
            end,
            kind,
            readable,
        }
    }
    #[test]
    fn une_plage_acpi_doit_etre_entierement_lisible() {
        let m = Map {
            spans: alloc::vec![
                region(4096, 8192, efi::ACPI_RECLAIM_MEMORY, true),
                region(8192, 12288, efi::ACPI_MEMORY_NVS, true),
                region(12288, 16384, efi::BOOT_SERVICES_DATA, true),
                region(20480, 24576, efi::ACPI_MEMORY_NVS, false),
            ],
        };
        let acpi = |k| matches!(k, efi::ACPI_RECLAIM_MEMORY | efi::ACPI_MEMORY_NVS);
        assert!(m.covers(8190, 4, acpi));
        for (base, len) in [
            (0, 4),
            (8192, 0),
            (12286, 4),
            (16382, 4100),
            (20480, 1),
            (u64::MAX, 2),
        ] {
            assert!(!m.covers(base, len, acpi));
        }
    }
}
