//! Allocation sans tas : parcours des plages utilisables puis recyclage dans les cadres libres.
use super::{Error, PAGE, Result};
use crate::boot::MemoryInfo;
use limine::memory_map::{Entry, EntryType};

pub const PHYS_MASK: u64 = 0x0000_ffff_ffff_f000;

pub fn mapped(kind: EntryType) -> bool {
    matches!(
        kind,
        EntryType::USABLE
            | EntryType::BOOTLOADER_RECLAIMABLE
            | EntryType::EXECUTABLE_AND_MODULES
            | EntryType::FRAMEBUFFER
    )
}

pub struct Frames {
    pub info: MemoryInfo,
    region: usize,
    next: u64,
    end: u64,
    recycled: u64,
    pub available: u64,
}

impl Frames {
    pub fn new(info: MemoryInfo) -> Result<Self> {
        let mut available = 0;
        let mut previous_end = 0;
        for entry in info
            .entries
            .iter()
            .copied()
            .filter(|e| mapped(e.entry_type))
        {
            let end = entry
                .base
                .checked_add(entry.length)
                .ok_or(Error::Geometry)?;
            if end == 0 || end > PHYS_MASK + PAGE || info.offset.checked_add(end - 1).is_none() {
                return Err(Error::Geometry);
            }
            if entry.entry_type == EntryType::USABLE {
                if (entry.base | end) % PAGE != 0 || entry.base < previous_end {
                    return Err(Error::Geometry);
                }
                available += end.saturating_sub(entry.base.max(PAGE)) / PAGE;
                previous_end = end;
            }
        }
        if info.offset % PAGE != 0 {
            return Err(Error::Geometry);
        }
        Ok(Self {
            info,
            region: 0,
            next: 0,
            end: 0,
            recycled: 0,
            available,
        })
    }

    pub fn covers(&self, frame: u64, kind: EntryType) -> bool {
        self.info.entries.iter().any(|e| {
            e.entry_type == kind
                && frame >= e.base
                && frame
                    .checked_add(PAGE)
                    .is_some_and(|end| end <= e.base + e.length)
        })
    }

    pub fn direct(&self, frame: u64) -> *mut u64 {
        (self.info.offset + frame) as *mut u64
    }

    pub fn take(&mut self) -> Result<u64> {
        let frame = if self.recycled != 0 {
            let head = self.recycled;
            // Sûreté : head est un cadre libéré par ce gestionnaire et contient
            // le lien de liste écrit avant sa publication. Accès via le HHDM.
            self.recycled = unsafe { self.direct(head).read() };
            head
        } else {
            while self.next == self.end {
                let e: &Entry = self.info.entries.get(self.region).ok_or(Error::Exhausted)?;
                self.region += 1;
                if e.entry_type == EntryType::USABLE {
                    self.end = e.base + e.length;
                    self.next = e.base.max(PAGE).min(self.end);
                }
            }
            let head = self.next;
            self.next += PAGE;
            head
        };
        // Sûreté : ce cadre utilisable de 4 Kio est désormais exclusif au BSP.
        unsafe {
            self.direct(frame)
                .cast::<u8>()
                .write_bytes(0, PAGE as usize)
        };
        self.available -= 1;
        Ok(frame)
    }

    /// L’appelant interne rend une seule fois un cadre de take(), sans traduction active.
    pub(crate) fn release(&mut self, frame: u64) {
        // Sûreté : le cadre exclusivement restitué reste accessible en RW via le HHDM.
        unsafe { self.direct(frame).write(self.recycled) };
        self.recycled = frame;
        self.available += 1;
    }
}
