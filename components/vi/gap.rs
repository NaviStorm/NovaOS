//! Les octets précédant et suivant l'espace libre sont deux chaînes UTF-8 complètes.
use alloc::{string::String, vec::Vec};
use core::ops::Range;

pub struct Gap {
    bytes: Vec<u8>,
    left: usize,
    right: usize,
    row: usize,
}

fn lines(bytes: &[u8]) -> usize {
    bytes.iter().filter(|&&b| b == b'\n').count()
}

impl Gap {
    pub fn new(text: String) -> Self {
        let end = text.len();
        let row = lines(text.as_bytes());
        Self {
            bytes: text.into_bytes(),
            left: end,
            right: end,
            row,
        }
    }
    #[cfg(test)]
    pub fn capacity(&self) -> usize {
        self.bytes.capacity()
    }
    pub fn len(&self) -> usize {
        self.bytes.len() - (self.right - self.left)
    }
    pub fn parts(&self, r: Range<usize>) -> [&str; 2] {
        // Sûreté : le constructeur reçoit un String. move_to et replace vérifient
        // les deux frontières avant de déplacer des octets ou de retirer un scalaire.
        let (before, after) = unsafe {
            (
                core::str::from_utf8_unchecked(&self.bytes[..self.left]),
                core::str::from_utf8_unchecked(&self.bytes[self.right..]),
            )
        };
        [
            &before[r.start.min(self.left)..r.end.min(self.left)],
            &after[r.start.saturating_sub(self.left)..r.end.saturating_sub(self.left)],
        ]
    }
    fn boundary(&self, at: usize) -> bool {
        if at >= self.len() {
            return at == self.len();
        }
        let physical = if at < self.left {
            at
        } else {
            self.right + (at - self.left)
        };
        self.bytes[physical] & 0xc0 != 0x80
    }
    pub fn row(&self, at: usize) -> usize {
        if at < self.left && at < self.left - at {
            lines(&self.bytes[..at])
        } else if at < self.left {
            self.row - lines(&self.bytes[at..self.left])
        } else {
            self.row + lines(&self.bytes[self.right..self.right + (at - self.left)])
        }
    }
    fn move_to(&mut self, at: usize) {
        assert!(self.boundary(at));
        self.row = self.row(at);
        if at < self.left {
            let n = self.left - at;
            self.bytes.copy_within(at..self.left, self.right - n);
            self.right -= n;
        } else if at > self.left {
            let n = at - self.left;
            self.bytes
                .copy_within(self.right..self.right + n, self.left);
            self.right += n;
        }
        self.left = at;
    }
    pub fn reserve(&mut self, additional: usize) -> Result<(), &'static str> {
        let needed = additional.saturating_sub(self.right - self.left);
        if needed == 0 {
            return Ok(());
        }
        // Une grande ligne ne double pas l'empreinte du tampon à la première frappe.
        let step = (self.len() / 4).clamp(64, 65536);
        self.bytes
            .try_reserve_exact(needed.max(step))
            .or_else(|_| self.bytes.try_reserve_exact(needed))
            .map_err(|_| "mémoire insuffisante")?;
        let old_end = self.bytes.len();
        let extra = self.bytes.capacity() - old_end;
        self.bytes.resize(old_end + extra, 0);
        self.bytes
            .copy_within(self.right..old_end, self.right + extra);
        self.right += extra;
        Ok(())
    }
    pub fn replace(&mut self, at: usize, removed: usize, new: &str) {
        assert!(self.boundary(at + removed));
        self.move_to(at);
        self.right += removed;
        self.row += lines(new.as_bytes());
        self.bytes[self.left..self.left + new.len()].copy_from_slice(new.as_bytes());
        self.left += new.len();
    }
    pub fn compact(&mut self) -> &str {
        self.move_to(self.len());
        self.parts(0..self.left)[0]
    }
}
