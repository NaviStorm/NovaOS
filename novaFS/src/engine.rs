//! Publication et vérification des deux générations. Aucun Drop n'effectue d'E/S.
use crate::codec::{Anchor, data_hash};
#[cfg(feature = "latency-probes")]
use crate::latency_probes::{Phase, enter};
use crate::space::Space;
use crate::tree::{self, Family, Node, ReadTree, WriteTree};
use crate::{Block, Device, Error, Ids, MAX_BLOCKS, Manifest, Ref, Result};
use std::cell::Cell;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU8, Ordering};

const HEALTHY: u8 = 0;
const FAULTED: u8 = 1;
const PUBLISHING: u8 = 2;
const MAX_COW: usize = 64;
const RESERVE: u64 = 128;
const SPACE_LEAF: usize = 112;
mod admission;
pub use admission::{PreparedWrite, ReservationCost, VolumeAccounting, VolumePolicy};
pub mod integrity;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    ReadOnly,
    ReadWrite,
}
#[derive(Debug)]
pub struct SpaceStats {
    pub free: u64,
    pub deferred: u64,
    pub largest_free: u64,
    pub intervals: usize,
}

pub trait View: ReadTree {
    fn ids(&self) -> Ids;
    fn manifest(&self) -> &Manifest;
    /// Lecture brute d'un bloc sans contrôle de son checksum. L'appelant qui
    /// dispose d'une référence de données doit utiliser checked_data.
    fn raw(&self, b: u64) -> Result<Block>;
    fn checked_data(&self, r: Ref) -> Result<Block> {
        check_payload(self.ids(), r, self.raw(r.block)?)
    }
}
fn check_payload(ids: Ids, r: Ref, bytes: Block) -> Result<Block> {
    if data_hash(ids, r.block, r.birth, &bytes) != r.hash {
        return Err(Error::Corrupt("somme du bloc de données"));
    }
    Ok(bytes)
}
#[derive(Clone)]
struct Head {
    m: Manifest,
    reference: Ref,
    slot: u64,
}
impl Head {
    fn load<D: Device>(dev: &D, ids: Ids, slot: u64) -> Result<Self> {
        if slot > 1 {
            return Err(Error::Invalid("slot d’ancrage"));
        }
        let a = Anchor::decode(&dev.read(slot)?, slot)?;
        Self::from_anchor(dev, ids, slot, a)
    }
    fn from_anchor<D: Device>(dev: &D, ids: Ids, slot: u64, a: Anchor) -> Result<Self> {
        if a.ids != ids || a.blocks != dev.blocks() || !(2..a.blocks).contains(&a.manifest.block) {
            return Err(Error::Corrupt("identité, taille ou manifeste de l’ancre"));
        }
        let m = Manifest::decode(&dev.read(a.manifest.block)?, ids, a.manifest)?;
        if m.blocks != a.blocks
            || m.previous.checked_add(1) != Some(a.manifest.birth)
            || m.root_oid != 1
            || m.next_oid <= m.root_oid
            || (m.previous == 0 && m.previous_hash != [0; 32])
            || (a.manifest.birth > 1 && slot != a.manifest.birth % 2)
        {
            return Err(Error::Corrupt("génération, racine ou géométrie"));
        }
        Ok(Self {
            m,
            reference: a.manifest,
            slot,
        })
    }
}
struct DiskView<'a, D> {
    dev: &'a D,
    ids: Ids,
    m: &'a Manifest,
    // Adresse certaine d'une erreur de référence, d'E/S ou de checksum.
    failed_block: Cell<Option<u64>>,
}
impl<'a, D> DiskView<'a, D> {
    fn new(dev: &'a D, ids: Ids, m: &'a Manifest) -> Self {
        Self {
            dev,
            ids,
            m,
            failed_block: Cell::new(None),
        }
    }
    fn at<T>(&self, block: u64, result: Result<T>) -> Result<T> {
        result.inspect_err(|_| self.failed_block.set(Some(block)))
    }
}
impl<D: Device> ReadTree for DiskView<'_, D> {
    fn read(&self, r: Ref, f: Family) -> Result<Node> {
        self.at(r.block, r.validate(self.m.blocks, self.m.previous + 1))?;
        let bytes = self.raw(r.block)?;
        self.at(r.block, Node::decode(&bytes, self.ids, r, f))
    }
}
impl<D: Device> View for DiskView<'_, D> {
    fn ids(&self) -> Ids {
        self.ids
    }
    fn manifest(&self) -> &Manifest {
        self.m
    }
    fn raw(&self, b: u64) -> Result<Block> {
        self.at(b, self.dev.read(b))
    }
    fn checked_data(&self, r: Ref) -> Result<Block> {
        self.at(r.block, check_payload(self.ids, r, self.raw(r.block)?))
    }
}

#[derive(Default)]
struct Allocation {
    space: Space,
    nodes: Vec<u64>,
    logical_bytes: u128,
}
struct Marks(Vec<u64>);
impl Marks {
    fn new(blocks: u64) -> Result<Self> {
        let n = blocks.div_ceil(64) as usize;
        let mut words = Vec::new();
        words.try_reserve_exact(n).map_err(|_| Error::NoMemory)?;
        words.resize(n, 0);
        Ok(Self(words))
    }
    fn mark(&mut self, r: Ref, blocks: u64) -> Result<()> {
        if !(2..blocks).contains(&r.block) || self.has(r.block) {
            return Err(Error::Corrupt("bloc hors limites ou multiplement possédé"));
        }
        self.0[r.block as usize / 64] |= 1 << (r.block % 64);
        Ok(())
    }
    fn has(&self, b: u64) -> bool {
        self.0[b as usize / 64] & (1 << (b % 64)) != 0
    }
}
fn geometry<D: Device>(dev: &D, ids: Ids) -> Result<()> {
    if !(2 + RESERVE + 3..=MAX_BLOCKS).contains(&dev.blocks())
        || ids.fs == [0; 16]
        || ids.lv == [0; 16]
    {
        return Err(Error::Invalid("géométrie P0 ou identité nulle"));
    }
    Ok(())
}
fn check_generation<D: Device>(
    v: &DiskView<D>,
    h: &Head,
    prior: Option<(&DiskView<D>, &Marks, &Space)>,
    full: bool,
) -> Result<(Allocation, Marks)> {
    let mut used = Marks::new(v.m.blocks)?;
    let mut alloc = Allocation::default();
    let mut own = |r: Ref| {
        used.mark(r, v.m.blocks)?;
        if let Some((older, old, space)) = prior {
            if !old.has(r.block) {
                let age = space
                    .lookup(r.block)
                    .ok_or(Error::Corrupt("allocation sans provenance"))?
                    .age;
                let oldest = older.m.previous.max(1);
                if r.birth != h.reference.birth || age >= oldest {
                    return Err(Error::Corrupt("réutilisation prématurée"));
                }
            }
        }
        Ok(())
    };
    own(h.reference)?;
    for family in Family::ALL {
        tree::walk(
            v,
            v.m.roots[family.index()],
            family,
            |r| {
                if family == Family::Space {
                    if alloc.nodes.len() >= MAX_COW {
                        return Err(Error::Limit("arbre espace trop grand"));
                    }
                    alloc.nodes.push(r.block);
                }
                own(r)
            },
            |k, val| {
                if family == Family::Space {
                    alloc.space.decoded(k, val, h.m.blocks, h.reference.birth)?
                }
                Ok(())
            },
        )?;
    }
    alloc.logical_bytes = crate::fs::validate(v, prior.map(|(o, _, _)| o), full, &mut own)?;
    let mut runs = alloc.space.0.iter().peekable();
    for b in 2..v.m.blocks {
        if runs.peek().is_some_and(|r| r.end <= b) {
            runs.next();
        }
        let available = runs.peek().filter(|r| r.start <= b);
        if available.is_some() == used.has(b) {
            return Err(Error::Corrupt("partition espace incomplète"));
        }
        if let (Some((older, old, _)), Some(run)) = (prior, available) {
            // Un bloc daté du prédécesseur immédiat doit y avoir été possédé,
            // et tout bloc retiré de cette vue porte exactement cet âge.
            if old.has(b) != (run.age == older.m.previous + 1) {
                return Err(Error::Corrupt("âge du bloc libéré"));
            }
        }
    }
    Ok((alloc, used))
}
fn inspect_pair<D: Device>(dev: &D, ids: Ids, full: bool) -> Result<(Head, Allocation)> {
    geometry(dev, ids)?;
    inspect_heads(
        dev,
        ids,
        Head::load(dev, ids, 0)?,
        Head::load(dev, ids, 1)?,
        full,
    )
}
fn inspect_heads<D: Device>(
    dev: &D,
    ids: Ids,
    mut high: Head,
    mut low: Head,
    full: bool,
) -> Result<(Head, Allocation)> {
    if low.reference.birth > high.reference.birth {
        std::mem::swap(&mut low, &mut high);
    }
    let hv = DiskView::new(dev, ids, &high.m);
    let lv = DiskView::new(dev, ids, &low.m);
    if high.reference.birth == 1 && high.reference == low.reference {
        let (alloc, _) = check_generation(&hv, &high, None, full)?;
        return Ok((high, alloc));
    }
    if low.reference.birth != high.m.previous
        || low.reference.hash != high.m.previous_hash
        || high.m.next_oid < low.m.next_oid
    {
        return Err(Error::Corrupt("filiation des ancrages"));
    }
    let (old_alloc, old_marks) = check_generation(&lv, &low, None, full)?;
    let (alloc, marks) =
        check_generation(&hv, &high, Some((&lv, &old_marks, &old_alloc.space)), full)?;
    let shared = |b| old_marks.has(b) && marks.has(b);
    crate::fs::check_shared_data(&lv, &hv, &shared)?;
    crate::fs::check_shared_data(&hv, &lv, &shared)?;
    Ok((high, alloc))
}

pub struct Engine<D> {
    dev: D,
    ids: Ids,
    head: Head,
    alloc: Allocation,
    mode: Mode,
    health: AtomicU8,
    cache: std::sync::Mutex<crate::cache::Cache>,
    salvage: bool,
    staging_limit: usize,
    policy: VolumePolicy,
}
impl<D> Engine<D> {
    /// Le propriétaire d'une racine P5 peut conserver le cache et l'état monté entre
    /// deux emprunts du même contenu vérifié. La clé (racine, lignée, géométrie)
    /// doit être comparée par l'appelant avant de rattacher un périphérique.
    pub(crate) fn replace_device<E>(self, dev: E, mode: Mode) -> (Engine<E>, D) {
        (
            Engine {
                dev,
                mode,
                ids: self.ids,
                head: self.head,
                alloc: self.alloc,
                health: self.health,
                cache: self.cache,
                salvage: self.salvage,
                staging_limit: self.staging_limit,
                policy: self.policy,
            },
            self.dev,
        )
    }

    pub(crate) fn protected_lba(&self, lba: u64) -> bool {
        self.alloc
            .space
            .lookup(lba)
            .is_none_or(|run| run.age >= self.head.m.previous.max(1))
    }

    /// Les octets privés de l'overlay et les références de données viennent
    /// du même plan FS. Seul le pool peut les admettre après vérification du
    /// graphe, F1 et F2. Un cache indisponible reste un simple défaut de cache.
    pub(crate) fn cache_published_data(&mut self, refs: &[Ref], pages: &BTreeMap<u64, Box<Block>>) {
        let cache = self.cache.get_mut().unwrap_or_else(|p| p.into_inner());
        for &r in refs {
            if let Some(page) = pages.get(&r.block) {
                cache.remember(None, r, **page);
            }
        }
    }

    /// Transition de la paire (G−1,G) à (G,G+1). Les retraits datés G−1
    /// viennent de sortir de la fenêtre récupérable du moteur de contenu.
    pub(crate) fn newly_unprotected(&self) -> impl Iterator<Item = u64> + '_ {
        let age = self.head.m.previous.checked_sub(1).filter(|&n| n > 0);
        self.alloc
            .space
            .0
            .iter()
            .filter(move |run| Some(run.age) == age)
            .flat_map(|run| run.start..run.end)
    }
}

impl<D: Device> ReadTree for Engine<D> {
    fn read(&self, r: Ref, f: Family) -> Result<Node> {
        (|| {
            if let Some(bytes) = self.cached(Some(f), r)? {
                return Node::decode(&bytes, self.ids, r, f);
            }
            #[cfg(feature = "latency-probes")]
            let _sample = enter(Phase::MetadataMiss);
            let bytes = self.dev.read(r.block)?;
            let node = Node::decode(&bytes, self.ids, r, f)?;
            self.remember_checked(Some(f), r, bytes)?;
            Ok(node)
        })()
        .map_err(|e| self.read_failure(e))
    }
}
impl<D: Device> View for Engine<D> {
    fn ids(&self) -> Ids {
        self.ids
    }
    fn manifest(&self) -> &Manifest {
        &self.head.m
    }
    fn raw(&self, b: u64) -> Result<Block> {
        self.check_health()?;
        self.observe(self.dev.read(b))
    }
    fn checked_data(&self, r: Ref) -> Result<Block> {
        (|| {
            if let Some(bytes) = self.cached(None, r)? {
                return Ok(bytes);
            }
            #[cfg(feature = "latency-probes")]
            let _sample = enter(Phase::DataMiss);
            let bytes = check_payload(self.ids, r, self.dev.read(r.block)?)?;
            self.remember_checked(None, r, bytes)?;
            Ok(bytes)
        })()
        .map_err(|e| self.read_failure(e))
    }
}
impl<D: Device> Engine<D> {
    fn cache(&self) -> std::sync::MutexGuard<'_, crate::cache::Cache> {
        self.cache.lock().expect("cache de lecture interrompu")
    }
    fn cached(&self, kind: Option<Family>, r: Ref) -> Result<Option<Block>> {
        #[cfg(feature = "latency-probes")]
        let _sample = enter(Phase::CacheLookup);
        let mut cache = self.cache();
        self.check_health()?;
        r.validate(self.head.m.blocks, self.generation())?;
        Ok(cache.get(kind, r))
    }
    fn remember_checked(&self, kind: Option<Family>, r: Ref, data: Block) -> Result<()> {
        // Une lecture en vol peut finir après la découverte d'une faute par
        // un autre client. Revérifier sous le même verrou que la purge.
        let mut cache = self.cache();
        self.check_health()?;
        cache.remember(kind, r, data);
        Ok(())
    }
    /// Budget du tas pour les pages propres et leur index. Zéro désactive les
    /// admissions. Un changement de capacité rend immédiatement la mémoire.
    pub fn set_cache_budget(&self, bytes: usize) -> Result<()> {
        self.cache().set_budget(bytes)
    }
    pub fn cache_stats(&self) -> crate::CacheStats {
        self.cache().stats()
    }
    pub fn reclaim_cache(&self) -> usize {
        self.cache().reclaim()
    }
    pub fn generation(&self) -> u64 {
        self.head.reference.birth
    }
    pub fn receipt(&self) -> Ref {
        self.head.reference
    }
    pub fn into_device(self) -> D {
        self.dev
    }
    pub(crate) fn check_health(&self) -> Result<()> {
        match self.health.load(Ordering::Acquire) {
            HEALTHY => Ok(()),
            FAULTED => Err(Error::Unavailable),
            _ => Err(Error::UnknownOutcome),
        }
    }
    pub fn unmount(self) -> Result<D> {
        self.check_health()?;
        Ok(self.dev)
    }
    /// Crédit de pages privées en construction. Un refus précède toute E/S de
    /// publication et permet une nouvelle tentative après restitution du crédit.
    pub fn set_staging_budget(&mut self, bytes: usize) {
        self.staging_limit = (bytes / 4096).min(MAX_COW);
    }
    fn read_failure(&self, error: Error) -> Error {
        if !matches!(error, Error::Corrupt(_) | Error::Io(_)) {
            return self.check_health().err().unwrap_or(error);
        }
        let mut cache = self.cache();
        match self.health.load(Ordering::Acquire) {
            HEALTHY => {
                self.health.store(FAULTED, Ordering::Release);
                cache.reclaim();
                error
            }
            // La même erreur peut remonter d'une lecture de nœud jusqu'à
            // l'opération de fichier. Conserver le diagnostic d'origine.
            FAULTED => error,
            _ => Error::UnknownOutcome,
        }
    }
    pub(crate) fn observe<T>(&self, result: Result<T>) -> Result<T> {
        match result {
            Ok(value) => {
                self.check_health()?;
                Ok(value)
            }
            Err(e) => Err(self.read_failure(e)),
        }
    }
    pub fn space_stats(&self) -> SpaceStats {
        let oldest = self.head.m.previous.max(1);
        let mut s = SpaceStats {
            free: 0,
            deferred: 0,
            largest_free: 0,
            intervals: self.alloc.space.0.len(),
        };
        for r in &self.alloc.space.0 {
            let n = r.end - r.start;
            if r.age < oldest {
                s.free += n;
                s.largest_free = s.largest_free.max(n);
            } else {
                s.deferred += n;
            }
        }
        s
    }
    pub fn verify(&self) -> Result<()> {
        self.observe(Ok(()))?;
        self.observe(self.verify_current())
    }
    fn verify_current(&self) -> Result<()> {
        let disk_alloc = if self.salvage {
            let h = Head::load(&self.dev, self.ids, self.head.slot)?;
            if h.reference != self.head.reference {
                return Err(Error::Corrupt("ancrage modifié depuis le sauvetage"));
            }
            let v = DiskView::new(&self.dev, self.ids, &h.m);
            check_generation(&v, &h, None, true)?.0
        } else {
            let (disk, allocation) = inspect_pair(&self.dev, self.ids, true)?;
            if disk.reference != self.head.reference {
                return Err(Error::Corrupt("tête différente du reçu en mémoire"));
            }
            allocation
        };
        if disk_alloc.logical_bytes != self.alloc.logical_bytes {
            return Err(Error::Corrupt(
                "usage logique différent du catalogue en mémoire",
            ));
        }
        Ok(())
    }
    pub fn mount(dev: D, ids: Ids, mode: Mode) -> Result<Self> {
        Self::mount_with_policy(dev, ids, mode, VolumePolicy::default())
    }
    pub fn mount_with_policy(
        mut dev: D,
        ids: Ids,
        mode: Mode,
        policy: VolumePolicy,
    ) -> Result<Self> {
        policy.validate(dev.blocks())?;
        // Une paire dont les métadonnées sont lisibles peut partager des
        // données endommagées. Les contrôler avant de livrer un montage normal.
        let (head, alloc) = inspect_pair(&dev, ids, true)?;
        if mode == Mode::ReadWrite {
            dev.flush()?;
        }
        Ok(Self {
            dev,
            ids,
            mode,
            policy,
            head,
            alloc,
            staging_limit: MAX_COW,
            health: AtomicU8::new(HEALTHY),
            cache: Default::default(),
            salvage: false,
        })
    }
    /// Le choix explicite d'un slot sert à extraire une génération identifiable.
    /// Une génération lisible ne fournit aucune attestation de fraîcheur.
    pub fn salvage(dev: D, ids: Ids, slot: u64) -> Result<Self> {
        geometry(&dev, ids)?;
        let h = Head::load(&dev, ids, slot)?;
        let v = DiskView::new(&dev, ids, &h.m);
        let (alloc, _) = check_generation(&v, &h, None, false)?;
        Ok(Self {
            dev,
            ids,
            mode: Mode::ReadOnly,
            policy: VolumePolicy::default(),
            head: h,
            alloc,
            staging_limit: MAX_COW,
            health: AtomicU8::new(HEALTHY),
            cache: Default::default(),
            salvage: true,
        })
    }
    pub fn format(mut dev: D, ids: Ids) -> Result<Self> {
        geometry(&dev, ids)?;
        for b in 0..dev.blocks() {
            if dev.read(b)?.iter().any(|&v| v != 0) {
                return Err(Error::Invalid("formatage réservé aux images vierges"));
            }
        }
        let blocks = dev.blocks();
        let m = Manifest {
            previous: 0,
            previous_hash: [0; 32],
            root_oid: 1,
            next_oid: 2,
            blocks,
            roots: [Ref::ZERO; 5],
        };
        let mut tx = Tx {
            dev: &dev,
            ids,
            m,
            space: Space::fresh(blocks),
            staged: BTreeMap::new(),
            data_count: 0,
            data_blocks: [0; 17],
            logical_bytes: 0,
            logical_quota_bytes: dev.logical_quota_bytes(),
            staging_limit: MAX_COW,
        };
        crate::fs::initialize(&mut tx)?;
        let plan = tx.finish(&[], None, RESERVE)?;
        let a = Anchor {
            ids,
            blocks,
            manifest: plan.head.reference,
        };
        for (b, data) in &plan.staged {
            dev.write(*b, data)?;
        }
        dev.flush()?;
        for slot in [0, 1] {
            dev.write(slot, &a.encode(slot)?)
                .map_err(|e| Error::Publication {
                    phase: "ancre initiale",
                    cause: Box::new(e),
                })?;
        }
        dev.flush().map_err(|e| Error::Publication {
            phase: "F2 initial",
            cause: Box::new(e),
        })?;
        let (head, alloc) = inspect_pair(&dev, ids, false)?;
        Ok(Self {
            dev,
            ids,
            mode: Mode::ReadWrite,
            policy: VolumePolicy::default(),
            head,
            alloc,
            staging_limit: MAX_COW,
            health: AtomicU8::new(HEALTHY),
            cache: Default::default(),
            salvage: false,
        })
    }
    pub(crate) fn transact<T>(
        &mut self,
        releasing: bool,
        f: impl FnOnce(&mut Tx<D>) -> Result<T>,
    ) -> Result<T> {
        let (plan, result) = self.prepare(releasing, f)?;
        self.publish(plan)?;
        Ok(result)
    }
    pub(crate) fn prepare_write_with<T>(
        &mut self,
        f: impl FnOnce(&mut Tx<D>) -> Result<T>,
    ) -> Result<(PreparedWrite<'_, D>, T)> {
        let (plan, value) = self.prepare(false, f)?;
        Ok((PreparedWrite { engine: self, plan }, value))
    }
    fn prepare<T>(
        &self,
        releasing: bool,
        f: impl FnOnce(&mut Tx<D>) -> Result<T>,
    ) -> Result<(Option<Plan>, T)> {
        if self.mode == Mode::ReadOnly {
            return Err(Error::ReadOnly);
        }
        self.check_health()?;
        let mut m = self.head.m.clone();
        self.generation()
            .checked_add(1)
            .ok_or(Error::Limit("génération épuisée"))?;
        m.previous = self.generation();
        m.previous_hash = self.head.reference.hash;
        let mut space = self.alloc.space.fork()?;
        space.advance(self.head.m.previous.max(1));
        let mut tx = Tx {
            dev: &self.dev,
            ids: self.ids,
            m,
            space,
            staged: BTreeMap::new(),
            data_count: 0,
            data_blocks: [0; 17],
            logical_bytes: self.alloc.logical_bytes,
            logical_quota_bytes: self.logical_quota(),
            staging_limit: self.staging_limit,
        };
        // Seules unlink/collect sont admises sur la réserve. L'épuisement de
        // cette voie peut intervenir dès la première page de données privées.
        let classify = |e| match (releasing, e) {
            (true, Error::NoSpace) => Error::MaintenanceExhausted,
            (_, error) => error,
        };
        let result = self.observe(f(&mut tx)).map_err(classify)?;
        if tx.staged.is_empty()
            && tx.m.roots == self.head.m.roots
            && tx.m.next_oid == self.head.m.next_oid
        {
            return Ok((None, result));
        }
        let floor = if releasing {
            0
        } else {
            self.policy.maintenance_blocks
        };
        let plan = self
            .observe(tx.finish(&self.alloc.nodes, Some(self.head.reference.block), floor))
            .map_err(classify)?;
        Ok((Some(plan), result))
    }
    fn publish(&mut self, plan: Option<Plan>) -> Result<()> {
        let Some(plan) = plan else { return Ok(()) };
        let anchor = Anchor {
            ids: self.ids,
            blocks: self.dev.blocks(),
            manifest: plan.head.reference,
        }
        .encode(plan.head.slot)?;
        // Dès une erreur d'E/S, aucune autre transaction ne peut recycler ces
        // adresses avant un nouveau montage. F1 précède toute écriture d'ancre.
        self.health.store(FAULTED, Ordering::Relaxed);
        let writes = (|| {
            for (b, data) in &plan.staged {
                self.dev.write(*b, data)?;
            }
            self.dev.flush()?;
            self.health.store(PUBLISHING, Ordering::Relaxed);
            self.dev
                .write(plan.head.slot, &anchor)
                .map_err(|e| Error::Publication {
                    phase: "ancre",
                    cause: Box::new(e),
                })?;
            self.dev.flush().map_err(|e| Error::Publication {
                phase: "F2",
                cause: Box::new(e),
            })
        })();
        let cache = self.cache.get_mut().expect("cache de lecture interrompu");
        if let Err(e) = writes {
            cache.reclaim();
            return Err(e);
        }
        for block in plan.staged.keys() {
            cache.invalidate(*block);
        }
        self.head = plan.head;
        self.alloc = plan.alloc;
        self.health.store(HEALTHY, Ordering::Relaxed);
        Ok(())
    }
}

pub struct Tx<'a, D> {
    dev: &'a D,
    ids: Ids,
    m: Manifest,
    space: Space,
    staged: BTreeMap<u64, Box<Block>>,
    data_count: usize,
    data_blocks: [u64; 17],
    logical_bytes: u128,
    logical_quota_bytes: Option<u64>,
    staging_limit: usize,
}
pub(crate) struct Plan {
    head: Head,
    alloc: Allocation,
    staged: BTreeMap<u64, Box<Block>>,
    cost: ReservationCost,
}
impl<D: Device> View for Tx<'_, D> {
    fn ids(&self) -> Ids {
        self.ids
    }
    fn manifest(&self) -> &Manifest {
        &self.m
    }
    fn raw(&self, b: u64) -> Result<Block> {
        if let Some(data) = self.staged.get(&b) {
            Ok(**data)
        } else {
            self.dev.read(b)
        }
    }
}
impl<D: Device> ReadTree for Tx<'_, D> {
    fn read(&self, r: Ref, f: Family) -> Result<Node> {
        if !(2..self.m.blocks).contains(&r.block) || r.birth == 0 || r.birth > self.m.previous + 1 {
            return Err(Error::Corrupt("référence transaction"));
        }
        Node::decode(&self.raw(r.block)?, self.ids, r, f)
    }
}
impl<D: Device> WriteTree for Tx<'_, D> {
    fn store(&mut self, old: Option<Ref>, node: &Node) -> Result<Ref> {
        let b = match old {
            Some(r) if self.staged.contains_key(&r.block) => r.block,
            old => {
                if let Some(r) = old {
                    self.retire_block(r.block)?;
                }
                self.reserve()?
            }
        };
        let (r, data) = node.encode(self.ids, b, self.m.previous + 1)?;
        self.staged.insert(b, Box::new(data));
        Ok(r)
    }
    fn retire(&mut self, old: Ref) -> Result<()> {
        self.retire_block(old.block)
    }
}
impl<D: Device> Tx<'_, D> {
    pub(crate) fn resize_logical(&mut self, previous: u64, next: u64) -> Result<()> {
        let updated = self
            .logical_bytes
            .checked_sub(u128::from(previous))
            .and_then(|n| n.checked_add(u128::from(next)))
            .ok_or(Error::Corrupt("somme des tailles logiques"))?;
        if updated > self.logical_bytes
            && self
                .logical_quota_bytes
                .is_some_and(|q| updated > u128::from(q))
        {
            return Err(Error::QuotaExceeded);
        }
        self.logical_bytes = updated;
        Ok(())
    }
    pub fn manifest_mut(&mut self) -> &mut Manifest {
        &mut self.m
    }
    pub fn put(&mut self, f: Family, k: Vec<u8>, v: Vec<u8>) -> Result<()> {
        let r = tree::put(self, self.m.roots[f.index()], f, &k, &v)?;
        self.m.roots[f.index()] = r;
        Ok(())
    }
    pub fn remove(&mut self, f: Family, k: &[u8]) -> Result<()> {
        let r = tree::remove(self, self.m.roots[f.index()], f, k)?;
        self.m.roots[f.index()] = r;
        Ok(())
    }
    fn reserve(&mut self) -> Result<u64> {
        if self.staged.len() >= MAX_COW {
            return Err(Error::Limit("64 blocs CoW"));
        }
        if self.staged.len() >= self.staging_limit {
            return Err(Error::NoMemory);
        }
        self.space.take()
    }
    pub fn alloc_data(&mut self, data: &Block) -> Result<Ref> {
        // 64 Kio non alignés peuvent toucher 17 blocs physiques.
        if self.data_count == self.data_blocks.len() {
            return Err(Error::Limit("données CoW"));
        }
        let block = self.reserve()?;
        self.data_blocks[self.data_count] = block;
        self.data_count += 1;
        let birth = self.m.previous + 1;
        self.staged.insert(block, Box::new(*data));
        Ok(Ref {
            block,
            birth,
            hash: data_hash(self.ids, block, birth, data),
        })
    }
    pub fn retire_block(&mut self, b: u64) -> Result<()> {
        let age = if self.staged.remove(&b).is_some() {
            if let Some(i) = self.data_blocks[..self.data_count]
                .iter()
                .position(|&x| x == b)
            {
                self.data_count -= 1;
                self.data_blocks[i] = self.data_blocks[self.data_count];
            }
            0
        } else {
            self.m.previous
        };
        self.space.release(b, age, self.m.blocks)
    }
    fn finish(
        mut self,
        old_space: &[u64],
        old_manifest: Option<u64>,
        maintenance_floor: u64,
    ) -> Result<Plan> {
        for &b in old_space {
            self.retire_block(b)?;
        }
        if let Some(b) = old_manifest {
            self.retire_block(b)?;
        }
        let mb = self.reserve()?;
        let mut refs = Vec::new();
        // Réserver les pages décrivant l'allocateur AVANT d'encoder le libre.
        // Le retrait d'une adresse peut réduire le nombre d'intervalles.
        loop {
            let leaves = self.space.0.len().div_ceil(SPACE_LEAF).max(1);
            let needed = if leaves == 1 { 1 } else { leaves + 1 };
            if refs.len() >= needed {
                break;
            }
            if self.staged.len() + needed + 1 > MAX_COW {
                return Err(Error::Limit("plan CoW et espace"));
            }
            if self.staged.len() + needed + 1 > self.staging_limit {
                return Err(Error::NoMemory);
            }
            while refs.len() < needed {
                refs.push(self.space.take()?);
            }
        }
        if self.space.eligible(1) < maintenance_floor {
            return Err(Error::NoSpace);
        }
        let g = self.m.previous + 1;
        let leaves = if refs.len() == 1 { 1 } else { refs.len() - 1 };
        if self.space.0.len() < leaves {
            return Err(Error::NoSpace);
        }
        let mut children = Vec::with_capacity(leaves);
        let mut offset = 0;
        for (i, &b) in refs[..leaves].iter().enumerate() {
            let count = (self.space.0.len() - offset).div_ceil(leaves - i);
            let mut entries = Vec::with_capacity(count);
            for run in &self.space.0[offset..offset + count] {
                let mut val = Vec::with_capacity(17);
                val.extend_from_slice(&(run.end - run.start).to_le_bytes());
                val.push(u8::from(run.age != 0));
                val.extend_from_slice(&run.age.to_le_bytes());
                entries.push((run.start.to_le_bytes().to_vec(), val));
            }
            offset += count;
            let key = entries[0].0.clone();
            let node = Node {
                family: Family::Space,
                level: 0,
                entries,
            };
            let (r, bytes) = node.encode(self.ids, b, g)?;
            self.staged.insert(b, Box::new(bytes));
            children.push((key, r));
        }
        let root = if leaves == 1 {
            children[0].1
        } else {
            let node = Node {
                family: Family::Space,
                level: 1,
                entries: children
                    .into_iter()
                    .map(|(k, r)| (k, r.to_bytes().to_vec()))
                    .collect(),
            };
            let (r, bytes) = node.encode(self.ids, refs[leaves], g)?;
            self.staged.insert(r.block, Box::new(bytes));
            r
        };
        self.m.roots[Family::Space.index()] = root;
        let (reference, data) = self.m.encode(self.ids, mb, g)?;
        self.staged.insert(mb, Box::new(data));
        Ok(Plan {
            cost: ReservationCost {
                data_blocks: self.data_count as u64,
                metadata_blocks: (self.staged.len() - self.data_count) as u64,
            },
            head: Head {
                m: self.m,
                reference,
                slot: g % 2,
            },
            alloc: Allocation {
                space: self.space,
                nodes: refs,
                logical_bytes: self.logical_bytes,
            },
            staged: self.staged,
        })
    }
}
