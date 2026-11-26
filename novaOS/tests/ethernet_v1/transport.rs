//! Modèles CPU d'un lien privé. Un seul lot en préparation, sans pointeur DMA.
use super::{Binding, Error, Result};
use crate::wire::{self, FRAME, HEADER, LIMIT};
use core::hint::black_box;

pub const WINDOW: usize = FRAME * LIMIT;
#[derive(Default, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cost {
    pub calls: u64,
    pub copies: u64,
    pub bytes: u64,
}
/// Les participants et les buffers sont installés par le lanceur du banc.
pub struct Channel<const N: usize> {
    src: Binding,
    dst: Binding,
    buffer: [u8; N],
    live: bool,
    last: u64,
    pending: Option<u64>,
    filled: usize,
    tag: u64,
}
impl<const N: usize> Channel<N> {
    pub fn new(src: Binding, dst: Binding) -> Result<Self> {
        if !matches!(N, 128 | 4096 | WINDOW)
            || src == dst
            || src.owner == dst.owner
            || src.boot != dst.boot
            || src.port != dst.port
            || src.epoch != dst.epoch
            || src.channel == 0
            || dst.channel == 0
        {
            return Err(Error::Denied);
        }
        Ok(Self {
            src,
            dst,
            buffer: [0; N],
            live: true,
            last: 0,
            pending: None,
            filled: 0,
            tag: 0,
        })
    }
    fn check(&self, caller: Binding, expected: Binding) -> Result<()> {
        if caller != expected {
            return Err(Error::Denied);
        }
        if !self.live {
            return Err(Error::Closed);
        }
        Ok(())
    }
    pub fn begin(&mut self, who: Binding, seq: u64) -> Result<()> {
        self.check(who, self.src)?;
        if self.pending.is_some() {
            return Err(Error::Full);
        }
        if self.last.checked_add(1) != Some(seq) {
            return Err(Error::Stale);
        }
        self.pending = Some(seq);
        Ok(())
    }
    fn put(&mut self, who: Binding, seq: u64, offset: usize, part: &[u8]) -> Result<()> {
        self.check(who, self.src)?;
        if self.pending != Some(seq) {
            return Err(Error::Stale);
        }
        if self.filled != 0 {
            return Err(Error::Full);
        }
        let end = offset.checked_add(part.len()).ok_or(Error::Invalid)?;
        if part.is_empty() || end > N {
            return Err(Error::Invalid);
        }
        self.buffer[offset..end].copy_from_slice(part);
        black_box(&self.buffer[..end]);
        self.filled = end;
        self.tag = seq;
        Ok(())
    }
    fn take(&mut self, who: Binding, seq: u64, offset: usize, out: &mut [u8]) -> Result<()> {
        self.check(who, self.dst)?;
        let end = offset.checked_add(out.len()).ok_or(Error::Invalid)?;
        if self.pending != Some(seq) || self.tag != seq || out.is_empty() || self.filled != end {
            return Err(Error::State);
        }
        out.copy_from_slice(&self.buffer[offset..end]);
        if N == 4096 {
            self.buffer[..end].fill(0);
        }
        self.filled = 0;
        Ok(())
    }
    pub fn transfer(
        &mut self,
        producer: Binding,
        consumer: Binding,
        command: &[u8],
        source: &[u8],
        dest: &mut [u8],
    ) -> Result<Cost> {
        self.check(producer, self.src)?;
        self.check(consumer, self.dst)?;
        let (h, body) = wire::Header::decode(command).map_err(|_| Error::Invalid)?;
        if h.op != wire::TX || h.reply || h.epoch != self.src.epoch {
            return Err(Error::Invalid);
        }
        if self.pending != Some(h.request) {
            return Err(Error::Stale);
        }
        let mut lengths = [0usize; LIMIT];
        for (i, d) in body.chunks_exact(wire::ENTRY).enumerate() {
            // Ce banc compact range les slots en ordre du lot, sans trous.
            if usize::from(wire::word(d, 0)) != i {
                return Err(Error::Invalid);
            }
            lengths[i] = usize::from(wire::word(d, 2));
        }
        let count = usize::from(h.count);
        let total: usize = lengths[..count].iter().sum();
        if source.len() != total || dest.len() != total {
            return Err(Error::Invalid);
        }
        let mut cost = Cost {
            calls: 4,
            copies: 0,
            bytes: 2 * total as u64,
        };
        if N == 128 {
            let mut sender = [0u8; wire::FRAGMENT];
            let mut receiver = [0u8; wire::FRAGMENT];
            cost.bytes = 4 * total as u64;
            let mut pos = 0;
            for (index, &len) in lengths[..count].iter().enumerate() {
                for offset in (0..len).step_by(wire::FRAGMENT) {
                    let n = wire::FRAGMENT.min(len - offset);
                    self.buffer[..HEADER].fill(0);
                    self.buffer[..8].copy_from_slice(&h.request.to_le_bytes());
                    self.buffer[8..10].copy_from_slice(&(index as u16).to_le_bytes());
                    self.buffer[10..12].copy_from_slice(&(offset as u16).to_le_bytes());
                    self.buffer[12..14].copy_from_slice(&(len as u16).to_le_bytes());
                    sender[..n].copy_from_slice(&source[pos..pos + n]);
                    self.put(producer, h.request, HEADER, black_box(&sender[..n]))?;
                    let packet = black_box(&self.buffer);
                    if u64::from_le_bytes(packet[..8].try_into().unwrap()) != h.request
                        || wire::word(packet, 8) as usize != index
                        || wire::word(packet, 10) as usize != offset
                        || wire::word(packet, 12) as usize != len
                        || packet[14..32] != [0; 18]
                    {
                        return Err(Error::Conflict);
                    }
                    self.take(consumer, h.request, HEADER, &mut receiver[..n])?;
                    dest[pos..pos + n].copy_from_slice(black_box(&receiver[..n]));
                    cost.copies += 4;
                    cost.calls += 2;
                    pos += n;
                }
            }
        } else if N == 4096 {
            let mut first = 0;
            let mut pos = 0;
            while first < count {
                let mut size = 0;
                while first < count && size + lengths[first] <= N {
                    size += lengths[first];
                    first += 1;
                }
                self.put(producer, h.request, 0, &source[pos..pos + size])?;
                self.take(consumer, h.request, 0, &mut dest[pos..pos + size])?;
                pos += size;
                cost.copies += 2;
                cost.calls += 2;
            }
        } else {
            // Tous les slots sont validés avant la première copie. Snapshot puis remise.
            let mut pos = 0;
            for &len in &lengths[..count] {
                self.buffer[pos..pos + len].copy_from_slice(&source[pos..pos + len]);
                pos += len;
            }
            black_box(&self.buffer[..total]);
            pos = 0;
            for &len in &lengths[..count] {
                dest[pos..pos + len].copy_from_slice(&self.buffer[pos..pos + len]);
                pos += len;
            }
            cost.copies = 2 * count as u64;
        }
        self.last = h.request;
        self.pending = None;
        Ok(cost)
    }
    pub fn revoke(&mut self) {
        self.live = false;
        self.pending = None;
        self.filled = 0;
        self.tag = 0;
        self.buffer.fill(0);
        black_box(&self.buffer);
    }
    #[cfg(test)]
    pub(crate) fn partial_box_test(&mut self, producer: Binding, consumer: Binding) {
        self.begin(producer, 1).unwrap();
        self.put(producer, 1, 0, &[0xa5; 64]).unwrap();
        assert_eq!(self.put(producer, 1, 0, &[7; 64]), Err(Error::Full));
        let mut out = [0x55; 64];
        assert_eq!(self.take(producer, 1, 0, &mut out), Err(Error::Denied));
        assert_eq!(self.take(consumer, 2, 0, &mut out), Err(Error::State));
        assert_eq!(self.take(consumer, 1, 0, &mut out[..63]), Err(Error::State));
        assert_eq!(out, [0x55; 64]);
        self.revoke();
        assert_eq!(self.take(consumer, 1, 0, &mut out), Err(Error::Closed));
        assert!(self.clean());
    }
    #[cfg(test)]
    pub(crate) fn clean(&self) -> bool {
        self.buffer.iter().all(|&b| b == 0) && self.filled == 0 && self.pending.is_none()
    }
}
