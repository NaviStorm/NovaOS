#![no_std]
#![no_main]
extern crate alloc;
use core::{arch::asm, panic::PanicInfo, ptr};
use r_efi::efi;

#[allow(dead_code, unexpected_cfgs)]
#[path = "../../kernel/src/hardware/acpi.rs"]
mod acpi;
mod catalogue;
mod chemins;
mod firmware;
mod memoire;
#[cfg(feature = "diagnostic-5h")]
mod phase5h;

#[global_allocator]
static HEAP: memoire::Heap = memoire::Heap;

// Schéma 1, génération de livraison 5, origine laboratoire 1, activation 0,
// plancher de catalogue 3. Les octets appartiennent à une section PE signée.
static CONFIG: [u8; 16] = *b"NOVA5E-CFG!\x01\x05\x01\x00\x03";
const MAX: usize = 64;
#[derive(Clone, Copy, Default)]
#[repr(C)]
struct Node {
    kind: u8,
    compatible: u8,
    selected: u8,
    activation: u8,
    digest: [u8; 32],
    location: chemins::Location,
}
#[repr(C)]
struct Report {
    magic: [u8; 16],
    address: u64,
    entry: u64,
    stop: u64,
    image: u64,
    status: u64,
    tables: u64,
    dmar: u64,
    units: u64,
    nodes: u64,
    arena: u64,
    live: u64,
    exit: u64,
    details: [Node; MAX],
}
static mut REPORT: Report = unsafe { core::mem::zeroed() };

fn inventory(st: &efi::SystemTable, handle: efi::Handle, r: &mut Report) -> Result<(), u64> {
    let rt = unsafe { st.runtime_services.as_ref() };
    #[cfg(not(feature = "diagnostic-5h"))]
    if !rt.is_some_and(firmware::secure) {
        return Err(1);
    }
    #[cfg(feature = "diagnostic-5h")]
    {
        let trust = rt.map_or(phase5h::Trust::Unknown, firmware::trust);
        phase5h::display(st, trust.message())?;
        if trust == phase5h::Trust::Unknown {
            return Err(1);
        }
    }
    let cfg = unsafe { ptr::read_volatile(&CONFIG) };
    if &cfg[..12] != b"NOVA5E-CFG!\x01"
        || cfg[12] != 5
        || cfg[13] != 1
        || cfg[14] != 0
        || cfg[15] < 3
    {
        return Err(11);
    }
    let bs = unsafe { st.boot_services.as_ref() }.ok_or(2u64)?;
    let img = unsafe { firmware::image(bs, handle) }?;
    r.image = img.image_base as u64;
    let map = firmware::map(bs)?;
    if !chemins::usb(bs, img, &map) {
        return Err(9);
    }
    let rsdp = unsafe { firmware::rsdp(st, &map) }?;
    let mut kinds = alloc::vec::Vec::new();
    let mut units = 0;
    #[cfg(feature = "diagnostic-5h")]
    let mut remappers = phase5h::Remappers::default();
    let inv = acpi::discover_with(
        rsdp,
        |a, b| map.copy_acpi(a, b),
        |sig, table| {
            let mut add = |kind| {
                if kinds.len() == MAX {
                    return Err(acpi::Error::Limit);
                }
                kinds.try_reserve(1).map_err(|_| acpi::Error::NoMemory)?;
                kinds.push(kind);
                Ok(())
            };
            match &sig {
                b"APIC" => {
                    let mut tail = &table[44..];
                    while !tail.is_empty() {
                        if tail[0] == 1 {
                            add(1)?;
                        }
                        tail = &tail[usize::from(tail[1])..];
                    }
                }
                b"HPET" => {
                    if table.len() < 56 {
                        return Err(acpi::Error::Length);
                    }
                    add(2)?;
                }
                b"MCFG" => {
                    for _ in table[44..].chunks_exact(16) {
                        add(3)?;
                    }
                }
                b"DMAR" => {
                    let mut tail = &table[48..];
                    while !tail.is_empty() {
                        if tail[0..2] == [0, 0] {
                            #[cfg(feature = "diagnostic-5h")]
                            remappers
                                .record(
                                    &tail[..usize::from(u16::from_le_bytes([tail[2], tail[3]]))],
                                )
                                .map_err(|_| acpi::Error::Format)?;
                            units += 1;
                            add(4)?;
                        }
                        tail = &tail[usize::from(u16::from_le_bytes([tail[2], tail[3]]))..];
                    }
                }
                _ => {}
            }
            Ok(())
        },
    )
    .map_err(|_| 5u64)?;
    let locations = chemins::pci(bs, &map, MAX - kinds.len())?;
    let first_pci = kinds.len();
    kinds.try_reserve(locations.len()).map_err(|_| 15u64)?;
    kinds.resize(first_pci + locations.len(), 5);
    let policy = catalogue::policy(u32::from(cfg[15])).map_err(|_| 6u64)?;
    let cat = catalogue::catalog(&policy).map_err(|e| match e {
        novaos_catalogue::Error::Floor => 12u64,
        novaos_catalogue::Error::Revoked => 13,
        novaos_catalogue::Error::Signature => 6,
        #[cfg(feature = "diagnostic-5h")]
        novaos_catalogue::Error::Elf => 18,
        _ => 14,
    })?;
    let mut details = [Node::default(); MAX];
    for (i, &kind) in kinds.iter().enumerate() {
        let name = match kind {
            1 => "ioapic",
            2 => "hpet",
            3 => "ecam",
            4 => "drhd",
            _ => "pci-device-path",
        };
        let node = &mut details[i];
        node.kind = kind;
        if kind == 5 {
            node.location = locations[i - first_pci];
        }
        let provider = if kind == 5 { "uefi" } else { "acpi" };
        let label = alloc::format!("{provider}.{name}.{i}");
        match catalogue::select(&cat, provider, name, &label) {
            Ok(Some(hash)) => {
                node.compatible = 1;
                node.selected = 1;
                node.digest = hash;
            }
            Ok(None) => {}
            Err(novaos_catalogue::Error::Conflict) => {
                node.compatible = 2;
            }
            Err(_) => return Err(7),
        }
    }
    r.tables = inv.tables.len() as u64;
    r.dmar = u64::from(inv.tables.contains(b"DMAR"));
    r.units = units;
    #[cfg(feature = "diagnostic-5h")]
    {
        let candidates = details.iter().filter(|n| n.selected != 0).count();
        phase5h::display(
            st,
            &alloc::format!("NovaOS: catalogue verifie - candidats={candidates}"),
        )?;
        remappers.report(st)?;
        // Aucun transfert de propriété DMA authentifié dans ce profil.
        Err(16)
    }
    #[cfg(not(feature = "diagnostic-5h"))]
    {
        r.nodes = kinds.len() as u64;
        r.details = details;
        Ok(())
    }
}
#[unsafe(no_mangle)]
pub extern "efiapi" fn efi_main(handle: efi::Handle, system: *mut efi::SystemTable) -> efi::Status {
    let r = unsafe { &mut *(&raw mut REPORT) };
    r.address = (&raw mut REPORT) as u64;
    r.entry = efi_main as *const () as u64;
    r.exit = firmware::exit as *const () as u64;
    r.stop = stop as *const () as u64;
    r.status = unsafe { system.as_ref() }
        .ok_or(1u64)
        .and_then(|st| inventory(st, handle, r))
        .err()
        .unwrap_or(0);
    let (used, live) = memoire::close();
    r.arena = used as u64;
    r.live = live as u64;
    if live != 0 {
        r.status = 8;
    }
    if let Some(bs) = unsafe { system.as_ref().and_then(|s| s.boot_services.as_ref()) } {
        if !unsafe { firmware::exit(bs, handle) } {
            r.status = 10;
        }
    }
    // Publication terminale. Aucun retour au Boot Manager, y compris au refus.
    unsafe {
        ptr::write_volatile(&mut r.magic, *b"NOVA5E-RAM-FINAL");
    }
    stop()
}
#[inline(never)]
extern "C" fn stop() -> ! {
    unsafe {
        asm!("cli", "2:", "hlt", "jmp 2b", options(noreturn, nostack));
    }
}
// En cas de panique, signal indépendant du rapport transactionnel, même sans tas.
static mut PANIC: ([u8; 16], usize) = ([0; 16], 0);
#[panic_handler]
fn panic(_: &PanicInfo) -> ! {
    let addr = (&raw mut PANIC) as usize;
    unsafe {
        ptr::write_volatile(&raw mut PANIC, (*b"NOVA5E-PANIC-RAM", addr));
    }
    stop()
}
