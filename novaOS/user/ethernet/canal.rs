//! Un canal point à point, installé avant le lancement. La direction reverse
//! emploie la même page de 4 Kio après restitution par le destinataire.
use crate::runtime::*;
use core::sync::atomic::{AtomicU64, Ordering};
#[allow(dead_code)]
#[path = "lot.rs"]
mod lot;
#[cfg(ethernet_3b_incidents)]
#[allow(unused_imports)]
pub use lot::PREMIER_SEGMENT;
#[allow(unused_imports)]
pub use lot::Trames;
pub const CONTROL: u64 = 0x45544831;
pub const BYTES: u64 = 0x45544832;
pub const EVENT: u64 = 0x45544833;
pub const CATALOGUE: u64 = 0x45544835;
pub const DRIVER: u64 = 0x45544834;
pub const FRAME: usize = 1514;
pub fn cfg(i: usize) -> u64 {
    assert!(i < 128);
    unsafe { (&*(0x401000 as *const AtomicU64).add(i)).load(Ordering::Acquire) }
}
pub fn mark(i: usize, v: u64) {
    assert!(i < 128);
    unsafe { (&*(0x401000 as *const AtomicU64).add(i)).store(v, Ordering::Release) }
}
pub fn require(v: bool) -> Result<()> {
    if v { Ok(()) } else { Err(BAD_ARGUMENT) }
}
#[derive(Clone, Copy)]
pub struct Canal {
    pub tx: u64,
    pub rx: u64,
    pub page: u64,
    pub boite: bool,
}
impl Canal {
    pub fn new(tx: u64, rx: u64, page: u64) -> Result<Self> {
        let voie = |dir: u32| value(call(IPC_FEATURES, page, dir.into(), 1));
        let a = voie(SEND)?;
        let b = voie(RECEIVE)?;
        require(a == b && a <= 1)?;
        Ok(Self {
            tx,
            rx,
            page,
            boite: a == 1,
        })
    }
    pub fn envoyer(&self, tag: u64, bytes: &[u8]) -> Result<()> {
        send_for(self.tx, tag, bytes, 2500)
    }
    pub fn recevoir(&self, tag: u64) -> Result<ReceiveV1> {
        loop {
            let r = receive_for(self.rx, 2500)?;
            if r.kind == 2 && r.aux & !15 == 0 {
                continue;
            }
            require(r.kind == 1 && r.tag == tag && r.len <= 128)?;
            return Ok(r);
        }
    }
    fn mot(&self, key: u64) -> Result<()> {
        let msg = self.recevoir(BYTES)?;
        require(msg.len == 8 && msg.payload[..8] == key.to_le_bytes())
    }
    fn fragment(key: u64, i: usize, length: usize) -> [u8; 128] {
        let mut packet = [0; 128];
        packet[..8].copy_from_slice(&key.to_le_bytes());
        packet[8..10].copy_from_slice(&((i * 96) as u16).to_le_bytes());
        packet[10..12].copy_from_slice(&(length as u16).to_le_bytes());
        packet
    }
    pub fn copier_page(&self, op: u64, tag: u64, address: u64, length: usize) -> Result<()> {
        require(self.boite && tag != 0 && (1..=4096).contains(&length))?;
        let desc = BulkV1 {
            version: VERSION,
            size: 32,
            tag,
            address,
            length: length as u64,
        };
        require(value(call(op, self.page, (&raw const desc) as u64, 32))? == desc.length)
    }
    pub fn offrir(&self, key: u64, buf: &[u8]) -> Result<()> {
        require(key != 0 && !buf.is_empty() && buf.len() <= FRAME)?;
        if self.boite {
            self.copier_page(IPC_PUT, key, buf.as_ptr() as u64, buf.len())?;
            self.envoyer(BYTES, &key.to_le_bytes())?;
            return self.mot(key);
        }
        for (i, area) in buf.chunks(96).enumerate() {
            let mut packet = Self::fragment(key, i, buf.len());
            packet[32..32 + area.len()].copy_from_slice(area);
            self.envoyer(BYTES, &packet[..32 + area.len()])?;
        }
        Ok(())
    }
    pub fn prendre(&self, key: u64, buf: &mut [u8]) -> Result<()> {
        let length = buf.len();
        require(key != 0 && length != 0 && length <= FRAME)?;
        if self.boite {
            self.mot(key)?;
            self.copier_page(IPC_TAKE, key, buf.as_mut_ptr() as u64, length)?;
            return self.envoyer(BYTES, &key.to_le_bytes());
        }
        for (i, area) in buf.chunks_mut(96).enumerate() {
            let packet = Self::fragment(key, i, length);
            let msg = self.recevoir(BYTES)?;
            require(msg.len as usize == 32 + area.len() && msg.payload[..32] == packet[..32])?;
            area.copy_from_slice(&msg.payload[32..32 + area.len()]);
        }
        Ok(())
    }
}
pub fn pair(op: u64, arg: u64) -> [u8; 16] {
    let mut out = [0; 16];
    out[..8].copy_from_slice(&op.to_le_bytes());
    out[8..].copy_from_slice(&arg.to_le_bytes());
    out
}
pub fn fields(msg: &ReceiveV1) -> Result<(u64, u64)> {
    require(msg.len == 16)?;
    Ok((
        u64::from_le_bytes(msg.payload[..8].try_into().unwrap()),
        u64::from_le_bytes(msg.payload[8..16].try_into().unwrap()),
    ))
}
