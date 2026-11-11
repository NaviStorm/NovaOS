//! Cache propre par montage. Les listes de seaux et CLOCK référencent des
//! indices dans un tableau borné ; aucune entrée n'est allouée sur un hit.
use crate::{Block, Error, Ref, Result, tree::Family};

const DEFAULT_PAGES: usize = 256;
const MAX_PAGES: usize = 512;
const END: u16 = u16::MAX;

#[derive(Clone, Copy, Debug, Default)]
pub struct CacheStats {
    pub pages: usize,
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
    /// Descripteurs examinés par CLOCK lors des remplacements.
    pub clock_steps: u64,
    /// Adresses physiquement réutilisées ou remplacées par une autre identité.
    pub invalidations: u64,
    pub budget_bytes: usize,
    pub capacity_pages: usize,
    pub payload_bytes: usize,
    pub metadata_bytes: usize,
    pub charged_bytes: usize,
    /// Empreinte fixe du cache et de son verrou dans Engine, présente même à budget zéro.
    pub fixed_bytes: usize,
}

struct Entry {
    reference: Ref,
    data: Box<Block>,
    kind: Option<Family>,
    recent: bool,
    next: u16,
}

impl Drop for Entry {
    fn drop(&mut self) {
        zeroize::Zeroize::zeroize(self.data.as_mut());
    }
}

// Le répertoire réserve des tailles puissances de deux sur les deux ABI.
const _: () = assert!(size_of::<Entry>().is_power_of_two());
const fn directory_bytes(slots: usize) -> usize {
    if slots == 0 {
        return 0;
    }
    let n = slots.next_power_of_two();
    let buckets = n * size_of::<u16>();
    // Plus petite classe d'allocation Darwin : 16 octets.
    n * size_of::<Entry>() + if buckets < 16 { 16 } else { buckets }
}

const fn budget_for(slots: usize) -> usize {
    slots * size_of::<Block>() + directory_bytes(slots)
}

pub struct Cache {
    buckets: Vec<u16>,
    entries: Vec<Entry>,
    hand: usize,
    limit: usize,
    budget: usize,
    counts: CacheStats,
}

impl Default for Cache {
    fn default() -> Self {
        Self {
            buckets: Vec::new(),
            entries: Vec::new(),
            hand: 0,
            limit: DEFAULT_PAGES,
            budget: budget_for(DEFAULT_PAGES),
            counts: CacheStats::default(),
        }
    }
}

impl Cache {
    pub fn set_budget(&mut self, bytes: usize) -> Result<()> {
        if bytes > budget_for(MAX_PAGES) {
            return Err(Error::Limit(
                "budget cache supérieur au profil de 512 pages",
            ));
        }
        if self.budget == bytes {
            return Ok(());
        }
        let slots = (0..=bytes / size_of::<Block>())
            .rev()
            .find(|&n| n <= MAX_PAGES && budget_for(n) <= bytes)
            .unwrap_or(0);
        if self.limit != slots {
            // Libérer l'ancien répertoire avant d'en admettre un autre.
            self.reclaim();
            self.limit = slots;
        }
        self.budget = bytes;
        Ok(())
    }

    fn directory_ready(&mut self) -> bool {
        if !self.buckets.is_empty() {
            return true;
        }
        if self.limit == 0 {
            return false;
        }
        let n = self.limit.next_power_of_two();
        let mut buckets = Vec::new();
        let mut entries = Vec::new();
        // Le cache est facultatif : un refus d'allocation laisse la lecture valide.
        if buckets.try_reserve_exact(n).is_err()
            || entries.try_reserve_exact(n).is_err()
            || buckets.capacity() != n
            || entries.capacity() != n
        {
            return false;
        }
        buckets.resize(n, END);
        self.entries = entries;
        self.buckets = buckets;
        true
    }

    fn bucket(&self, block: u64) -> usize {
        (block as usize) & (self.buckets.len() - 1)
    }

    fn locate(&self, block: u64) -> Option<usize> {
        if self.buckets.is_empty() {
            return None;
        }
        let mut i = self.buckets[self.bucket(block)];
        while i != END {
            let entry = &self.entries[usize::from(i)];
            if entry.reference.block == block {
                return Some(usize::from(i));
            }
            i = entry.next;
        }
        None
    }

    fn link(&mut self, i: usize) {
        let bucket = self.bucket(self.entries[i].reference.block);
        self.entries[i].next = self.buckets[bucket];
        self.buckets[bucket] = i as u16;
    }

    fn unlink(&mut self, i: usize) {
        let bucket = self.bucket(self.entries[i].reference.block);
        let mut at = self.buckets[bucket];
        let mut previous = END;
        while usize::from(at) != i {
            previous = at;
            at = self.entries[usize::from(at)].next;
        }
        let next = self.entries[i].next;
        if previous == END {
            self.buckets[bucket] = next;
        } else {
            self.entries[usize::from(previous)].next = next;
        }
    }

    pub fn get(&mut self, kind: Option<Family>, r: Ref) -> Option<Block> {
        if let Some(i) = self.locate(r.block) {
            let e = &mut self.entries[i];
            if e.kind == kind && e.reference == r {
                self.counts.hits = self.counts.hits.saturating_add(1);
                e.recent = true;
                return Some(*e.data);
            }
        }
        self.counts.misses = self.counts.misses.saturating_add(1);
        None
    }

    pub fn remember(&mut self, kind: Option<Family>, r: Ref, data: Block) {
        if !self.directory_ready() {
            return;
        }
        if let Some(i) = self.locate(r.block) {
            let e = &mut self.entries[i];
            if e.reference != r || e.kind != kind {
                self.counts.invalidations = self.counts.invalidations.saturating_add(1);
                *e.data = data;
                e.reference = r;
                e.kind = kind;
            }
            e.recent = true;
            return;
        }
        if self.entries.len() < self.limit {
            let mut bytes = Vec::new();
            if bytes.try_reserve_exact(data.len()).is_err() || bytes.capacity() != data.len() {
                return;
            }
            bytes.extend_from_slice(&data);
            let data = bytes
                .into_boxed_slice()
                .try_into()
                .expect("bloc complet de 4 Kio");
            self.entries.push(Entry {
                kind,
                reference: r,
                data,
                recent: true,
                next: END,
            });
            self.link(self.entries.len() - 1);
            return;
        }
        // Seconde chance : au plus un tour complet, puis une victime. Son
        // allocation de 4 Kio est réutilisée sans pic d'une page supplémentaire.
        #[cfg(feature = "latency-probes")]
        let _sample = crate::latency_probes::enter(crate::latency_probes::Phase::Eviction);
        loop {
            self.counts.clock_steps = self.counts.clock_steps.saturating_add(1);
            let i = self.hand;
            self.hand = (i + 1) % self.limit;
            if self.entries[i].recent {
                self.entries[i].recent = false;
                continue;
            }
            self.unlink(i);
            let e = &mut self.entries[i];
            *e.data = data;
            e.kind = kind;
            e.reference = r;
            e.recent = true;
            self.link(i);
            self.counts.evictions = self.counts.evictions.saturating_add(1);
            return;
        }
    }

    pub fn invalidate(&mut self, block: u64) {
        let Some(i) = self.locate(block) else { return };
        let last = self.entries.len() - 1;
        self.unlink(i);
        if i != last {
            self.unlink(last);
        }
        self.entries.swap_remove(i);
        if i != last {
            self.link(i);
        }
        if self.hand >= self.entries.len() {
            self.hand = 0;
        }
        self.counts.invalidations = self.counts.invalidations.saturating_add(1);
    }

    pub fn reclaim(&mut self) -> usize {
        let pages = self.entries.len();
        self.entries = Vec::new();
        self.buckets = Vec::new();
        self.hand = 0;
        pages
    }

    pub fn stats(&self) -> CacheStats {
        let pages = self.entries.len();
        let metadata_bytes = directory_bytes(self.entries.capacity());
        CacheStats {
            pages,
            budget_bytes: self.budget,
            capacity_pages: self.limit,
            payload_bytes: pages * size_of::<Block>(),
            metadata_bytes,
            charged_bytes: pages * size_of::<Block>() + metadata_bytes,
            fixed_bytes: size_of::<std::sync::Mutex<Self>>(),
            ..self.counts
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reference(block: u64, birth: u64) -> Ref {
        Ref {
            block,
            birth,
            hash: [birth as u8; 32],
        }
    }

    #[test]
    fn collisions_du_repertoire_et_restitution_du_budget() {
        let mut cache = Cache::default();
        cache.set_budget(budget_for(12)).unwrap();
        for n in 0..12 {
            // Tous ces blocs entrent dans le même seau. Supprimer la tête,
            // le milieu ou la fin doit aussi réindexer le slot déplacé.
            let r = reference(2 + n * 256, 1);
            cache.remember(Some(Family::Objects), r, [n as u8; 4096]);
        }
        for n in [11, 4, 0, 6] {
            let r = reference(2 + n * 256, 1);
            cache.invalidate(r.block);
            assert!(cache.get(Some(Family::Objects), r).is_none());
        }
        for n in [1, 2, 3, 5, 7, 8, 9, 10] {
            let r = reference(2 + n * 256, 1);
            assert!(cache.get(None, r).is_none());
            assert_eq!(cache.get(Some(Family::Objects), r), Some([n as u8; 4096]));
            assert!(
                cache
                    .get(Some(Family::Objects), reference(r.block, 2))
                    .is_none()
            );
            let mut other_hash = r;
            other_hash.hash[0] ^= 1;
            assert!(cache.get(Some(Family::Objects), other_hash).is_none());
        }
        assert_eq!(cache.stats().invalidations, 4);
        for n in 0..200 {
            let r = reference(2 + (n % 15) * 256, n + 2);
            cache.remember(None, r, [n as u8; 4096]);
            assert_eq!(cache.get(None, r), Some([n as u8; 4096]));
            let stats = cache.stats();
            assert!(stats.pages <= stats.capacity_pages);
            assert_eq!(
                stats.payload_bytes + stats.metadata_bytes,
                stats.charged_bytes
            );
            assert!(stats.charged_bytes <= stats.budget_bytes);
        }
        assert!(cache.stats().evictions > 0);
        assert!(cache.set_budget(usize::MAX).is_err());
        assert_eq!(cache.stats().budget_bytes, budget_for(12));
        for n in [0, 1, 7, 65, 255, 256, 512] {
            let budget = budget_for(n);
            cache.set_budget(budget).unwrap();
            for i in 0..n + 1 {
                cache.remember(None, reference(i as u64 + 2, 1), [0; 4096]);
            }
            let stats = cache.stats();
            assert_eq!(
                (stats.pages, stats.capacity_pages, stats.charged_bytes),
                (n, n, budget)
            );
            assert_eq!(cache.reclaim(), n);
            assert_eq!(cache.stats().charged_bytes, 0);
        }
    }
}
