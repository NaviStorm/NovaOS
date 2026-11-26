//! Anneaux du profil privé. Pendant la préparation, BME et DRIVER_OK restent fermés.
use super::{
    Error, Result,
    io::{Frame, PAGE, barrier},
    pci::Device,
};
use crate::hal;

pub(super) fn preparer(nic: &Device, cadres: &[Frame; 22], installer: bool) -> Result<()> {
    let architecture = if cfg!(target_arch = "aarch64") {
        "ARM"
    } else {
        "Q35"
    };
    if nic.common.r8(20) != 11 || nic.cfg.get(4, 2) & 0x404 != 0x400 || nic.common.r16(18) < 2 {
        return Err(Error::Route);
    }
    let ver = nic.common.r8(21);
    let mut mac = [0u8; 6];
    nic.specific.bytes(0, &mut mac);
    if nic.common.r8(21) != ver || mac == [0; 6] || mac[0] & 1 != 0 {
        return Err(Error::Feature);
    }
    for file in 0..2 {
        nic.common.w16(22, file as u16);
        if nic.common.r16(28) != 0 || nic.common.r16(24) < 16 {
            return Err(Error::Queue);
        }
        nic.common.w16(24, 16);
        for r in 0..3 {
            let addr = 0x1000_0000 + ((file * 3 + r) * PAGE) as u64;
            let off = 32 + r * 8;
            nic.common.w32(off, addr as u32);
            nic.common.w32(off + 4, (addr >> 32) as u32);
            if nic.common.r32(off) != addr as u32 || nic.common.r32(off + 4) != (addr >> 32) as u32
            {
                return Err(Error::Queue);
            }
        }
        let ring = cadres[file * 3].mem;
        for i in 0..16 {
            let dma = 0x1000_0000 + ((if file == 0 { 6 } else { 14 }) * PAGE + i * 2048) as u64;
            ring.w64(i * 16, dma);
            ring.w32(i * 16 + 8, if file == 0 { 2048 } else { 0 });
            ring.w16(i * 16 + 12, if file == 0 { 2 } else { 0 });
            ring.w16(i * 16 + 14, 0);
        }
        let off = usize::from(nic.common.r16(30))
            .checked_mul(nic.stride)
            .ok_or(Error::Geometry)?;
        nic.notify.sub(off, 2)?;
        if installer {
            // Aucun kick, aucun descripteur disponible, DRIVER_OK et BME fermés.
            // Examiner les erreurs réelles de la préparation avant une attribution.
            barrier();
            nic.common.w16(28, 1);
            barrier();
            hal::log(format_args!(
                "NET {architecture} installation file={file} enable={} statut={} BME={}",
                nic.common.r16(28),
                nic.common.r8(20),
                (nic.cfg.get(4, 2) >> 2) & 1
            ));
            if nic.common.r8(20) != 11 {
                return Err(Error::Device);
            }
        }
        if nic.common.r16(24) != 16
            || nic.common.r16(28) != u16::from(installer)
            || nic.cfg.get(4, 2) & 4 != 0
        {
            return Err(Error::Quarantine);
        }
    }
    barrier();
    hal::log(format_args!(
        "NET {architecture} registres RX=16 TX=16 mac={mac:02x?} BME=0 installation={installer}"
    ));
    Ok(())
}
