//! Une rafale de qualification. Descripteurs et buffers restent au superviseur.
use super::{
    Device, Error, Frame, Result,
    io::{Region, barrier},
};
use crate::{abi, hal};
const N: usize = 16;
#[cfg(feature="ethernet-v1-port")]
mod v1;

pub(super) struct Port<'a> {
    #[cfg(feature="ethernet-v1-port")]
    flux: v1::Flux,
    nic: &'a Device,
    cadres: &'a [Frame; 22],
    kicks: [Region; 2],
    total_rx: u32,
    total_tx: u32,
    tours: u32,
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
            #[cfg(feature="ethernet-v1-port")]
            flux: v1::Flux::default(),
            nic,
            cadres,
            kicks,
            total_rx: 0,
            total_tx: 0,
            tours: 0,
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
        #[cfg(feature="ethernet-v1-port")]
        if tag != 1 {
            if matches!(tag, 7 | 8) && !irq { return Err(abi::WOULD_BLOCK); }
            return self.v1(tag, len, copie);
        }
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
                    if self.total_tx == 0 {
                        hal::log(format_args!("NET L2 TX soumis=16 longueurs alternées 72/1526"));
                    }
                    self.depart = hal::timer::counter();
                    barrier();
                    self.cadres[4].mem.w16(2, (self.total_tx as u16).wrapping_add(N as u16));
                    barrier();
                    self.kicks[1].w16(0, 1);
                }
                Ok(0)
            }
            2 if self.ouvert && self.postes == N && self.livres == 0 => Err(abi::WOULD_BLOCK),
            3 | 7 | 8 if self.ouvert && self.livres < N => {
                let prendre_rx = tag != 7;
                let prendre_tx = tag != 8;
                if prendre_tx && self.postes != N { return Err(abi::DENIED); }
                let (rx,tx)=(self.cadres[2].mem,self.cadres[5].mem);
                let r=rx.r16(2).wrapping_sub(self.total_rx as u16);
                let t=tx.r16(2).wrapping_sub(self.total_tx as u16);
                if r>16 || usize::from(t)>self.postes {return Err(abi::REFUSED);}
                let i=self.livres;
                if !irq || (prendre_rx && usize::from(r)<=i) || (prendre_tx && usize::from(t)<=i) {return Err(abi::WOULD_BLOCK);}
                barrier();
                let pos=4+8*i;
                let mut n=0;
                if prendre_rx {
                    let id=rx.r32(pos) as usize;
                    n=rx.r32(pos+4) as usize;
                    if id>=N || self.rx & (1<<id)!=0 || !(26..=1526).contains(&n) {return Err(abi::REFUSED);}
                    self.cadres[6+id/2].mem.bytes(id%2*2048,&mut copie[..n]);
                    self.rx |= 1<<id;
                }
                if prendre_tx {
                    let id=tx.r32(pos) as usize;
                    if id>=N || self.tx & (1<<id)!=0 || tx.r32(pos+4)!=0 {return Err(abi::REFUSED);}
                    self.tx |= 1<<id;
                }
                self.collectes[i]=hal::timer::counter().wrapping_sub(self.depart);
                self.livres+=1;
                Ok(n)
            }
            6 if self.ouvert && self.livres==N => {
                if !matches!(self.rx, 0 | u16::MAX) || !matches!(self.tx, 0 | u16::MAX) || self.rx|self.tx!=u16::MAX {
                    return Err(abi::REFUSED);
                }
                let rx_plein=self.rx==u16::MAX;
                let tx_plein=self.tx==u16::MAX;
                if self.postes!=if tx_plein {N} else {0} {return Err(abi::REFUSED);}
                let precedent=self.total_rx as u16;
                if tx_plein {self.total_tx+=N as u32;}
                if rx_plein {self.total_rx+=N as u32;}
                if self.cadres[5].mem.r16(2)!=self.total_tx as u16 || self.cadres[2].mem.r16(2).wrapping_sub(self.total_rx as u16)>if rx_plein {0} else {16} {
                    return Err(abi::REFUSED);
                }
                self.tours+=1;
                #[cfg(net_p9_coexistence)]
                super::net_stockage::progression(self.total_rx);
                if rx_plein && ((self.total_rx as u16)<precedent || self.total_rx%8192==0) {
                    hal::log(format_args!("NET L2 progression={} index={} precedent={}",self.total_rx,self.total_rx as u16,precedent));
                }
                self.postes=0;
                self.livres=0;
                self.rx=0;
                self.tx=0;
                let avail=self.cadres[1].mem;
                self.cadres[4].mem.w16(36,self.total_tx as u16);
                avail.w16(36,self.total_rx as u16);
                if rx_plein {
                    for i in 0..N {self.cadres[6+i/2].mem.w16(i%2*2048+10,0x5a5a);}
                    barrier();
                    avail.w16(2,(self.total_rx as u16).wrapping_add(N as u16));
                    barrier();
                    self.kicks[0].w16(0,0);
                }
                Ok(0)
            }
            4 if self.livres == N && self.rx == u16::MAX && self.tx == u16::MAX => {
                let fin=(self.total_rx as u16).wrapping_add(N as u16);
                if self.cadres[2].mem.r16(2)!=fin || self.cadres[5].mem.r16(2)!=(self.total_tx as u16).wrapping_add(N as u16) {return Err(abi::REFUSED);}
                self.draine = true;
                hal::log(format_args!("NET L2 total TX={} RX={} index={} rearmements={}",self.total_tx+16,self.total_rx+16,fin,self.tours));
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
