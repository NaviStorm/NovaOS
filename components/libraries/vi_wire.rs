//! Dialogue privé entre nsh et l'ELF vi. Aucun de ces messages n'est un appel noyau ou NovaFS.
use crate::runtime as rt;

pub fn length(data: &[u8]) -> rt::Result<usize> {
    let n = u64::from_le_bytes(data.try_into().map_err(|_| rt::BAD_ARGUMENT)?);
    usize::try_from(n).map_err(|_| rt::LIMIT)
}
pub const CHUNK: usize = 127;
pub const INFO: u8 = 1;
pub const READ: u8 = 2;
pub const KEY: u8 = 3;
pub const BEGIN: u8 = 4;
pub const NAME: u8 = 5;
pub const TEXT: u8 = 6;
pub const SAVE: u8 = 7;
pub const FIN: u8 = 8;

pub struct Link {
    channels: (u64, u64),
    tag: u64,
}
impl Link {
    pub fn new() -> rt::Result<Self> {
        Ok(Self {
            channels: rt::channels()?,
            tag: 0,
        })
    }
    pub fn ask(&mut self, op: u8, bytes: &[u8]) -> rt::Result<rt::ReceiveV1> {
        if bytes.len() > CHUNK {
            return Err(rt::LIMIT);
        }
        let mut frame = [0; 128];
        frame[0] = op;
        frame[1..1 + bytes.len()].copy_from_slice(bytes);
        self.tag = self.tag.checked_add(1).ok_or(rt::LIMIT)?;
        rt::send_for(self.channels.0, self.tag, &frame[..1 + bytes.len()], 15000)?;
        // Après livraison synchrone de SAVE, seule une réponse valide atteste le résultat.
        let missing_reply = |e| {
            if op == SAVE {
                crate::protocol::UNCERTAIN
            } else {
                e
            }
        };
        let reply = loop {
            match rt::receive_for(self.channels.1, 15000) {
                Err(rt::TIMED_OUT) if matches!(op, KEY | SAVE) => continue,
                r => break r.map_err(missing_reply)?,
            }
        };
        if reply.kind != 1 || reply.tag != self.tag || !(1..=128).contains(&reply.len) {
            return Err(missing_reply(rt::BAD_ARGUMENT));
        }
        if reply.payload[0] != 0 {
            return Err(reply.payload[0].into());
        }
        Ok(reply)
    }
}
