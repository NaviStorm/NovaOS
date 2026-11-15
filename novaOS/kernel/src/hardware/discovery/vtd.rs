//! Reprise du seul profil Q35, entre ExitBootServices et le lancement des AP.
use super::*;
use crate::memory;
use pci::resources::lab::{EDU, LabPlatform};

/// Autorisation de lire les registres du DRHD, après fermeture des masters
/// hérités et recoupement du profil. Aucune construction depuis des données utilisateur.
pub(crate) struct Drhd(acpi::Q35Dmar, u16);
impl Drhd {
    pub(crate) fn base(&self) -> u64 {
        self.0.base
    }
    pub(crate) fn host_bits(&self) -> u8 {
        self.0.host_bits
    }
    pub(crate) fn sid(&self) -> u16 {
        self.1
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum VtdProfile {
    Standard,
    DelayedReceipt,
    QueueError,
    NeverSubmitted,
    LostCompletion,
}
impl VtdProfile {
    pub(crate) fn selected() -> Option<Self> {
        match boot::scenario() {
            b"vtd-edu-test" => Some(Self::Standard),
            b"vtd-edu-quarantaine" => Some(Self::DelayedReceipt),
            b"vtd-edu-erreur-qi" => Some(Self::QueueError),
            b"vtd-edu-jamais-soumise" => Some(Self::NeverSubmitted),
            b"vtd-edu-fin-incertaine" => Some(Self::LostCompletion),
            _ => None,
        }
    }
}

pub(crate) fn vtd_edu(_: VtdProfile) -> Option<(EduBar, Option<Drhd>)> {
    // CF8/CFC est l'accès indépendant à la racine fixée par le profil Q35.
    // Même si ACPI est inutilisable, on peut reprendre et fermer EDU sur la
    // base du seul inventaire PCI exact. Un matériel inconnu n'est pas touché.
    let mut functions = Vec::new();
    functions.try_reserve_exact(5).ok()?;
    let mut valid = true;
    let count = pci::enumerate(
        &mut Config,
        pci::BusRange {
            segment: 0,
            first: 0,
            last: 0,
        },
        |a, f| match f {
            Ok(f) if functions.len() < 5 => functions.push((a, f)),
            _ => valid = false,
        },
    );
    if count.is_err() || !valid {
        log(format_args!("VTD topologie PCI refusee"));
        return None;
    }
    let (edu, command) = inspect_edu(LabPlatform::Q35, &functions)?;
    // Même une fonction 0 seule actuellement visible ne prouve pas que son
    // groupe est isolable si le slot se déclare multifonction.
    let pci = super::super::pci_ports::port;
    if pci(0x30, 0x0e, 1, None) != 0 {
        log(format_args!("VTD groupe EDU multifonction refuse"));
        return None;
    }
    // Inspection du groupe intégralement en lecture seule. Sans MSE, le
    // registre RUN n'est pas observable. Ne pas ouvrir le décodage pour sonder.
    if command & 2 == 0 || !edu_idle(edu.base) {
        log(format_args!(
            "VTD reprise moteur EDU refusee avant ecriture PCI commande={command:#06x}"
        ));
        return None;
    }
    let ahci = functions
        .iter()
        .find(|(a, _)| (a.device, a.function) == (31, 2))?
        .1;
    let abar = ahci.bars[5]?.base;
    if ahci.command & 2 == 0 || pci(0xfa, 0x24, 4, None) as u64 != abar || !ahci_ports(abar, false)
    {
        log(format_args!("VTD reprise AHCI refusee avant ecriture PCI"));
        return None;
    }
    log(format_args!(
        "VTD precontrole groupe=racine EDU_vierge=true AHCI_sans_commandes=true"
    ));
    let edu = adopt_edu(edu, command)?;
    // Les pages de Limine et du firmware sont encore réservées ici. Un moteur
    // non maîtrisé doit empêcher leur remise à l'allocateur, même sans DMAR.
    if !edu_idle(edu.base) {
        log(format_args!("VTD reprise moteur EDU refusee"));
        return None;
    }
    log(format_args!("VTD EDU reprise moteur_vierge=true BME=0"));
    if !quiesce_peers(&functions, EDU) {
        log(format_args!("VTD reprise DMA pairs refusee"));
        return None;
    }
    let candidate = q35_drhd(EDU);
    if let Some(drhd) = &candidate {
        log(format_args!(
            "VTD DMAR DRHD segment=0 base={:#x} HAW={} SID={:#06x} portee_explicite={}",
            drhd.base(),
            drhd.host_bits(),
            drhd.sid(),
            drhd.0.explicit
        ));
    }
    Some((edu, candidate))
}

pub(in crate::hardware) fn q35_drhd(target: pci::Address) -> Option<Drhd> {
    if target != pci::resources::lab::Q35_LAB_SLOT {
        return None;
    }
    let sid = u16::from(target.device) << 3 | u16::from(target.function);
    boot::rsdp_physical()
        .and_then(|pa| {
            acpi::discover(pa, firmware::copy_acpi)
                .map_err(|e| {
                    log(format_args!("VTD ACPI refuse {e:?}"));
                })
                .ok()
        })
        .and_then(|inv| {
            if inv.ecam.len() != 1
                || inv.ecam[0]
                    != (acpi::Ecam {
                        base: 0xe000_0000,
                        segment: 0,
                        first_bus: 0,
                        last_bus: 255,
                    })
            {
                return None;
            }
            if inv.q35_dmar.is_none() {
                log(format_args!(
                    "VTD DMAR presente={} unite_admise=false",
                    inv.tables.contains(b"DMAR")
                ));
            }
            inv.q35_dmar
        })
        .filter(|d| firmware::unlisted_extent(d.base, memory::PAGE))
        .map(|d| Drhd(d, sid))
}

pub(in crate::hardware) fn quiesce_peers(
    functions: &[(pci::Address, pci::Function)],
    target: pci::Address,
) -> bool {
    for &(a, f) in functions {
        if a == target {
            continue;
        }
        // Seule la fonction AHCI de ce chipset QEMU possède un moteur
        // autonome de DMA PCI sur les quatre pairs acceptés par qualify_edu.
        let sid = u16::from(a.device) << 3 | u16::from(a.function);
        let cfg = |off, width, value| crate::hardware::pci_ports::port(sid, off, width, value);
        let id = u32::from(f.device) << 16 | u32::from(f.vendor);
        if cfg(0, 4, None) != id || cfg(4, 2, None) as u16 != f.command {
            return false;
        }
        if sid == 0xfa {
            let Some(abar) = f.bars[5] else { return false };
            if f.command & 2 == 0
                || cfg(0x24, 4, None) as u64 != abar.base
                || !ahci_ports(abar.base, true)
            {
                return false;
            }
        }
        // Le registre de commande de la reprise ne doit pas ouvrir le décodage.
        if f.bus_master {
            cfg(4, 2, Some(u32::from(f.command & !4)));
        }
        let command = cfg(4, 2, None) as u16;
        if cfg(0, 4, None) != id || command != f.command & !4 {
            return false;
        }
        log(format_args!(
            "VTD PCI pair SID={sid:#06x} commande={:#06x}->{command:#06x}",
            f.command
        ));
    }
    true
}

// Slot de lecture fixe réutilisé séquentiellement sur le BSP. Aucun bail IRQ
// ni client n'existe encore sur ce scénario exclusif.
const SCRATCH: u64 = memory::IO_BASE + 323 * memory::PAGE;

fn edu_idle(bar: u64) -> bool {
    if memory::map_io(bar, SCRATCH).is_err() {
        return false;
    }
    // Sûreté : page UC du BAR EDU relu, pas d'écriture à l'appareil.
    let r32 = |off: u64| unsafe { ((SCRATCH + off) as *const u32).read_volatile() };
    let r64 = |off: u64| unsafe { ((SCRATCH + off) as *const u64).read_volatile() };
    let command = r64(0x98);
    let idle = r32(0) == 0x0100_00ed
        && r32(0x20) == 0
        && r32(0x24) == 0
        && command == 0
        && [0x80, 0x88, 0x90].into_iter().all(|off| r64(off) == 0);
    let retired = memory::unmap_lab_io(bar, SCRATCH).is_ok();
    if !idle {
        log(format_args!(
            "VTD EDU etat herite RUN={} vierge=false",
            command & 1
        ));
    }
    idle && retired
}
pub(in crate::hardware) fn ahci_ports(bar: u64, stop: bool) -> bool {
    // ABAR5 appartient au seul ICH9 2922 du profil, 4 Kio, dans le trou PCI Q35.
    if !(0x8000_0000..0xb000_0000).contains(&bar)
        || !firmware::unlisted_extent(bar, memory::PAGE)
        || memory::map_io(bar, SCRATCH).is_err()
    {
        return false;
    }
    let good = inspect_ahci_ports(stop);
    memory::unmap_lab_io(bar, SCRATCH).is_ok() && good
}

fn inspect_ahci_ports(stop: bool) -> bool {
    let read = |off: usize| unsafe { ((SCRATCH as usize + off) as *const u32).read_volatile() };
    let write =
        |off: usize, value| unsafe { ((SCRATCH as usize + off) as *mut u32).write_volatile(value) };
    let ports = read(0x0c);
    if read(4) & 1 != 0 || ports != 0x3f {
        return false;
    }
    for n in 0..6 {
        let port = 0x100 + 0x80 * n;
        let idle = || read(port + 0x38) == 0 && read(port + 0x34) == 0;
        let cmd = port + 0x18;
        if !idle() || read(cmd) & (1 << 3) != 0 {
            return false;
        }
        if !stop {
            continue;
        }
        for (enable, running) in [(1, 1 << 15), (1 << 4, 1 << 14)] {
            let old = read(cmd);
            if old & enable != 0 {
                write(cmd, old & !enable);
            }
            // Limite aussi le nombre de lectures si le timer du BSP se fige.
            let start = crate::sched::ticks();
            let mut cleared = false;
            for _ in 0..1_000_000 {
                if read(cmd) & (enable | running) == 0 {
                    cleared = true;
                    break;
                }
                if crate::sched::ticks().wrapping_sub(start) >= u64::from(crate::sched::HZ) {
                    break;
                }
                core::hint::spin_loop();
            }
            if !cleared {
                return false;
            }
        }
        if !idle() {
            return false;
        }
    }
    if stop {
        log(format_args!(
            "VTD AHCI ports=6 CI=0 SACT=0 ST=0 CR=0 FRE=0 FR=0"
        ));
    }
    true
}
