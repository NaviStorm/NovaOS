// Enveloppes Ethernet privées v1.0, indépendantes du layout Rust.
pub const HEADER: usize = 32;
pub const ENTRY: usize = 12;
pub const LIMIT: usize = 8;
pub const FRAME: usize = 1514;
pub const FRAGMENT: usize = 96;
pub const NEGOTIATE: u16 = 1;
pub const OPEN: u16 = 2;
pub const TX: u16 = 3;
pub const QUERY: u16 = 4;
pub const RX: u16 = 5;
pub const RELEASE: u16 = 6;
pub const ARM: u16 = 7;
pub const LINK: u16 = 8;
pub const STATS: u16 = 9;
pub const JOIN: u16 = 10;
pub const LEAVE: u16 = 11;
pub const CLOSE: u16 = 12;
pub const ACK: u16 = 13;
pub const SET_LINK: u16 = 14;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WireError {
    Size,
    Version,
    Reserved,
    Opcode,
    Range,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Header {
    pub op: u16,
    pub reply: bool,
    pub count: u16,
    pub epoch: u64,
    pub request: u64,
}
pub fn word(b: &[u8], i: usize) -> u16 {
    u16::from_le_bytes([b[i], b[i + 1]])
}
fn wide(b: &[u8], i: usize) -> u64 {
    u64::from_le_bytes(b[i..i + 8].try_into().unwrap())
}

impl Header {
    fn body(self) -> Result<usize, WireError> {
        if self.epoch == 0 || self.request == 0 || usize::from(self.count) > LIMIT {
            return Err(WireError::Range);
        }
        let list = matches!(
            (self.op, self.reply),
            (TX, false) | (RELEASE, false) | (QUERY, true) | (RX, true) | (TX, true)
        );
        if list {
            if self.count == 0 && !matches!((self.op, self.reply), (RX, true)) {
                return Err(WireError::Range);
            }
            return Ok(usize::from(self.count) * ENTRY);
        }
        if self.count != 0 {
            return Err(WireError::Range);
        }
        Ok(match (self.op, self.reply) {
            (NEGOTIATE, _) => 16,
            (OPEN, false) => 8,
            (OPEN, true) => 16,
            (QUERY, false) | (ACK, false) | (ARM, false) => 8,
            (ARM, true) => 16,
            (LINK, true) | (SET_LINK, false) => 16,
            (STATS, true) => 64,
            (JOIN | LEAVE, false) => 6,
            (LINK | STATS | RX | CLOSE, false)
            | (RELEASE | JOIN | LEAVE | CLOSE | ACK | SET_LINK, true) => 0,
            _ => return Err(WireError::Opcode),
        })
    }

    pub fn encode(self, out: &mut [u8; 128], payload: &[u8]) -> Result<usize, WireError> {
        let n = self.body()?;
        if payload.len() != n {
            return Err(WireError::Size);
        }
        out.fill(0);
        for (i, v) in [
            1,
            0,
            self.op,
            u16::from(self.reply),
            (HEADER + n) as u16,
            self.count,
            0,
            0,
        ]
        .iter()
        .enumerate()
        {
            out[2 * i..2 * i + 2].copy_from_slice(&v.to_le_bytes());
        }
        out[16..24].copy_from_slice(&self.epoch.to_le_bytes());
        out[24..32].copy_from_slice(&self.request.to_le_bytes());
        out[HEADER..HEADER + n].copy_from_slice(payload);
        Self::decode(&out[..HEADER + n])?;
        Ok(HEADER + n)
    }

    pub fn decode(data: &[u8]) -> Result<(Self, &[u8]), WireError> {
        if !(HEADER..=128).contains(&data.len()) || usize::from(word(data, 8)) != data.len() {
            return Err(WireError::Size);
        }
        if word(data, 0) != 1 || word(data, 2) != 0 {
            return Err(WireError::Version);
        }
        if word(data, 6) > 1 || word(data, 12) != 0 || word(data, 14) != 0 {
            return Err(WireError::Reserved);
        }
        let h = Self {
            op: word(data, 4),
            reply: word(data, 6) == 1,
            count: word(data, 10),
            epoch: wide(data, 16),
            request: wide(data, 24),
        };
        if HEADER + h.body()? != data.len() {
            return Err(WireError::Size);
        }
        let body = &data[HEADER..];
        if (h.op == TX || h.op == RELEASE) && !h.reply {
            let mut seen = 0u16;
            for d in body.chunks_exact(ENTRY) {
                let slot = word(d, 0);
                let len = word(d, 2);
                if usize::from(slot) >= LIMIT
                    || seen & (1 << slot) != 0
                    || d[4..8] == [0; 4]
                    || d[8..12] != [0; 4]
                    || (h.op == TX && !(14..=FRAME as u16).contains(&len))
                    || (h.op == RELEASE && len != 0)
                {
                    return Err(WireError::Range);
                }
                seen |= 1 << slot;
            }
        }
        if h.reply && matches!(h.op, TX | QUERY) {
            let mut id = None;
            for (i, d) in body.chunks_exact(ENTRY).enumerate() {
                let req = wide(d, 4);
                if usize::from(word(d, 0)) != i
                    || word(d, 2) > 5
                    || req == 0
                    || (h.op == TX && req != h.request)
                    || id.is_some_and(|n| n != req)
                {
                    return Err(WireError::Range);
                }
                id = Some(req);
            }
        }
        if h.reply && h.op == RX {
            let mut used = 0u16;
            for d in body.chunks_exact(ENTRY) {
                let s = word(d, 0);
                if s >= 8
                    || used & (1 << s) != 0
                    || !(60..=FRAME as u16).contains(&word(d, 2))
                    || d[4..8] == [0; 4]
                    || d[8..12] != [0; 4]
                {
                    return Err(WireError::Range);
                }
                used |= 1 << s;
            }
        }
        Ok((h, body))
    }
}

pub fn descriptor(slot: u16, len: u16, generation: u32) -> [u8; ENTRY] {
    let mut data = [0; ENTRY];
    data[..2].copy_from_slice(&slot.to_le_bytes());
    data[2..4].copy_from_slice(&len.to_le_bytes());
    data[4..8].copy_from_slice(&generation.to_le_bytes());
    data
}
