//! Un groupe est une commande normale ou une session d'insertion. L'éviction porte
//! sur des groupes entiers. Une édition trop grande reste possible sans historique.
use super::gap::Gap;
use alloc::{collections::VecDeque, string::String};
use core::mem::size_of;

const PAYLOAD: usize = 128 * 1024;
const RECORDS: usize = (128 * 1024) / size_of::<Edit>();

struct Edit {
    first: bool,
    offset: usize,
    before: usize,
    after: usize,
    removed: String,
    inserted: String,
}
#[derive(Default)]
pub struct History {
    edits: VecDeque<Edit>,
    applied: usize,
    payload: usize,
    first: bool,
    overflow: bool,
}

impl History {
    pub fn boundary(&mut self) {
        self.first = true;
        self.overflow = false;
    }
    pub fn finish(&mut self, at: usize) {
        if !self.first
            && let Some(last) = self.edits.back_mut()
        {
            last.after = at;
        }
    }
    pub fn truncated(&self) -> bool {
        self.overflow
    }
    pub fn record(
        &mut self,
        gap: &Gap,
        range: core::ops::Range<usize>,
        new: &str,
        before: usize,
        after: usize,
    ) -> Result<(), &'static str> {
        if self.overflow {
            return Ok(());
        }
        if !self.first
            && self.applied == self.edits.len()
            && range.is_empty()
            && let Some(edit) = self.edits.back_mut()
            && edit.removed.is_empty()
            && edit.offset + edit.inserted.len() == range.start
        {
            let needed = edit.inserted.len() + new.len();
            if needed > PAYLOAD {
                *self = Self {
                    overflow: true,
                    ..Self::default()
                };
                return Ok(());
            }
            let old = edit.inserted.capacity();
            if needed > old {
                let target = needed.max(old.saturating_mul(2).min(PAYLOAD));
                edit.inserted
                    .try_reserve_exact(target - edit.inserted.len())
                    .map_err(|_| "mémoire insuffisante")?;
            }
            self.payload += edit.inserted.capacity() - old;
            edit.inserted.push_str(new);
            edit.after = after;
            while self.payload > PAYLOAD {
                self.evict();
            }
            return Ok(());
        }
        if range.len() > PAYLOAD {
            *self = Self {
                overflow: true,
                ..Self::default()
            };
            return Ok(());
        }
        let mut removed = String::new();
        removed
            .try_reserve_exact(range.len())
            .map_err(|_| "mémoire insuffisante")?;
        for part in gap.parts(range.clone()) {
            removed.push_str(part);
        }
        let mut inserted = String::new();
        inserted
            .try_reserve_exact(new.len())
            .map_err(|_| "mémoire insuffisante")?;
        inserted.push_str(new);
        if self.edits.len() == self.edits.capacity()
            && self.applied == self.edits.len()
            && self.edits.len() < RECORDS
        {
            let step = self.edits.len().max(4).min(RECORDS - self.edits.len());
            self.edits
                .try_reserve_exact(step)
                .map_err(|_| "mémoire insuffisante")?;
        }
        while self.edits.len() > self.applied {
            let old = self.edits.pop_back().unwrap();
            self.payload -= old.removed.capacity() + old.inserted.capacity();
        }
        let extra = removed.capacity() + inserted.capacity();
        while self.edits.len() == RECORDS || self.payload + extra > PAYLOAD {
            self.evict();
            if self.overflow {
                return Ok(());
            }
        }
        self.edits.push_back(Edit {
            first: self.first || self.applied == 0,
            offset: range.start,
            before,
            after,
            removed,
            inserted,
        });
        self.payload += extra;
        self.applied += 1;
        self.first = false;
        Ok(())
    }
    fn evict(&mut self) {
        while let Some(old) = self.edits.pop_front() {
            self.payload -= old.removed.capacity() + old.inserted.capacity();
            self.applied -= 1;
            if self.edits.front().is_none_or(|e| e.first) {
                break;
            }
        }
        if self.edits.is_empty() && !self.first {
            self.overflow = true;
        }
    }
    pub fn available(&self, redo: bool) -> bool {
        if redo {
            self.applied < self.edits.len()
        } else {
            self.applied > 0
        }
    }
    /// Les pages lues depuis l'édition ont pu occuper le trou laissé par une
    /// suppression. Réserver le maximum du groupe avant sa première mutation.
    pub fn growth(&self, redo: bool) -> usize {
        let mut i = self.applied;
        let (mut delta, mut peak) = (0isize, 0isize);
        loop {
            let index = if redo {
                i
            } else if let Some(n) = i.checked_sub(1) {
                n
            } else {
                break;
            };
            let Some(edit) = self.edits.get(index) else {
                break;
            };
            let d = edit.inserted.len() as isize - edit.removed.len() as isize;
            delta += if redo { d } else { -d };
            peak = peak.max(delta);
            if redo {
                i += 1;
                if self.edits.get(i).is_none_or(|e| e.first) {
                    break;
                }
            } else {
                i -= 1;
                if edit.first {
                    break;
                }
            }
        }
        peak as usize
    }
    pub fn replay(&mut self, gap: &mut Gap, redo: bool) -> Option<usize> {
        self.boundary();
        loop {
            let index = if redo {
                self.applied
            } else {
                self.applied.checked_sub(1)?
            };
            let edit = self.edits.get(index)?;
            if redo {
                gap.replace(edit.offset, edit.removed.len(), &edit.inserted);
                self.applied += 1;
                if self.edits.get(self.applied).is_none_or(|e| e.first) {
                    return Some(edit.after);
                }
            } else {
                gap.replace(edit.offset, edit.inserted.len(), &edit.removed);
                self.applied -= 1;
                if edit.first {
                    return Some(edit.before);
                }
            }
        }
    }
    #[cfg(test)]
    pub fn bytes(&self) -> usize {
        self.edits.capacity() * size_of::<Edit>() + self.payload
    }
}
