//! Encadrement privé, sans autorité de sécurité. Les signatures restent vérifiées par R.
use crate::{Error, Result};
pub const MAX: usize = 2048;
pub const HEADER: usize = 16;
pub const PART: usize = 96;
pub const TAG: u64 = u64::from_le_bytes(*b"NARPC002");
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Header {
    pub sequence: u32,
    pub operation: u16,
    pub size: u16,
}
impl Header {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        if bytes.len() != HEADER || &bytes[..8] != b"NARPC002" {
            return Err(Error::Format);
        }
        let h = Self {
            sequence: u32::from_be_bytes(bytes[8..12].try_into().unwrap()),
            operation: u16::from_be_bytes(bytes[12..14].try_into().unwrap()),
            size: u16::from_be_bytes(bytes[14..16].try_into().unwrap()),
        };
        if h.sequence == 0 || h.operation == 0 || usize::from(h.size) > MAX - HEADER {
            return Err(Error::Limit);
        }
        Ok(h)
    }
    pub fn encode(self) -> Result<[u8; HEADER]> {
        let mut b = [0; HEADER];
        b[..8].copy_from_slice(b"NARPC002");
        b[8..12].copy_from_slice(&self.sequence.to_be_bytes());
        b[12..14].copy_from_slice(&self.operation.to_be_bytes());
        b[14..].copy_from_slice(&self.size.to_be_bytes());
        Self::parse(&b)?;
        Ok(b)
    }
    pub fn frame(self, offset: usize, body: &[u8]) -> Result<[u8; 128]> {
        if body.len() != usize::from(self.size)
            || offset % PART != 0
            || offset > body.len()
            || (offset == body.len() && offset != 0)
        {
            return Err(Error::Order);
        }
        let n = (body.len() - offset).min(PART);
        let mut out = [0; 128];
        out[..16].copy_from_slice(&self.encode()?);
        out[16..18].copy_from_slice(&(offset as u16).to_be_bytes());
        out[18..20].copy_from_slice(&(n as u16).to_be_bytes());
        out[32..32 + n].copy_from_slice(&body[offset..offset + n]);
        Ok(out)
    }
}
/// L'appelant possède la mémoire bornée. Aucun tampon ni allocation cachés.
pub struct Assembly {
    header: Header,
    next: usize,
    complete: bool,
}
impl Assembly {
    pub fn new(header: Header) -> Result<Self> {
        header.encode()?;
        Ok(Self {
            header,
            next: 0,
            complete: false,
        })
    }
    pub fn push(&mut self, frame: &[u8; 128], destination: &mut [u8]) -> Result<bool> {
        let h = Header::parse(&frame[..16])?;
        let offset = u16::from_be_bytes(frame[16..18].try_into().unwrap()) as usize;
        let n = u16::from_be_bytes(frame[18..20].try_into().unwrap()) as usize;
        let remaining = usize::from(h.size)
            .checked_sub(self.next)
            .ok_or(Error::Order)?;
        if self.complete
            || h != self.header
            || offset != self.next
            || n != remaining.min(PART)
            || destination.len() != usize::from(h.size)
            || frame[20..32] != [0; 12]
            || frame[32 + n..].iter().any(|b| *b != 0)
        {
            return Err(Error::Order);
        }
        destination[offset..offset + n].copy_from_slice(&frame[32..32 + n]);
        self.next += n;
        self.complete = self.next == usize::from(h.size);
        Ok(self.complete)
    }
}
