//! Capture en lecture seule pour le profil de diagnostic QEMU explicitement choisi.
use super::{acpi, pci_inventory as pci};
use crate::{boot, hal::log, memory::firmware};
use alloc::vec::Vec;
#[allow(dead_code)]
#[path = "../../../user/device_inventory.rs"]
mod wire;

struct Config;
impl pci::ConfigRead for Config {
    fn read32(&mut self, a: pci::Address, off: u16) -> Option<u32> {
        if a.segment != 0 || a.device > 31 || a.function > 7 || off & 3 != 0 || off >= 256 {
            return None;
        }
        let bdf = u16::from(a.bus) << 8 | u16::from(a.device) << 3 | u16::from(a.function);
        #[cfg(target_arch = "x86_64")]
        {
            Some(super::pci_ports::port(bdf, usize::from(off), 4, None))
        }
        #[cfg(target_arch = "aarch64")]
        {
            firmware::virt_config_dword(0x0040_1000_0000 + (u64::from(bdf) << 12) + u64::from(off))
        }
    }
}

pub(crate) fn capture(qemu_profile: bool) -> Option<Vec<u8>> {
    let inventory = match boot::rsdp_physical() {
        Some(pa) => match acpi::discover(pa, firmware::copy_acpi) {
            Ok(info) => Some(info),
            Err(e) => {
                log(format_args!("ACPI refuse {e:?}"));
                return None;
            }
        },
        None => {
            #[cfg(target_arch = "aarch64")]
            log(format_args!(
                "ACPI absent dtb={}",
                boot::device_tree_address().is_some()
            ));
            #[cfg(target_arch = "x86_64")]
            log(format_args!("ACPI absent"));
            None
        }
    };
    let mut data = Vec::new();
    data.try_reserve_exact(wire::PRIVATE_PAGE).ok()?;
    data.extend_from_slice(wire::MAGIC);
    data.resize(wire::HEADER, 0);
    let mut root = None;
    if let Some(info) = inventory {
        log(format_args!(
            "ACPI revision={} xsdt={} tables={}",
            info.revision,
            info.xsdt,
            info.tables.len()
        ));
        data[5] = u8::try_from(info.tables.len()).ok()?;
        data[6] = info.revision;
        data[7] = u8::from(info.xsdt);
        for name in &info.tables {
            log(format_args!(
                "ACPI table {}",
                core::str::from_utf8(name).ok()?
            ));
            data.extend_from_slice(name);
        }
        for e in &info.ecam {
            log(format_args!(
                "MCFG segment={} bus={}..{} base={:#x}",
                e.segment, e.first_bus, e.last_bus, e.base
            ));
        }
        if let Some(e) = info
            .ecam
            .iter()
            .find(|e| e.segment == 0 && e.first_bus == 0)
        {
            #[cfg(target_arch = "aarch64")]
            let allowed = e.base == 0x0040_1000_0000;
            #[cfg(target_arch = "x86_64")]
            let allowed = true; // CF8/CFC, la base MCFG n'est pas utilisée.
            if allowed {
                root = Some(e.last_bus);
            }
        }
    } else if qemu_profile {
        #[cfg(target_arch = "aarch64")]
        if let Some(window) = firmware_tree() {
            log(format_args!(
                "DTB ECAM verifie base={:#x} bus={}..{}",
                window.base, window.first_bus, window.last_bus
            ));
            root = Some(window.last_bus);
        }
    }
    if !qemu_profile {
        log(format_args!("PCI acces non attribue"));
        return Some(data);
    }
    let Some(last_bus) = root else {
        log(format_args!("PCI racine QEMU non etablie"));
        return None;
    };
    let mut complete = true;
    let count = pci::enumerate(
        &mut Config,
        pci::BusRange {
            segment: 0,
            first: 0,
            last: last_bus,
        },
        |a, result| match result {
            Ok(f) => {
                if usize::from(data[4]) == wire::MAX_DEVICES {
                    complete = false;
                    return;
                }
                let mut bars = 0u8;
                for (i, bar) in f.bars.iter().enumerate() {
                    if let Some(b) = bar {
                        bars |= 1 << i;
                        log(format_args!(
                            "BAR {:04x}:{:02x}:{:02x}.{} n={} type={:?} base={:#x} taille=inconnue",
                            a.segment, a.bus, a.device, a.function, i, b.kind, b.base
                        ));
                    }
                }
                let flags = u8::from(f.msi) | u8::from(f.msix) << 1 | u8::from(f.pcie) << 2;
                log(format_args!(
                    "PCI {:04x}:{:02x}:{:02x}.{} {:04x}:{:04x} classe={:02x}{:02x}{:02x} msi={} msix={} pcie={} pin={} ligne={} master={} etat=inventorie pilote=non-attribue modele={}",
                    a.segment,
                    a.bus,
                    a.device,
                    a.function,
                    f.vendor,
                    f.device,
                    f.class,
                    f.subclass,
                    f.interface,
                    f.msi,
                    f.msix,
                    f.pcie,
                    f.irq_pin,
                    f.irq_line,
                    f.bus_master,
                    wire::model(f.vendor, f.device)
                        .and_then(|(_, name)| core::str::from_utf8(name).ok())
                        .unwrap_or("inconnu")
                ));
                data.extend_from_slice(&a.segment.to_le_bytes());
                data.extend_from_slice(&[a.bus, a.device << 3 | a.function]);
                data.extend_from_slice(&f.vendor.to_le_bytes());
                data.extend_from_slice(&f.device.to_le_bytes());
                data.extend_from_slice(&[
                    f.class,
                    f.subclass,
                    f.interface,
                    bars,
                    f.irq_pin,
                    flags,
                    f.irq_line,
                    f.header,
                ]);
                data[4] += 1;
            }
            Err(e) => {
                complete = false;
                log(format_args!("PCI fonction refusee {a:?} {e:?}"));
            }
        },
    );
    log(format_args!(
        "MATERIEL fonctions={count:?} inventaire_complet={complete}"
    ));
    (count.is_ok() && complete).then_some(data)
}

#[cfg(target_arch = "aarch64")]
fn firmware_tree() -> Option<super::virt_dtb::Window> {
    let address = boot::device_tree_address()?;
    let mut header = [0; 40];
    firmware::copy_loader_blob(address, &mut header).ok()?;
    let size = super::virt_dtb::declared_size(&header).ok()?;
    let mut blob = Vec::new();
    blob.try_reserve_exact(size).ok()?;
    blob.resize(size, 0);
    firmware::copy_loader_blob(address, &mut blob).ok()?;
    super::virt_dtb::ecam(&blob).ok()
}
