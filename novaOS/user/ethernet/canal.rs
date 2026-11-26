//! Un canal point à point, installé avant le lancement. La direction reverse
//! emploie la même page de 4 Kio après restitution par le destinataire.
use crate::runtime::*;
use core::sync::atomic::{AtomicU64, Ordering};
pub const CONTROL: u64 = 0x45544831;
pub const BYTES: u64 = 0x45544832;
pub const EVENT: u64 = 0x45544833;
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
    pub fn donnees(&self, put: bool, key: u64, buf: &mut [u8]) -> Result<()> {
        require(key != 0 && !buf.is_empty() && buf.len() <= FRAME)?;
        if self.boite {
            let desc = BulkV1 {
                version: VERSION,
                size: 32,
                tag: key,
                address: buf.as_mut_ptr() as u64,
                length: buf.len() as u64,
            };
            if put {
                let n = value(call(IPC_PUT, self.page, &desc as *const BulkV1 as u64, 32))?;
                require(n == desc.length)?;
                self.envoyer(BYTES, &key.to_le_bytes())?;
                let ack = self.recevoir(BYTES)?;
                require(ack.len == 8 && ack.payload[..8] == key.to_le_bytes())
            } else {
                let msg = self.recevoir(BYTES)?;
                require(msg.len == 8 && msg.payload[..8] == key.to_le_bytes())?;
                let n = value(call(IPC_TAKE, self.page, &desc as *const BulkV1 as u64, 32))?;
                require(n == desc.length)?;
                self.envoyer(BYTES, &key.to_le_bytes())
            }
        } else {
            let length = buf.len();
            for (i, area) in buf.chunks_mut(96).enumerate() {
                let mut packet = [0u8; 128];
                packet[..8].copy_from_slice(&key.to_le_bytes());
                packet[8..10].copy_from_slice(&((i * 96) as u16).to_le_bytes());
                packet[10..12].copy_from_slice(&(length as u16).to_le_bytes());
                if put {
                    packet[32..32 + area.len()].copy_from_slice(area);
                    self.envoyer(BYTES, &packet[..32 + area.len()])?;
                } else {
                    let msg = self.recevoir(BYTES)?;
                    require(
                        msg.len as usize == 32 + area.len() && msg.payload[..32] == packet[..32],
                    )?;
                    area.copy_from_slice(&msg.payload[32..32 + area.len()]);
                }
            }
            Ok(())
        }
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
