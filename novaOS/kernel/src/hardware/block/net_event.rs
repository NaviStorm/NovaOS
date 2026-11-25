//! Négociation privée du NIC du banc. Aucune transition vers DRIVER_OK ici.
use super::{Error, Result, pci::Device};
use crate::hal;

const REQUIS: u64 = (1 << 32) | (1 << 33) | (1 << 5) | (1 << 29);

pub(super) fn negocier(nic: &Device) -> Result<()> {
    if nic.cfg.get(4, 2) & 0x404 != 0x400 || nic.msix.is_some() {
        return Err(Error::Route);
    }
    let mut offre = 0u64;
    for bank in 0..2 {
        nic.common.w32(0, bank);
        offre |= u64::from(nic.common.r32(4)) << (bank * 32);
    }
    let verification = nic.start(REQUIS).and_then(|()| {
        let mut relu = 0u64;
        for bank in 0..2 {
            nic.common.w32(8, bank);
            relu |= u64::from(nic.common.r32(12)) << (bank * 32);
        }
        if offre & REQUIS != REQUIS || relu != REQUIS || nic.common.r8(20) != 11 {
            return Err(Error::Feature);
        }
        if nic.cfg.get(4, 2) & 0x404 != 0x400 {
            return Err(Error::Route);
        }
        hal::log(format_args!(
            "NET EVENT_IDX offre={offre:#x} relu={relu:#x} statut=11 BME=0"
        ));
        Ok(())
    });
    if let Err(cause) = verification {
        nic.reset()?;
        if nic.cfg.get(4, 2) & 0x404 != 0x400 {
            return Err(Error::Quarantine);
        }
        hal::log(format_args!(
            "NET EVENT_IDX refus={cause:?} offre={offre:#x} statut=0 BME=0"
        ));
        return Err(cause);
    }
    Ok(())
}

#[cfg(target_arch = "x86_64")]
pub(in crate::hardware) fn q35(cadres: &[super::Frame; 22]) -> Result<()> {
    // BAR4 de 16 Kio assignée par OVMF. Le routage VT-d est déjà confirmé.
    let cfg = super::pci::Config::open(56)?;
    let base = (u64::from(cfg.get(0x24, 4)) << 32) | u64::from(cfg.get(0x20, 4) & !15);
    if cfg.get(4, 2) & 4 != 0
        || cfg.get(0x20, 4) & 15 != 12
        || base != 0xc0_0000_0000
        || !crate::memory::firmware::unlisted_extent(base, 4 * super::PAGE as u64)
    {
        return Err(Error::Geometry);
    }
    let nic = Device::discover(56, 0x1041, 368)?;
    for _ in 0..2 {
        negocier(&nic)?;
        if let Err(e) = super::net_files::preparer(&nic, cadres, true) {
            nic.reset()?;
            return Err(e);
        }
        nic.reset()?;
        if nic.cfg.get(4, 2) & 0x404 != 0x400 || nic.common.r8(20) != 0 {
            return Err(Error::Quarantine);
        }
    }
    Ok(())
}
