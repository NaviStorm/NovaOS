//! Négociation privée du NIC du banc. Aucune transition vers DRIVER_OK ici.
use super::{Error, Result, pci::Device};
use crate::hal;

const REQUIS: u64 = (1 << 32) | (1 << 33) | (1 << 5) | (1 << 29);

pub(super) fn negocier(nic: &Device) -> Result<()> {
    negocier_options(nic, 0).map(|_| ())
}

#[cfg(all(target_arch = "aarch64", feature = "ethernet-v1-port"))]
pub(super) fn negocier_reset(nic: &Device) -> Result<bool> {
    negocier_options(nic, 1 << 40).map(|masque| masque & (1 << 40) != 0)
}

fn negocier_options(nic: &Device, options: u64) -> Result<u64> {
    if nic.cfg.get(4, 2) & 0x404 != 0x400 || nic.msix.is_some() {
        return Err(Error::Route);
    }
    let mut offre = 0u64;
    for bank in 0..2 {
        nic.common.w32(0, bank);
        offre |= u64::from(nic.common.r32(4)) << (bank * 32);
    }
    let masque = REQUIS | (offre & options);
    if masque & (1 << 40) != 0 { nic.common.sub(58, 2)?; }
    let verification = nic.start(masque).and_then(|()| {
        let mut relu = 0u64;
        for bank in 0..2 {
            nic.common.w32(8, bank);
            relu |= u64::from(nic.common.r32(12)) << (bank * 32);
        }
        if offre & REQUIS != REQUIS || relu != masque || nic.common.r8(20) != 11 {
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
    Ok(masque)
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

#[cfg(all(target_arch = "x86_64", feature = "net-ethernet-lab"))]
pub(in crate::hardware) fn trafic_q35(cadres: &[super::Frame; 22], generation: u64) -> Result<()> {
    use crate::{hardware::pci_ports, memory};
    let cfg = super::pci::Config::open(56)?;
    let base = u64::from(cfg.get(0x24, 4)) << 32 | u64::from(cfg.get(0x20, 4) & !15);
    if cfg.get(4, 2) & 4 != 0
        || cfg.get(0x20, 4) & 15 != 12
        || base != 0xc0_0000_0000
        || !memory::firmware::unlisted_extent(base, 16384)
        || cfg.get(0x3d, 1) != 1
        || pci_ports::port(248, 0xf0, 4, None) != 0xfed1c001
    {
        return Err(Error::Route);
    }
    // 00:07.0 INTA -> PIRQH. Lire le routage configurable de D31 avant d'allouer GSI 23.
    let r = super::io::Region::map(0xfed1f000, 323, 4096)?;
    let ir = r.r16(0x140);
    memory::unmap_lab_io(0xfed1f000, memory::IO_BASE + 323 * 4096).map_err(|_| Error::Route)?;
    for sid in [0, 248, 250, 251] {
        let pin = pci_ports::port(sid, 0x3d, 1, None);
        if pin > 4 || (pin != 0 && (sid == 0 || (ir >> (4 * (pin - 1))) & 7 == 7)) {
            return Err(Error::Route);
        }
    }
    let nic = Device::discover(56, 0x1041, 368)?;
    negocier(&nic)?;
    let mut port = super::net_trames::Port::nouveau(&nic, cadres)?;
    let ligne =
        crate::hardware::lab_irq::net_intx::Ligne::ouvrir(nic.isr.base()).ok_or(Error::Route)?;
    let hz = hal::timer::counter_hz();
    #[cfg(feature="ethernet-v1-port")]
    let mut scelle = false;
    let ok = crate::sched::process::net_preparation::ethernet(generation, ||true, |tag, len, copie| {
        if tag == 5 {
            #[cfg(feature="ethernet-v1-port")]
            {
                scelle = port.sceller(generation).is_ok();
                return scelle.then_some(0).ok_or(crate::abi::REFUSED);
            }
            #[cfg(not(feature="ethernet-v1-port"))]
            return port.stopper().map(|()| 0).map_err(|_| crate::abi::REFUSED);
        }
        if tag==6 {
            let _=nic.isr.r8(0);
            if !ligne.rearmer() {return Err(crate::abi::REFUSED);}
        }
        port.demande(tag, len, copie, ligne.cause() == 0x101)
    });
    #[cfg(not(feature="ethernet-v1-port"))]
    let stop = port.stopper();
    let cause = ligne.cause();
    hal::log(format_args!(
        "NET Q35 L2 GSI=23 ISR={cause:#x} D31IR={ir:#x} hz={hz} dma={}",
        memory::pinned_dma_frames()
    ));
    if !ligne.fermer() {
        return Err(Error::Quarantine);
    }
    #[cfg(not(feature="ethernet-v1-port"))]
    {
        stop?;
        if !ok || cause != 0x101 { return Err(Error::Fault); }
    }
    #[cfg(feature="ethernet-v1-port")]
    {
        hal::log(format_args!("E3C session generation={generation} succes={ok}"));
        if !scelle { return Err(Error::Quarantine); }
        // Les cadres restent épinglés. Le propriétaire VT-d doit retirer la
        // racine, achever CC/IOTLB DR+DW et vérifier son WAIT avant restitution.
    }
    Ok(())
}
