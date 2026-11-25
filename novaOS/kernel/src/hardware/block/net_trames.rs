//! Une rafale de qualification. Descripteurs et buffers restent au superviseur.
use super::{
    Device, Error, Frame, Result,
    io::{Region, barrier},
};
use crate::{abi, hal};
const N: usize = 16;

pub(super) struct Port<'a> {
    nic: &'a Device,
    cadres: &'a [Frame; 22],
    kicks: [Region; 2],
    postes: usize,
    rx: u16,
    tx: u16,
    livres: usize,
    ouvert: bool,
    draine: bool,
    depart: u64,
    collectes: [u64; 16],
}
impl<'a> Port<'a> {
    pub(super) fn nouveau(nic: &'a Device, cadres: &'a [Frame; 22]) -> Result<Self> {
        if nic.cfg.get(4, 2) & 0x404 != 0x400 || nic.common.r8(20) != 11 || nic.isr.r8(0) != 0 {
            return Err(Error::Route);
        }
        super::net_files::preparer(nic, cadres, true)?;
        let mut kicks = [nic.notify; 2];
        for (i, k) in kicks.iter_mut().enumerate() {
            nic.common.w16(22, i as u16);
            if nic.common.r16(24) != 16 || nic.common.r16(28) != 1 {
                return Err(Error::Queue);
            }
            let offset = usize::from(nic.common.r16(30))
                .checked_mul(nic.stride)
                .ok_or(Error::Geometry)?;
            *k = nic.notify.sub(offset, 2)?;
            if cadres[i * 3 + 1].mem.r16(2) != 0 || cadres[i * 3 + 2].mem.r16(2) != 0 {
                return Err(Error::State);
            }
        }
        Ok(Self {
            nic,
            cadres,
            kicks,
            postes: 0,
            rx: 0,
            tx: 0,
            livres: 0,
            ouvert: false,
            draine: false,
            depart: 0,
            collectes: [0; 16],
        })
    }
    pub(super) fn demande(
        &mut self,
        tag: u64,
        len: usize,
        copie: &mut [u8],
        irq: bool,
    ) -> core::result::Result<usize, u64> {
        match tag {
            1 if !self.ouvert => {
                // Le filtre du laboratoire est fixe et sans offload.
                let mut mac = [0u8; 6];
                self.nic.specific.bytes(0, &mut mac);
                if mac != [0x52, 0x54, 0, 0x12, 0x34, 0x56] {
                    return Err(abi::REFUSED);
                }
                let avail = self.cadres[1].mem;
                for i in 0..N {
                    self.cadres[6 + i / 2].mem.w16((i % 2) * 2048 + 10, 0x5a5a);
                    avail.w16(4 + i * 2, i as u16);
                }
                avail.w16(36, 0);
                self.cadres[4].mem.w16(36, 0);
                barrier();
                avail.w16(2, 16);
                barrier();
                self.ouvert = true; // Toute erreur suivante passe par le confinement du retrait.
                self.nic.ready().map_err(|_| abi::REFUSED)?;
                self.nic.cfg.intx(true);
                if self.nic.cfg.get(4, 2) & 0x404 != 4 || self.nic.common.r8(20) != 15 {
                    return Err(abi::REFUSED);
                }
                self.kicks[0].w16(0, 0);
                hal::log(format_args!("NET L2 ouverture statut=15 RX=16 TX=16 BME=1"));
                Ok(0)
            }
            2 if self.ouvert && self.postes < N && (72..=1526).contains(&len) => {
                let i = self.postes;
                if copie[..12] != [0; 12]
                    || copie[12..18] != [2, 0x4e, 0x56, 0, 0, 1]
                    || copie[18..24] != [0x52, 0x54, 0, 0x12, 0x34, 0x56]
                    || copie[24..26] != [0x88, 0xb5]
                {
                    return Err(abi::BAD_ARGUMENT);
                }
                self.cadres[14 + i / 2]
                    .mem
                    .put((i % 2) * 2048, &copie[..len]);
                self.cadres[3].mem.w32(i * 16 + 8, len as u32);
                self.cadres[4].mem.w16(4 + i * 2, i as u16);
                self.postes += 1;
                if self.postes == N {
                    hal::log(format_args!(
                        "NET L2 TX soumis=16 longueurs alternées 72/1526"
                    ));
                    self.depart = hal::timer::counter();
                    barrier();
                    self.cadres[4].mem.w16(2, N as u16);
                    barrier();
                    self.kicks[1].w16(0, 1);
                }
                Ok(0)
            }
            2 if self.ouvert && self.postes == N && self.livres == 0 => Err(abi::WOULD_BLOCK),
            3 if self.postes == N && self.livres < N => {
                barrier();
                let (rx, tx) = (self.cadres[2].mem, self.cadres[5].mem);
                let (r, t) = (rx.r16(2), tx.r16(2));
                if r > 16 || t > 16 {
                    return Err(abi::REFUSED);
                }
                if !irq || usize::from(r) <= self.livres || usize::from(t) <= self.livres {
                    return Err(abi::WOULD_BLOCK);
                }
                barrier();
                let i = self.livres;
                let pos = 4 + 8 * i;
                let (ri, ti) = (rx.r32(pos), tx.r32(pos));
                let n = rx.r32(pos + 4) as usize;
                if ri >= 16
                    || ti >= 16
                    || !(26..=1526).contains(&n)
                    || tx.r32(pos + 4) != 0
                    || self.rx & (1 << ri) != 0
                    || self.tx & (1 << ti) != 0
                {
                    return Err(abi::REFUSED);
                }
                self.collectes[i] = hal::timer::counter().wrapping_sub(self.depart);
                self.rx |= 1 << ri;
                self.tx |= 1 << ti;
                self.livres += 1;
                self.cadres[6 + ri as usize / 2]
                    .mem
                    .bytes((ri as usize % 2) * 2048, &mut copie[..n]);
                Ok(n)
            }
            4 if self.livres == N && self.rx == u16::MAX && self.tx == u16::MAX => {
                self.draine = true;
                hal::log(format_args!(
                    "NET L2 kick_vers_collecte_cycles={:?}",
                    self.collectes
                ));
                hal::log(format_args!(
                    "NET L2 completions TX=16 RX=16 identifiants_uniques=true"
                ));
                Ok(0)
            }
            _ => Err(abi::DENIED),
        }
    }
    pub(super) fn stopper(&self) -> Result<()> {
        self.nic.cfg.intx(false);
        self.nic.cfg.master(false);
        barrier();
        if self.nic.cfg.get(4, 2) & 0x404 != 0x400 {
            return Err(Error::Quarantine);
        }
        self.nic.reset()?;
        if self.ouvert && !self.draine {
            return Err(Error::Quarantine);
        }
        Ok(())
    }
}
