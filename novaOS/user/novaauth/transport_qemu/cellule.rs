//! Codec partagé entre la sonde sans allocateur et l’adaptateur de qualification.
pub const PAGE: usize = 4096;
pub const HEADER: usize = 64;
pub const MAX: usize = 512 - HEADER;
pub const OPEN: u8 = 1;
pub const DATA: u8 = 2;
pub const EMPTY: u8 = 3;
pub const ACK: u8 = 4;
pub const CLOSE: u8 = 5;
pub const BROKEN: u8 = 6;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Cell {
    pub kind: u8,
    pub size: u16,
    pub link: [u8; 16],
    pub sequence: u64,
    pub ack: u64,
}

impl Cell {
    fn valid(self) -> bool {
        (OPEN..=BROKEN).contains(&self.kind)
            && usize::from(self.size) <= MAX
            && (self.kind == DATA) == (self.size > 0)
    }

    pub fn encode(self, out: &mut [u8]) -> Result<(), ()> {
        if out.len() != HEADER || !self.valid() {
            return Err(());
        }
        out.fill(0);
        out[..4].copy_from_slice(b"NA24");
        out[4] = 1;
        out[5] = self.kind;
        out[6..8].copy_from_slice(&self.size.to_le_bytes());
        out[8..24].copy_from_slice(&self.link);
        out[24..32].copy_from_slice(&self.sequence.to_le_bytes());
        out[32..40].copy_from_slice(&self.ack.to_le_bytes());
        Ok(())
    }

    pub fn decode(src: &[u8]) -> Result<Self, ()> {
        if src.len() != HEADER
            || &src[..4] != b"NA24"
            || src[4] != 1
            || src[40..].iter().any(|b| *b != 0)
        {
            return Err(());
        }
        let cell = Self {
            kind: src[5],
            size: u16::from_le_bytes(src[6..8].try_into().unwrap()),
            link: src[8..24].try_into().unwrap(),
            sequence: u64::from_le_bytes(src[24..32].try_into().unwrap()),
            ack: u64::from_le_bytes(src[32..40].try_into().unwrap()),
        };
        cell.valid().then_some(cell).ok_or(())
    }
}

pub fn decode_sector(src: &[u8]) -> Result<Cell, ()> {
    if !matches!(src.len(), 512 | PAGE) {
        return Err(());
    }
    let cell = Cell::decode(&src[..HEADER])?;
    if src[HEADER + usize::from(cell.size)..]
        .iter()
        .any(|b| *b != 0)
    {
        return Err(());
    }
    Ok(cell)
}

const _: () = assert!(HEADER <= 256);
