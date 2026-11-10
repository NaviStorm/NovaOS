//! Tas superviseur. Le moteur ne connaît que sa fenêtre virtuelle et les
//! opérations de pages de NovaOS. Toutes les mutations passent par le verrou global.
mod global;
mod probes;
mod tree;

use crate::{
    hal::memory::{Access, VirtualAddress},
    memory::{self, Memory},
};
use core::{alloc::Layout, mem::size_of};
pub(crate) use global::{install, realign, with_pages};
pub(crate) use probes::{shortage, test};
use tree::{Block, Tree};

const PAGE_BYTES: usize = memory::PAGE as usize;
const QUANTUM: usize = 32;
const CAPACITY: usize = 256 << 20;
const BASE: usize = 0xffff_f001_0000_0000;
const TRIM_THRESHOLD: usize = 64 << 10;
const CANARY: u64 = 0xb8ab_47e9_c351_6f2d;

#[derive(Debug, PartialEq, Eq)]
enum Error {
    NoMemory,
    Reentrant,
    InvalidPointer,
    WrongLayout,
    Metadata,
    Overrun,
    Paging(memory::Error),
}
type Result<T> = core::result::Result<T, Error>;

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(C)]
struct Tag {
    size: usize,
    align: usize,
}

impl From<Layout> for Tag {
    fn from(l: Layout) -> Self {
        Self {
            size: l.size(),
            align: l.align(),
        }
    }
}

fn block_size(layout: Layout) -> Result<usize> {
    if layout.size() == 0 {
        return Err(Error::NoMemory);
    }
    layout
        .size()
        .checked_add(size_of::<Tag>() + size_of::<u64>())
        .and_then(|n| {
            n.max(layout.align())
                .max(QUANTUM)
                .checked_next_power_of_two()
        })
        .filter(|&n| n <= CAPACITY)
        .ok_or(Error::NoMemory)
}

fn meta_bytes(bytes: usize) -> usize {
    // bytes <= CAPACITY. Deux octets par quantum couvrent tous les indices infixes.
    (bytes / QUANTUM * 2).next_multiple_of(PAGE_BYTES)
}

struct Heap {
    base: usize,
    capacity: usize,
    metadata: usize,
    committed: usize,
    live_blocks: usize,
    occupied: usize,
}

impl Heap {
    // Les deux fenêtres et leurs gardes sont réservées à cette instance.
    fn new(base: usize, capacity: usize) -> Option<Self> {
        if base < PAGE_BYTES
            || !capacity.is_power_of_two()
            || !(PAGE_BYTES..=CAPACITY).contains(&capacity)
            || base % capacity != 0
        {
            return None;
        }
        let metadata = base.checked_add(capacity)?.checked_add(PAGE_BYTES)?;
        metadata
            .checked_add(meta_bytes(capacity))?
            .checked_add(PAGE_BYTES)?;
        Some(Self {
            base,
            capacity,
            metadata,
            committed: 0,
            live_blocks: 0,
            occupied: 0,
        })
    }

    fn tree(&mut self) -> Tree<'_> {
        let len = self.committed / QUANTUM * 2;
        // Sûreté : plage privée alignée, initialisée à zéro par allocate_page.
        // committed n’est publié qu’après mappage complet. &mut self exclut tout alias.
        let nodes = unsafe { core::slice::from_raw_parts_mut(self.metadata as *mut u8, len) };
        Tree {
            nodes,
            bytes: self.committed,
        }
    }

    fn unmap(pages: &mut Memory, base: usize, from: usize, to: usize) {
        for offset in (from..to).step_by(PAGE_BYTES).rev() {
            // Sûreté : pages exclusives du tas, mappées par map_range, sans objet
            // vivant, emprunt du tableau de nœuds ni autre accès en cours.
            if let Err(error) = unsafe { pages.release_page(VirtualAddress(base + offset)) } {
                global::fatal(Error::Paging(error));
            }
        }
    }

    fn map_range(pages: &mut Memory, base: usize, from: usize, to: usize) -> Result<()> {
        for offset in (from..to).step_by(PAGE_BYTES) {
            if let Err(e) = pages.allocate_page(VirtualAddress(base + offset), Access::ReadWrite) {
                Self::unmap(pages, base, from, offset);
                return Err(if e == memory::Error::Exhausted {
                    Error::NoMemory
                } else {
                    Error::Paging(e)
                });
            }
        }
        Ok(())
    }

    fn grow(&mut self, pages: &mut Memory, size: usize) -> Result<()> {
        let end = self
            .tree()
            .used_end()
            .checked_next_multiple_of(size)
            .and_then(|start| start.checked_add(size))
            .and_then(|end| end.checked_next_multiple_of(PAGE_BYTES))
            .filter(|&end| end > self.committed && end <= self.capacity)
            .ok_or(Error::NoMemory)?;
        self.extend_to(pages, end)
    }

    // end est alignée sur une page et bornée par capacity. Le nouvel état
    // n’est visible qu’après le succès des deux réservations.
    fn extend_to(&mut self, pages: &mut Memory, end: usize) -> Result<()> {
        let old = self.committed;
        let (meta_old, meta_new) = (meta_bytes(old), meta_bytes(end));
        Self::map_range(pages, self.metadata, meta_old, meta_new)?;
        if let Err(e) = Self::map_range(pages, self.base, old, end) {
            Self::unmap(pages, self.metadata, meta_old, meta_new);
            return Err(e);
        }
        self.committed = end;
        self.tree().extend(old);
        Ok(())
    }

    fn trim(&mut self, pages: &mut Memory, threshold: usize) {
        if self.committed <= threshold {
            return;
        }
        let used = self.tree().used_end();
        let minimum = if threshold == 0 { 0 } else { PAGE_BYTES };
        let keep = used.next_multiple_of(PAGE_BYTES).max(minimum);
        let old = self.committed;
        if old - keep < threshold || old == keep {
            return;
        }
        if used == 0 && keep != 0 {
            self.tree().empty_first_page();
        }
        self.committed = keep;
        Self::unmap(pages, self.base, keep, old);
        Self::unmap(pages, self.metadata, meta_bytes(keep), meta_bytes(old));
    }

    fn trailer(&self, b: Block) -> *mut Tag {
        (self.base + b.start + b.size - size_of::<Tag>()) as *mut Tag
    }

    fn mark(&self, b: Block, layout: Layout) {
        let address = self.base + b.start;
        // Sûreté : le bloc LIVE contient la charge, la sentinelle et le Tag.
        // L’adresse de la sentinelle peut être impaire.
        unsafe {
            self.trailer(b).write(layout.into());
            ((address + layout.size()) as *mut u64).write_unaligned(CANARY ^ address as u64);
        }
    }

    fn inspect(&mut self, address: usize, layout: Layout) -> Result<(Block, Block)> {
        let offset = address
            .checked_sub(self.base)
            .ok_or(Error::InvalidPointer)?;
        let (b, root) = self.tree().locate(offset).ok_or(Error::InvalidPointer)?;
        // Sûreté : l’arbre hors charge a identifié le début exact d’un bloc LIVE.
        // Son Tag est initialisé et aligné. Le layout doit correspondre avant
        // de calculer l’adresse de la sentinelle, même pour une requête erronée.
        unsafe {
            if block_size(layout).ok() != Some(b.size) || self.trailer(b).read() != layout.into() {
                return Err(Error::WrongLayout);
            }
            if ((address + layout.size()) as *const u64).read_unaligned() != CANARY ^ address as u64
            {
                return Err(Error::Overrun);
            }
        }
        Ok((b, root))
    }

    fn allocate(&mut self, pages: &mut Memory, layout: Layout) -> Result<*mut u8> {
        let size = block_size(layout)?;
        let block = match self.tree().take(size) {
            Some(b) => b,
            None => {
                self.grow(pages, size)?;
                self.tree()
                    .take(size)
                    .expect("branche nouvellement disponible")
            }
        };
        self.mark(block, layout);
        self.live_blocks += 1;
        self.occupied += block.size;
        Ok((self.base + block.start) as *mut u8)
    }

    fn free(&mut self, pages: &mut Memory, pointer: *mut u8, layout: Layout) -> Result<()> {
        let (b, root) = self.inspect(pointer.addr(), layout)?;
        self.tree().release(b, root);
        self.live_blocks = self.live_blocks.checked_sub(1).ok_or(Error::Metadata)?;
        self.occupied = self.occupied.checked_sub(b.size).ok_or(Error::Metadata)?;
        self.trim(pages, TRIM_THRESHOLD);
        Ok(())
    }

    /// En cas de déplacement, les deux blocs restent vivants afin de copier
    /// la charge utile hors de la section critique.
    fn prepare_resize(
        &mut self,
        pages: &mut Memory,
        pointer: *mut u8,
        old: Layout,
        new: Layout,
    ) -> Result<*mut u8> {
        let (b, root) = self.inspect(pointer.addr(), old)?;
        let size = block_size(new)?;
        if let Some(block) = self.tree().resize(b, root, size) {
            self.mark(block, new);
            self.occupied = self.occupied.checked_sub(b.size).ok_or(Error::Metadata)? + block.size;
            if size < b.size {
                self.trim(pages, TRIM_THRESHOLD);
            }
            return Ok(pointer);
        }
        // Un Vec en bout de tas peut grandir sans réserver une seconde copie.
        let end = b.start.checked_add(size).ok_or(Error::NoMemory)?;
        if b.start % size == 0
            && end > self.committed
            && end <= self.capacity
            && self.tree().used_end() == b.start + b.size
        {
            match self.extend_to(pages, end.next_multiple_of(PAGE_BYTES)) {
                Ok(()) => {
                    let (_, new_root) = self.tree().locate(b.start).expect("bloc conservé");
                    let block = self
                        .tree()
                        .resize(b, new_root, size)
                        .expect("jumeaux disponibles");
                    self.mark(block, new);
                    self.occupied =
                        self.occupied.checked_sub(b.size).ok_or(Error::Metadata)? + block.size;
                    return Ok(pointer);
                }
                // Un autre bloc déjà engagé peut encore convenir.
                Err(Error::NoMemory) if self.tree().has_space(size) => {}
                Err(e) => return Err(e),
            }
        }
        self.allocate(pages, new)
    }
}

/// Photographie cohérente sous le verrou des pages et du tas. La normalisation
/// rend le suffixe libre du cache lors des comparaisons à charge nulle.
#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub(crate) struct Usage {
    pub blocks: usize,
    pub occupied: usize,
    pub committed: usize,
    pub free_pages: u64,
}

pub(crate) fn audit(normalize: bool) -> Usage {
    global::GLOBAL
        .with_state(|s| {
            let (blocks, occupied) = s.heap.tree().audit()?;
            if (blocks, occupied) != (s.heap.live_blocks, s.heap.occupied) {
                return Err(Error::Metadata);
            }
            if normalize {
                s.heap.trim(&mut s.pages, 0);
            }
            Ok(Usage {
                blocks,
                occupied,
                committed: s.heap.committed,
                free_pages: s.pages.free_pages(),
            })
        })
        .unwrap_or_else(|e| global::fatal(e))
}
