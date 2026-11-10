//! Cache propre d'une instance montée : identité du FS implicite dans son
//! propriétaire, type, adresse, naissance et digest dans chaque clé.
use crate::{Block, Ref, tree::Family};
pub(crate) const CAPACITY: usize = 256;
#[derive(Clone, Copy, Debug, Default)]
pub struct CacheStats {
    pub pages: usize,
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
}
struct Entry {
    kind: Option<Family>,
    reference: Ref,
    data: Box<Block>,
    recent: bool,
}
#[derive(Default)]
pub(crate) struct Cache {
    entries: Vec<Entry>,
    hand: usize,
    counts: CacheStats,
}
impl Cache {
    pub fn get(&mut self, kind: Option<Family>, r: Ref) -> Option<Block> {
        if let Some(e) = self
            .entries
            .iter_mut()
            .find(|e| e.kind == kind && e.reference == r)
        {
            self.counts.hits = self.counts.hits.saturating_add(1);
            e.recent = true;
            return Some(*e.data);
        }
        self.counts.misses = self.counts.misses.saturating_add(1);
        None
    }
    pub fn remember(&mut self, kind: Option<Family>, r: Ref, data: Block) {
        if self
            .entries
            .iter()
            .any(|e| e.kind == kind && e.reference == r)
        {
            return;
        }
        if self.entries.len() < CAPACITY {
            self.entries.push(Entry {
                kind,
                reference: r,
                data: Box::new(data),
                recent: true,
            });
            return;
        }
        // Seconde chance ; au plus deux tours. Toutes les entrées sont propres.
        while self.entries[self.hand].recent {
            self.entries[self.hand].recent = false;
            self.hand = (self.hand + 1) % CAPACITY;
        }
        let e = &mut self.entries[self.hand];
        *e.data = data;
        e.kind = kind;
        e.reference = r;
        e.recent = true;
        self.hand = (self.hand + 1) % CAPACITY;
        self.counts.evictions = self.counts.evictions.saturating_add(1);
    }
    pub fn reclaim(&mut self) -> usize {
        let pages = self.entries.len();
        self.entries = Vec::new();
        self.hand = 0;
        pages
    }
    pub fn stats(&self) -> CacheStats {
        CacheStats {
            pages: self.entries.len(),
            ..self.counts
        }
    }
}
