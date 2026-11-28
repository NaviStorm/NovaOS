#![no_std]
#![no_main]
#![allow(unexpected_cfgs, dead_code)] // Modules communs compilés aussi dans les profils historiques.
extern crate alloc;

use alloc::{format, vec::Vec};
use core::{
    arch::{asm, x86_64::__cpuid},
    panic::PanicInfo,
    ptr,
};
use novaos_catalogue::{Bus, Error};
use novaos_vtd_multi::{Adresse, Fonction};
use r_efi::efi;

#[path = "../../kernel/src/hardware/acpi.rs"]
mod acpi;
#[path = "../diagnostic-5e/catalogue.rs"]
mod catalogue;
#[path = "../diagnostic-5e/chemins.rs"]
mod chemins;
mod firmware {
    include!("../diagnostic-5e/firmware.rs");
    impl Map {
        pub fn ram_mio(&self) -> (u64, u64) {
            let octets = |sp: &Span| sp.end - sp.start;
            let visible = self
                .spans
                .iter()
                .filter(|s| {
                    matches!(
                        s.kind,
                        efi::LOADER_DATA
                            | efi::LOADER_CODE
                            | efi::BOOT_SERVICES_DATA
                            | efi::BOOT_SERVICES_CODE
                            | efi::RUNTIME_SERVICES_CODE
                            | efi::RUNTIME_SERVICES_DATA
                            | efi::CONVENTIONAL_MEMORY
                            | efi::ACPI_MEMORY_NVS
                            | efi::ACPI_RECLAIM_MEMORY
                    )
                })
                .map(octets)
                .sum::<u64>();
            let libre = self
                .spans
                .iter()
                .filter(|s| s.kind == efi::CONVENTIONAL_MEMORY)
                .map(octets)
                .sum::<u64>();
            (visible >> 20, libre >> 20)
        }
    }
}
#[path = "../diagnostic-5e/journal5i.rs"]
mod journal;
#[path = "../diagnostic-5e/memoire.rs"]
mod memoire;
#[path = "../diagnostic-5e/phase5h.rs"]
mod phase5h;
#[path = "../diagnostic-5e/phase5i.rs"]
mod phase5i;
mod selection;
#[path = "../diagnostic-5e/topologie5i.rs"]
mod topologie5i;

#[global_allocator]
static HEAP: memoire::Heap = memoire::Heap;

fn afficher(st: &efi::SystemTable, ligne: &str) -> Result<(), u64> {
    journal::append(ligne)?;
    phase5h::display(st, ligne)
}

fn diagnostic(st: &efi::SystemTable, handle: efi::Handle) -> Result<(), u64> {
    let bs = unsafe { st.boot_services.as_ref() }.ok_or(2u64)?;
    // UEFI arme normalement le watchdog avant StartImage. Le verdict doit rester à l'écran.
    if unsafe { (bs.set_watchdog_timer)(0, 0, 0, ptr::null_mut()) } != efi::Status::SUCCESS {
        return Err(21);
    }
    let con = unsafe { st.con_out.as_ref() }.ok_or(17u64)?;
    if unsafe { (con.clear_screen)(st.con_out) } != efi::Status::SUCCESS {
        return Err(17);
    }
    afficher(st, "NovaOS | STANDARD | Diagnostic UEFI x86_64")?;
    let mode = unsafe { st.runtime_services.as_ref() }
        .map(firmware::trust)
        .unwrap_or(phase5h::Trust::Unknown);
    afficher(st, mode.message())?;
    afficher(st, "Protection DMA avant chargement : INDETERMINEE")?;
    if mode == phase5h::Trust::Unknown {
        return Err(1);
    }
    let img = unsafe { firmware::image(bs, handle) }?;
    let policy = catalogue::policy(3).map_err(|_| 6u64)?;
    let cat = catalogue::catalog(&policy).map_err(|_| 6u64)?;
    afficher(
        st,
        "Catalogue de laboratoire : signatures des manifestes et ELF verifiees",
    )?;
    let map = firmware::map(bs)?;
    let origine = if chemins::usb(bs, img, &map) {
        "USB declare par UEFI"
    } else {
        "indeterminee"
    };
    afficher(
        st,
        &format!("Origine : {origine} | sans attestation du media"),
    )?;

    let cpu = __cpuid(0);
    let mut vendor = [0u8; 12];
    for (out, value) in vendor.chunks_exact_mut(4).zip([cpu.ebx, cpu.edx, cpu.ecx]) {
        out.copy_from_slice(&value.to_le_bytes());
    }
    let nom = core::str::from_utf8(&vendor)
        .ok()
        .filter(|s| s.bytes().all(|c| c.is_ascii_graphic()))
        .unwrap_or("inconnu");
    let signature = if cpu.eax >= 1 { __cpuid(1).eax } else { 0 };
    let (visible, libre) = map.ram_mio();
    afficher(
        st,
        &format!(
            "CPU {nom} signature {signature:08x} | RAM UEFI {visible} Mio, libres {libre} Mio"
        ),
    )?;
    let mut dmar = None;
    let (mut ioapic, mut cpu_actifs) = (0, 0);
    let inv = acpi::discover_with(
        unsafe { firmware::rsdp(st, &map) }?,
        |a, b| map.copy_acpi(a, b),
        |sig, t| {
            if &sig == b"DMAR" {
                if t.len() > 16384 {
                    return Err(acpi::Error::Limit);
                }
                dmar = Some(t.to_vec());
            }
            if &sig == b"APIC" {
                let mut s = &t[44..];
                while !s.is_empty() {
                    let n = s[1] as usize;
                    let (record, rest) = s.split_at(n);
                    if record[0] == 1 {
                        ioapic += 1;
                    }
                    let offset = match record[0] {
                        0 => Some(4),
                        9 => Some(8),
                        _ => None,
                    };
                    if let Some(offset) = offset {
                        let flags = record.get(offset..offset + 4).ok_or(acpi::Error::Length)?;
                        cpu_actifs += u32::from_le_bytes(flags.try_into().unwrap()) & 1;
                    }
                    s = rest;
                }
            }
            Ok(())
        },
    )
    .map_err(|_| 5u64)?;
    let pci = phase5i::functions(bs, &inv.ecam)?;
    topologie5i::validate_bridges(&pci)?;
    afficher(
        st,
        &format!(
            "ACPI {} tables | CPU actifs MADT {} | PCIe {} fenetres, {} fonctions",
            inv.tables.len(),
            cpu_actifs,
            inv.ecam.len(),
            pci.len()
        ),
    )?;
    let fs: Vec<_> = pci
        .iter()
        .map(|f| Fonction {
            adresse: Adresse {
                segment: f.id.seg,
                rid: u16::from_be_bytes([f.id.bus, f.id.dev << 3 | f.id.fun]),
            },
            titulaire: None,
            classe: f.class,
            pont: f.bridge,
        })
        .collect();
    let mut categories = [0u32; 5];
    let mut plans = 0;
    let mut conflits = 0;
    let couverture = match dmar {
        None => {
            afficher(st, "VT-d : table DMAR absente | isolation DMA non etablie")?;
            None
        }
        Some(ref t) => match novaos_vtd_multi::decrire(t, &fs) {
            Ok(c) => {
                afficher(
                    st,
                    &format!(
                        "VT-d : {} DRHD declares | traduction et droits effectifs indetermines",
                        c.unites.len()
                    ),
                )?;
                Some(c)
            }
            Err(e) => {
                afficher(st, &format!("VT-d : attribution indeterminee ({e:?})"))?;
                None
            }
        },
    };
    for (i, f) in pci.iter().enumerate() {
        let groupe = selection::categorie(f.class);
        categories[groupe] += 1;
        if groupe < 4 && categories[groupe] == 1 {
            let nom = ["Stockage", "Reseau", "Graphique", "USB"][groupe];
            afficher(
                st,
                &format!(
                    "{nom} {:04x}:{:04x} sur {:04x}:{:02x}:{:02x}.{}",
                    f.ident as u16,
                    f.ident >> 16,
                    f.id.seg,
                    f.id.bus,
                    f.id.dev,
                    f.id.fun
                ),
            )?;
        }
        let bus = Bus::Pci {
            vendor: Some(f.ident as u16),
            device: Some((f.ident >> 16) as u16),
            subsystem_vendor: None,
            subsystem_device: None,
            revision: None,
            class_base: Some((f.class >> 16) as u8),
            subclass: Some((f.class >> 8) as u8),
        };
        let id = format!(
            "{:04x}:{:02x}:{:02x}.{}",
            f.id.seg, f.id.bus, f.id.dev, f.id.fun
        );
        let etat = match selection::select_pci(
            &cat,
            bus,
            &format!("pci.{}.{}.{}.{}", f.id.seg, f.id.bus, f.id.dev, f.id.fun),
        ) {
            Ok(Some(_)) => {
                plans += 1;
                "selectionne"
            }
            Ok(None) => "aucun",
            Err(Error::Conflict) => {
                conflits += 1;
                "conflit"
            }
            Err(_) => return Err(7),
        };
        let unite = couverture.as_ref().map(|c| c.affectations[i]);
        journal::append(&format!(
            "PCI {id} id={:08x} classe={:06x} BME={} DRHD={unite:x?} pilote={etat}",
            f.ident,
            f.class,
            (f.command >> 2) & 1
        ))?;
    }
    for (nom, n) in [
        ("ioapic", ioapic),
        ("hpet", u32::from(inv.tables.contains(b"HPET"))),
    ] {
        if n > 0 {
            match catalogue::select(&cat, "acpi", nom, nom) {
                Ok(Some(_)) => {
                    plans += n;
                    afficher(
                        st,
                        &format!("Pilote compatible : {nom} (catalogue de laboratoire)"),
                    )?;
                }
                Err(Error::Conflict) => conflits += n,
                Ok(None) => {}
                Err(_) => return Err(7),
            }
        }
    }
    let [stock, net, gpu, usb, autres] = categories;
    afficher(
        st,
        &format!(
            "Controleurs : USB {usb} | reseau {net} | graphique {gpu} | stockage {stock} | autres {autres}"
        ),
    )?;
    afficher(
        st,
        &format!("Pilotes selectionnes {plans} | conflits {conflits} | activations 0"),
    )?;
    afficher(
        st,
        "Isolation pour activation : NON ETABLIE | aucun peripherique active",
    )?;
    afficher(st, "Inventaire termine | journal en RAM | arret local")
}

// L'observateur peut repérer les deux bornes après une calibration indépendante.
#[repr(C)]
struct Bornes {
    magic: [u8; 16],
    position: u64,
    entree: u64,
    arret: u64,
}
static mut BORNES: Bornes = Bornes {
    magic: [0; 16],
    position: 0,
    entree: 0,
    arret: 0,
};
#[unsafe(no_mangle)]
pub extern "efiapi" fn efi_main(handle: efi::Handle, system: *mut efi::SystemTable) -> efi::Status {
    if let Some(st) = unsafe { system.as_ref() } {
        if let Err(code) = diagnostic(st, handle) {
            let motif = match code {
                1 => "etat Secure Boot incoherent",
                2 => "image ou services EFI",
                3 => "carte memoire EFI",
                4 => "RSDP",
                5 => "tables ACPI",
                6 => "authentification du catalogue",
                7 => "selection",
                17 => "console",
                19 => "inventaire PCI",
                21 => "watchdog",
                _ => "inventaire incomplet",
            };
            let _ = afficher(
                st,
                &format!("STANDARD REFUS {code} : {motif} | arret local"),
            );
        }
    }
    journal::seal();
    unsafe {
        ptr::write_volatile(
            &raw mut BORNES,
            Bornes {
                magic: *b"NOVA5W-TERMINAL!",
                position: (&raw const BORNES) as u64,
                entree: efi_main as *const () as u64,
                arret: arret as *const () as u64,
            },
        );
    }
    arret()
}
#[inline(never)]
extern "C" fn arret() -> ! {
    unsafe {
        asm!("cli", "2:", "hlt", "jmp 2b", options(noreturn, nostack));
    }
}
#[panic_handler]
fn panique(_: &PanicInfo) -> ! {
    let _ = journal::append("STANDARD REFUS : erreur interne");
    journal::seal();
    arret()
}
