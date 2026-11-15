//! Détail d'une complétion NVMe en erreur. Cette extension est réservée
//! aux images de qualification et ne transporte ni données ni adresse.
use crate::runtime as rt;

pub const LEN: usize = 8;

#[allow(dead_code)]
pub enum Layer {
    Block,
    Files,
    Application,
}

#[derive(Clone, Copy)]
pub struct Cause {
    cid: u16,
    status: u16,
}

impl Cause {
    pub fn new(cid: u64, status: u64) -> Option<Self> {
        let cid = u16::try_from(cid).ok()?;
        let status = u16::try_from(status).ok()?;
        (cid != 0 && status & 0x7ff != 0).then_some(Self { cid, status })
    }

    pub fn encode(self) -> [u8; LEN] {
        let mut bytes = *b"NCQ1\0\0\0\0";
        bytes[4..6].copy_from_slice(&self.cid.to_le_bytes());
        bytes[6..8].copy_from_slice(&self.status.to_le_bytes());
        bytes
    }

    pub fn decode(bytes: &[u8]) -> Option<Self> {
        if bytes.len() != LEN || &bytes[..4] != b"NCQ1" {
            return None;
        }
        let word = |at| u16::from_le_bytes([bytes[at], bytes[at + 1]]);
        Self::new(word(4).into(), word(6).into())
    }

    pub fn report(self, layer: Layer, code: u8) {
        let role: &[u8] = match layer {
            Layer::Block => b"bloc",
            Layer::Files => b"p9",
            Layer::Application => b"app",
        };
        let mut fields = *b" code=00 cid=0000 statut=0000";
        let digits = |mut n: u16, field: &mut [u8]| {
            for byte in field.iter_mut().rev() {
                *byte = b"0123456789abcdef"[(n & 15) as usize];
                n >>= 4;
            }
        };
        digits(code.into(), &mut fields[6..8]);
        digits(self.cid, &mut fields[13..17]);
        digits(self.status, &mut fields[25..29]);
        let prefix = b"NVME CQE ";
        let mut line = [0; 42];
        let at = prefix.len() + role.len();
        let len = at + fields.len();
        line[..prefix.len()].copy_from_slice(prefix);
        line[prefix.len()..at].copy_from_slice(role);
        line[at..len].copy_from_slice(&fields);
        rt::call(rt::DEBUG_WRITE, line.as_ptr() as u64, len as u64, 0);
    }
}
