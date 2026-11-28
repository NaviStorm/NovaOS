use crate::{Resultat, acpi, catalogue, check, chemins, firmware};
use novaos_catalogue::{Blob, Catalog, Error};
use r_efi::efi;
pub fn examiner(
    st: &efi::SystemTable,
    map: &firmware::Map,
    manifest: &[u8],
    sig: &[u8],
    elf: &[u8],
    r: &mut Resultat,
) -> Result<(), u64> {
    let pol = catalogue::policy(3).map_err(|_| 6u64)?;
    let mut cat = Catalog::new(&pol);
    for (m, s) in [
        (manifest, sig),
        (
            &include_bytes!("../diagnostic-5e/vecteurs/hpet-a.toml")[..],
            &include_bytes!("../diagnostic-5e/vecteurs/hpet-a.sig")[..],
        ),
        (
            &include_bytes!("../diagnostic-5e/vecteurs/hpet-b.toml")[..],
            &include_bytes!("../diagnostic-5e/vecteurs/hpet-b.sig")[..],
        ),
    ] {
        cat.admit(
            m,
            s,
            &[Blob {
                isa: "x86_64",
                bytes: elf,
            }],
        )
        .map_err(|e| match e {
            Error::Elf | Error::Digest => 18u64,
            _ => 6,
        })?;
    }
    // Ce profil distribue trois pilotes fictifs identifiés; refuser un autre manifeste signé.
    check(
        manifest == include_bytes!("../diagnostic-5e/vecteurs/ioapic.toml"),
        6,
    )?;
    let rsdp = unsafe { firmware::rsdp(st, map) }?;
    let mut io = 0;
    let mut hp = 0;
    let mut dr = 0;
    let inventory = acpi::discover_with(
        rsdp,
        |addr, b| map.copy_acpi(addr, b),
        |sig, t| {
            match &sig {
                b"APIC" => {
                    let mut p = t.get(44..).ok_or(acpi::Error::Length)?;
                    while !p.is_empty() {
                        if p.len() < 2 || p[1] < 2 {
                            return Err(acpi::Error::Length);
                        }
                        if p[0] == 1 {
                            io += 1;
                        }
                        p = p.get(p[1] as usize..).ok_or(acpi::Error::Length)?;
                    }
                }
                b"HPET" => {
                    if t.len() < 56 {
                        return Err(acpi::Error::Length);
                    }
                    hp += 1;
                }
                b"DMAR" => {
                    let mut p = t.get(48..).ok_or(acpi::Error::Length)?;
                    while !p.is_empty() {
                        if p.len() < 4 {
                            return Err(acpi::Error::Length);
                        }
                        let n = u16::from_le_bytes([p[2], p[3]]) as usize;
                        if n < 4 {
                            return Err(acpi::Error::Length);
                        }
                        if p[..2] == [0, 0] {
                            dr += 1;
                        }
                        p = p.get(n..).ok_or(acpi::Error::Length)?;
                    }
                }
                _ => {}
            }
            Ok(())
        },
    )
    .map_err(|_| 5u64)?;
    let bs = unsafe { st.boot_services.as_ref() }.ok_or(2u64)?;
    let pci = chemins::pci(bs, map, 64)?;
    check(
        dr == 1
            && inventory.tables.iter().filter(|s| *s == b"DMAR").count() == 1
            && io + hp + dr + pci.len() <= 64
            && pci
                .iter()
                .any(|p| p.depth == 1 && p.device as u64 * 8 + p.function as u64 == r.rid),
        15,
    )?;
    let mut candidates = 0;
    let mut conflicts = 0;
    let mut digest = [0; 32];
    for (name, count) in [("ioapic", io), ("hpet", hp)] {
        if count == 0 {
            continue;
        }
        match catalogue::select(&cat, "acpi", name, &alloc::format!("acpi.{name}")) {
            Ok(Some(h)) => {
                check(name == "ioapic", 7)?;
                candidates += count;
                digest = h;
            }
            Ok(None) => {}
            Err(Error::Conflict) => conflicts += count,
            Err(_) => return Err(7),
        }
    }
    r.tables = inventory.tables.len() as u64;
    r.pci = pci.len() as u64;
    r.ioapic = io as u64;
    r.hpet = hp as u64;
    r.drhd = dr as u64;
    r.compatibles = candidates as u64;
    r.conflits = conflicts as u64;
    r.plan = digest;
    Ok(())
}
