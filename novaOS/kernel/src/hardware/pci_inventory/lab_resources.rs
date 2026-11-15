//! Autorité et géométries du banc QEMU, absentes du noyau de distribution.
use super::super::{Bar, BarKind, Function};
use super::{Address, Error, Span};
extern crate alloc;
use alloc::vec::Vec;

pub(in super::super) fn extent(bar: Bar, size: u64) -> Result<Span, Error> {
    let (minimum, ceiling) = match bar.kind {
        BarKind::Io => (4, u64::from(u32::MAX)),
        BarKind::MemoryBelow1M => (16, (1 << 20) - 1),
        BarKind::Memory32 => (16, u64::from(u32::MAX)),
        BarKind::Memory64 => (16, u64::MAX),
    };
    if !size.is_power_of_two() || size < minimum || bar.base == 0 || bar.base % size != 0 {
        return Err(Error::Bar);
    }
    let last = bar
        .base
        .checked_add(size - 1)
        .filter(|&end| end <= ceiling)
        .ok_or(Error::Bar)?;
    Ok(Span {
        base: bar.base,
        last,
    })
}

/// Géométries du profil fixe du banc, comparées aux régions QMP par le banc.
/// Une correspondance sert à vérifier une attribution existante, sans sondage de BAR.
/// La voie d’inventaire ordinaire conserve une taille inconnue.
fn profile_bar(a: Address, f: &Function, index: usize) -> Option<(BarKind, bool, u64)> {
    use BarKind::{Io, Memory32, Memory64};
    match (a.device, a.function, f.vendor, f.device, index) {
        (6, 0, 0x1234, 0x11e8, 0) => Some((Memory32, false, EDU_SIZE)),
        (31, 2, 0x8086, 0x2922, 4) => Some((Io, false, 32)),
        (31, 2, 0x8086, 0x2922, 5) => Some((Memory32, false, 4096)),
        (31, 3, 0x8086, 0x2930, 4) => Some((Io, false, 64)),
        (1, 0, 0x1af4, 0x1004, 0) => Some((Io, false, 64)),
        (1, 0, 0x1af4, 0x1004, 1) => Some((Memory32, false, 4096)),
        (1, 0, 0x1af4, 0x1004, 4) => Some((Memory64, true, 16384)),
        _ => None,
    }
}

pub fn profile_extent(
    a: Address,
    f: &Function,
    index: usize,
    bar: Bar,
) -> Result<(Span, u64), Error> {
    let (kind, prefetch, size) = profile_bar(a, f, index).ok_or(Error::Bar)?;
    if kind != bar.kind || prefetch != bar.prefetchable {
        return Err(Error::Bar);
    }
    Ok((extent(bar, size)?, size))
}

/// Géométrie native du périphérique EDU du seul profil de laboratoire.
/// Aucun graphe PCI découvert ni identifiant seul ne peut accorder un mapping.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EduBar {
    pub base: u64,
    pub size: u64,
    pub irq_pin: u8,
    pub irq_line: u8,
}

pub const EDU_SIZE: u64 = 1 << 20;
pub const EDU: Address = Address {
    segment: 0,
    bus: 0,
    device: 6,
    function: 0,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LabPlatform {
    #[cfg_attr(all(not(test), target_arch = "aarch64"), allow(dead_code))]
    Q35,
    #[cfg_attr(all(not(test), target_arch = "x86_64"), allow(dead_code))]
    Virt,
}

type Slot = (u8, u8, u16, u16, u8);
const Q35: &[Slot] = &[
    (0, 0, 0x8086, 0x29c0, 0),  // pont hôte
    (6, 0, 0x1234, 0x11e8, 1),  // EDU
    (31, 0, 0x8086, 0x2918, 0), // LPC
    (31, 2, 0x8086, 0x2922, 1), // AHCI
    (31, 3, 0x8086, 0x2930, 1), // SMBus
];
const VIRT: &[Slot] = &[
    (0, 0, 0x1b36, 0x0008, 0), // GPEX
    (1, 0, 0x1af4, 0x1004, 1), // contrôleur du CD UEFI
    (6, 0, 0x1234, 0x11e8, 1),
];

/// Ce contrat concerne les instances QEMU créées par le banc, sans hotplug, option
/// ROM active, VGA ou fonction additionnelle. Les tailles annoncées par le banc sont recoupées
/// avec la forme et la position lues sur la fonction avant toute attribution.
pub fn qualify_edu(
    platform: LabPlatform,
    functions: &[(Address, Function)],
) -> Result<EduBar, Error> {
    let (profile, aperture) = match platform {
        LabPlatform::Q35 => (
            Q35,
            Span {
                base: 0x8000_0000,
                last: 0xafff_ffff,
            },
        ),
        LabPlatform::Virt => (
            VIRT,
            Span {
                base: 0x1000_0000,
                last: 0x2fff_ffff,
            },
        ),
    };
    // Ce profil ne comporte aucun pont PCI-PCI. Un pont ajouté, même avec des
    // fenêtres fermées, invalide aussi le fait que le routage INTx est exclusif.
    if functions.len() != profile.len() {
        return Err(Error::Identity);
    }
    for &(dev, fun, vendor, device, pin) in profile {
        let addr = Address {
            segment: 0,
            bus: 0,
            device: dev,
            function: fun,
        };
        let (_, f) = functions
            .iter()
            .find(|(a, _)| *a == addr)
            .ok_or(Error::Identity)?;
        if (f.vendor, f.device, f.header, f.irq_pin) != (vendor, device, 0, pin) || f.rom_enabled {
            return Err(Error::Identity);
        }
    }
    let (_, edu) = functions
        .iter()
        .find(|(a, _)| *a == EDU)
        .ok_or(Error::Identity)?;
    let bar = edu.bars[0].ok_or(Error::Bar)?;
    if (edu.class, edu.subclass, edu.interface) != (0, 0xff, 0)
        || !edu.msi
        || edu.msi_armed
        || edu.msix
        || edu.command & 0x400 != 0
        || edu.bars[1..].iter().any(Option::is_some)
        || bar.kind != BarKind::Memory32
        || bar.prefetchable
    {
        return Err(Error::Bar);
    }
    let candidate = extent(bar, EDU_SIZE)?;
    if candidate.base < aperture.base || candidate.last > aperture.last {
        return Err(Error::Range);
    }
    // La taille du profil bornera même une BAR dont la base est très alignée.
    // Un BAR non décrit ou une intersection entre deux propriétaires ferme l’accès.
    let mut occupied = Vec::new();
    occupied
        .try_reserve_exact(6 * functions.len())
        .map_err(|_| Error::Budget)?;
    for &(a, f) in functions {
        for (i, bar) in f.bars.iter().enumerate() {
            let Some(bar) = bar else { continue };
            let io = bar.kind == BarKind::Io;
            let (window, _) = profile_extent(a, &f, i, *bar)?;
            if occupied
                .iter()
                .any(|&(other_io, other)| io == other_io && window.intersects(other))
            {
                return Err(Error::Bar);
            }
            occupied.push((io, window));
        }
    }
    Ok(EduBar {
        base: bar.base,
        size: EDU_SIZE,
        irq_pin: edu.irq_pin,
        irq_line: edu.irq_line,
    })
}
