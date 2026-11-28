//! Observation PCI après entrée EFI. Les adresses DRHD et RMRR restent descriptives.
use crate::{acpi, phase5h::display};
use alloc::{format, vec::Vec};
use core::{mem, ptr};
use r_efi::{efi, protocols::pci_io};

use crate::topologie5i::{
    Dmar, Function, REFUS, Rid, attribution, bridge_range, possible_group, validate_bridges,
};
const MAX_HANDLES: usize = 64;
#[derive(Clone, Copy)]
enum Reg {
    Identity,
    Command,
    Class,
    Header,
    Buses,
}
impl Reg {
    fn address(self) -> (u32, pci_io::Width) {
        match self {
            Self::Identity => (0, pci_io::WIDTH_UINT32),
            Self::Command => (4, pci_io::WIDTH_UINT16),
            Self::Class => (8, pci_io::WIDTH_UINT32),
            Self::Header => (14, pci_io::WIDTH_UINT8),
            Self::Buses => (24, pci_io::WIDTH_UINT32),
        }
    }
}

fn read(p: *mut pci_io::Protocol, reg: Reg) -> Result<u32, u64> {
    let (offset, width) = reg.address();
    let mut value = 0u32;
    let s = unsafe { ((*p).pci.read)(p, width, offset, 1, (&raw mut value).cast()) };
    if s == efi::Status::SUCCESS {
        Ok(value)
    } else {
        Err(REFUS)
    }
}

pub(crate) fn functions(bs: &efi::BootServices, windows: &[acpi::Ecam]) -> Result<Vec<Function>, u64> {
    let mut handles = [ptr::null_mut(); MAX_HANDLES];
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
    if s != efi::Status::SUCCESS
        || bytes == 0
        || bytes > mem::size_of_val(&handles)
        || !bytes.is_multiple_of(mem::size_of::<efi::Handle>())
    {
        return Err(REFUS);
    }
    let mut out = Vec::<Function>::new();
    let n = bytes / mem::size_of::<efi::Handle>();
    out.try_reserve_exact(n).map_err(|_| REFUS)?;
    for &h in &handles[..n] {
        let mut raw = ptr::null_mut();
        if unsafe { (bs.handle_protocol)(h, &mut { pci_io::PROTOCOL_GUID }, &mut raw) }
            != efi::Status::SUCCESS
            || raw.is_null()
        {
            return Err(REFUS);
        }
        let p = raw.cast::<pci_io::Protocol>();
        let (mut s, mut b, mut d, mut f) = (0, 0, 0, 0);
        if unsafe { ((*p).get_location)(p, &mut s, &mut b, &mut d, &mut f) } != efi::Status::SUCCESS
            || s > 65535
            || b > 255
            || d > 31
            || f > 7
        {
            return Err(REFUS);
        }
        let id = Rid {
            seg: s as u16,
            bus: b as u8,
            dev: d as u8,
            fun: f as u8,
        };
        if windows
            .iter()
            .filter(|w| w.segment == id.seg && (w.first_bus..=w.last_bus).contains(&id.bus))
            .count()
            != 1
            || out.iter().any(|x| x.id == id)
        {
            return Err(REFUS);
        }
        let ident = read(p, Reg::Identity)?;
        let class = read(p, Reg::Class)? >> 8;
        let command = read(p, Reg::Command)? as u16;
        let header = read(p, Reg::Header)? as u8;
        if ident as u16 == 0xffff
            || header & 127 > 1
            || (header & 127 == 1) != (class >> 8 == 0x0604)
        {
            return Err(REFUS);
        }
        let bridge = if header & 127 == 1 {
            Some(bridge_range(id, read(p, Reg::Buses)?)?)
        } else {
            None
        };
        out.push(Function {
            id,
            ident,
            class,
            command,
            header,
            bridge,
        });
    }
    Ok(out)
}

pub(crate) fn observe(
    st: &efi::SystemTable,
    bs: &efi::BootServices,
    windows: &[acpi::Ecam],
    table: Option<&[u8]>,
) -> Result<(), u64> {
    let start = ticks();
    let topology = match table {
        Some(t) => Dmar::parse(t)?,
        None => Dmar::default(),
    };
    let devices = functions(bs, windows)?;
    validate_bridges(&devices)?;
    let elapsed = ticks().wrapping_sub(start);
    for w in windows {
        display(
            st,
            &format!(
                "5I ECAM seg={} base={:#x} buses={}-{}",
                w.segment, w.base, w.first_bus, w.last_bus
            ),
        )?;
    }
    for r in &topology.records {
        display(
            st,
            &format!(
                "5I {} seg={} range={:#x}-{:#x}",
                if r.kind == 1 { "RMRR" } else { "DRHD" },
                r.segment,
                r.start,
                r.end
            ),
        )?;
    }
    for (index, f) in devices.iter().enumerate() {
        let id = f.id;
        let (g, n) = possible_group(index, &devices);
        display(
            st,
            &format!(
                "5I possible-group {:04x}:{:02x}:{:02x}.{} ref={:04x}:{:02x}:{:02x}.{} count={n}",
                id.seg, id.bus, id.dev, id.fun, g.seg, g.bus, g.dev, g.fun
            ),
        )?;
        let (unit, rmrr) = match attribution(&topology, id, &devices) {
            Some((base, n)) => (
                base.map_or_else(|| "none".into(), |b| format!("{b:x}")),
                format!("{n}"),
            ),
            None => ("unknown".into(), "unknown".into()),
        };
        display(
            st,
            &format!(
                "5I RID {:04x}:{:02x}:{:02x}.{} id={:08x} cls={:06x} cmd={:04x} hdr={:02x} drhd={} rmrr={}",
                id.seg, id.bus, id.dev, id.fun, f.ident, f.class, f.command, f.header, unit, rmrr
            ),
        )?;
        if let Some((sec, sub)) = f.bridge {
            display(
                st,
                &format!(
                    "5I bridge {:04x}:{:02x}:{:02x}.{} secondary={} subordinate={}",
                    id.seg, id.bus, id.dev, id.fun, sec, sub
                ),
            )?;
        }
    }
    display(
        st,
        &format!(
            "5I scan_tsc={elapsed} functions={} aliases=unproven owner=unknown",
            devices.len()
        ),
    )?;
    Ok(())
}

#[cfg(target_arch = "x86_64")]
fn ticks() -> u64 {
    unsafe {
        core::arch::x86_64::_mm_lfence();
        core::arch::x86_64::_rdtsc()
    }
}
#[cfg(not(target_arch = "x86_64"))]
fn ticks() -> u64 {
    0
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn liste_fermee_registres_pci() {
        let got = [
            Reg::Identity,
            Reg::Command,
            Reg::Class,
            Reg::Header,
            Reg::Buses,
        ]
        .map(Reg::address);
        assert_eq!(got, [(0, 2), (4, 1), (8, 2), (14, 0), (24, 2)]);
    }
}
