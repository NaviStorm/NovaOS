//! Seule la paire donnée au gestionnaire permet de lire et publier sa configuration.
use super::runtime as rt;
#[allow(dead_code)]
#[path = "../../abi/files.rs"]
mod files;
#[path = "config_wire.rs"]
mod wire;
pub use files::{BUSY, CORRUPT, UNCERTAIN};
use wire::{Frame, get, request};

pub struct Client {
    requests: u64,
    replies: u64,
    tag: u64,
}
impl Client {
    pub fn new(requests: u64, replies: u64) -> Self {
        Self {
            requests,
            replies,
            tag: 0,
        }
    }

    fn exchange(&mut self, q: Frame) -> rt::Result<Frame> {
        if self.requests == 0 || self.replies == 0 {
            return Err(rt::DENIED);
        }
        self.tag = self.tag.checked_add(1).ok_or(rt::LIMIT)?;
        let op = get(&q, 8);
        rt::send_frame(self.requests, self.tag, &q, 0, false)?;
        // Après l’admission de COMMIT, la perte de sa réponse est ambiguë.
        let unconfirmed = |e| if op == wire::COMMIT { UNCERTAIN } else { e };
        for _ in 0..16 {
            let msg = rt::receive_for(self.replies, 15000).map_err(unconfirmed)?;
            if msg.tag < self.tag {
                continue;
            }
            if msg.kind != 1
                || msg.len != 128
                || msg.tag != self.tag
                || get(&msg.payload, 0) != wire::VERSION
                || get(&msg.payload, 8) != op
            {
                return Err(unconfirmed(rt::REFUSED));
            }
            let status = get(&msg.payload, 16);
            return if status == 0 {
                Ok(msg.payload)
            } else {
                Err(status)
            };
        }
        Err(unconfirmed(rt::REFUSED))
    }

    fn read(&mut self, op: u64, dst: &mut [u8]) -> rt::Result<(u64, usize)> {
        let mut generation = 0;
        let mut offset = 0;
        let mut total = 0;
        loop {
            let f = self.exchange(request(op, generation, offset, 0))?;
            let size = usize::try_from(get(&f, 40)).map_err(|_| rt::LIMIT)?;
            let seen = get(&f, 24);
            if offset == 0 {
                generation = seen;
                total = size;
            }
            if total != size
                || generation != seen
                || get(&f, 32) != offset as u64
                || size > wire::LIMIT
                || size > dst.len()
                || offset > size
                || (seen == 0) != (size == 0)
            {
                return Err(rt::REFUSED);
            }
            let n = (size - offset).min(wire::CHUNK);
            dst[offset..offset + n].copy_from_slice(&f[wire::DATA..wire::DATA + n]);
            offset += n;
            if offset == size {
                return Ok((generation, size));
            }
        }
    }
    pub fn load(&mut self, dst: &mut [u8]) -> rt::Result<(u64, usize)> {
        self.read(wire::LOAD, dst)
    }
    pub fn previous(&mut self, dst: &mut [u8]) -> rt::Result<(u64, usize)> {
        self.read(wire::PREVIOUS, dst)
    }
    pub fn save(&mut self, expected: u64, data: &[u8]) -> rt::Result<u64> {
        if data.is_empty() || data.len() > wire::LIMIT {
            return Err(rt::LIMIT);
        }
        let next = expected.checked_add(1).ok_or(rt::LIMIT)?;
        self.exchange(request(wire::BEGIN, expected, 0, data.len()))?;
        for (i, part) in data.chunks(wire::CHUNK).enumerate() {
            let mut f = request(wire::PART, expected, i * wire::CHUNK, part.len());
            f[wire::DATA..wire::DATA + part.len()].copy_from_slice(part);
            self.exchange(f)?;
        }
        let f = self.exchange(request(wire::COMMIT, expected, 0, 0))?;
        if get(&f, 24) != next {
            return Err(UNCERTAIN);
        }
        Ok(next)
    }
}
