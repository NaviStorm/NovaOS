//! Un seul écrivain par pool. Une vue possède une identité publique distincte
//! du domaine de checksum de son contenu. Aucune adresse physique n'est exportée.
mod coordinator;
mod packing;
use crate::shared_graph::{self as graph, Catalog, Entry, Graph, Kind, PageRef};
use crate::{Block, Compression, Device, Engine, Error, Ids, Mode, Ref, Result};
pub use coordinator::{SharedAdmin, SharedCoordinator, SharedRead, SharedWrite};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::sync::{
    Mutex,
    atomic::{AtomicU8, Ordering},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjectKind {
    Origin,
    Snapshot,
    Clone,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ObjectInfo {
    pub policy: Compression,
    pub id: u64,
    pub owner: [u8; 16],
    pub ids: Ids,
    pub kind: ObjectKind,
    pub blocks: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Receipt {
    pub pool: [u8; 16],
    pub id: u64,
    pub root: [u8; 32],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Usage {
    pub logical_blocks: u64,
    pub exclusive: u64,
    pub shared: u64,
    pub occupied: u64,
    pub deferred: u64,
    /// Capacité facturée à l'admission, y compris les blocs déjà matérialisés.
    pub reserved: u64,
}

/// Charge des unités de données compressées accessibles depuis une racine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CompressionUsage {
    pub mapped_compressed_pages: u64,
    pub compressed_units: u64,
    pub encoded_bytes: u64,
    pub physical_bytes: u64,
    pub saved_bytes: u64,
}

impl From<Kind> for ObjectKind {
    fn from(kind: Kind) -> Self {
        match kind {
            Kind::Origin => Self::Origin,
            Kind::Snapshot => Self::Snapshot,
            Kind::Clone => Self::Clone,
        }
    }
}
impl From<ObjectKind> for Kind {
    fn from(kind: ObjectKind) -> Self {
        match kind {
            ObjectKind::Origin => Self::Origin,
            ObjectKind::Snapshot => Self::Snapshot,
            ObjectKind::Clone => Self::Clone,
        }
    }
}

impl From<&Entry> for ObjectInfo {
    fn from(e: &Entry) -> Self {
        Self {
            id: e.id,
            policy: e.policy,
            kind: e.kind.into(),
            blocks: e.blocks,
            ids: e.ids,
            owner: e.owner,
        }
    }
}

pub fn looks_like_pool(dev: &impl Device) -> Result<bool> {
    graph::looks_like_pool(dev)
}

/// Limite haute de 64 pages du plan P3 + une ancre FS, 16 feuilles et une
/// racine de traduction. Crédit pour la dernière divergence et la suivante.
const TRANSIENT: u64 = 2 * (65 + 16 + 1);
const MAX_ENTRIES: usize = 16;

struct Volume<'a, D> {
    disk: &'a D,
    index: Option<&'a Graph>,
    p6: bool,
    root: PageRef,
    blocks: u64,
}
impl<D: Device> Device for Volume<'_, D> {
    fn blocks(&self) -> u64 {
        self.blocks
    }
    fn logical_quota_bytes(&self) -> Option<u64> {
        Some(self.blocks * 4096)
    }
    fn read(&self, b: u64) -> Result<Block> {
        if b >= self.blocks {
            return Err(Error::Invalid("hors de la vue P5"));
        }
        match self.index {
            Some(index) => index.read_indexed_page(self.disk, self.root, b),
            None => graph::read_page(self.disk, self.root, b, self.p6),
        }
    }
    fn write(&mut self, _: u64, _: &Block) -> Result<()> {
        Err(Error::ReadOnly)
    }
    fn flush(&mut self) -> Result<()> {
        Err(Error::ReadOnly)
    }
}

fn mount_verified_volume<'a, D: Device>(
    dev: &'a D,
    graph: &'a Graph,
    e: &Entry,
    indexed: bool,
) -> Result<Engine<Volume<'a, D>>> {
    let fs = Engine::mount(
        Volume {
            p6: graph.is_p6(),
            disk: dev,
            index: indexed.then_some(graph),
            root: e.root,
            blocks: e.blocks,
        },
        e.ids,
        Mode::ReadOnly,
    )?;
    if graph.is_p6() && fs.accounting()?.logical_bytes > u128::from(e.blocks) * 4096 {
        return Err(Error::Corrupt("quota logique de la vue P6 dépassé"));
    }
    let data_pages =
        if dev.typed_fs_pages() || (graph.is_p6() && graph.compression_usage(e.root)?.0 > 0) {
            Some(fs.file_page_map()?)
        } else {
            None
        };
    for lba in graph::mapped_lbas(dev, e.root, graph.is_p6())? {
        if let Some(map) = &data_pages
            && graph.is_packed(e.root, lba)?
            && !is_data_page(map, lba)
        {
            return Err(Error::Corrupt(
                "page compressée hors des extents de données",
            ));
        }
        if dev.typed_fs_pages() {
            let Some(page) = graph.lookup(e.root, lba)? else {
                return Err(Error::Corrupt("LBA sans référence"));
            };
            dev.check_file_class(
                page.block,
                is_data_page(
                    data_pages
                        .as_ref()
                        .ok_or(Error::Corrupt("classification absente"))?,
                    lba,
                ),
            )?;
        }
        if !fs.protected_lba(lba) {
            return Err(Error::Corrupt("page mappée hors des générations FS"));
        }
    }
    Ok(fs)
}

/// Ces entrées proviennent des catalogues de tête et parent contrôlés. Le
/// dispositif P7 authentifie leur publication avant de les fournir.
pub(crate) fn views_at_head(dev: &impl Device, pool: [u8; 16]) -> Result<Vec<ObjectInfo>> {
    Ok(Graph::views_at_head(dev, pool)?
        .iter()
        .map(ObjectInfo::from)
        .collect())
}

pub(crate) fn read_one<D: Device>(
    dev: &D,
    pool: [u8; 16],
    id: u64,
    path: &str,
    offset: u64,
    len: usize,
) -> Result<Vec<u8>> {
    Graph::inspect_one(dev, pool, id, |graph, e| {
        mount_verified_volume(dev, graph, e, false)?.read(path, offset, len)
    })
}

struct VerifiedView<'a, D> {
    pool: &'a SharedPool<D>,
    volume: Volume<'a, D>,
}
impl<D: Device> Device for VerifiedView<'_, D> {
    fn blocks(&self) -> u64 {
        self.volume.blocks
    }
    fn logical_quota_bytes(&self) -> Option<u64> {
        self.volume.logical_quota_bytes()
    }
    fn read(&self, b: u64) -> Result<Block> {
        self.pool.healthy()?;
        self.pool.observe(self.volume.read(b))
    }
    fn write(&mut self, _: u64, _: &Block) -> Result<()> {
        Err(Error::ReadOnly)
    }
    fn flush(&mut self) -> Result<()> {
        Err(Error::ReadOnly)
    }
}

/// Les barrières du moteur de contenu isolent ses étapes dans une image privée.
/// Le pool synchronise toutes les dépendances de cette image avant publication.
struct Overlay<'a, D> {
    base: Volume<'a, D>,
    dirty: BTreeMap<u64, crate::PrivatePage>,
}
impl<D: Device> Device for Overlay<'_, D> {
    fn blocks(&self) -> u64 {
        self.base.blocks
    }
    fn logical_quota_bytes(&self) -> Option<u64> {
        self.base.logical_quota_bytes()
    }
    fn read(&self, b: u64) -> Result<Block> {
        if let Some(page) = self.dirty.get(&b) {
            return Ok(***page);
        }
        self.base.read(b)
    }
    fn write(&mut self, b: u64, data: &Block) -> Result<()> {
        if b >= self.blocks() {
            return Err(Error::Invalid("écriture hors vue"));
        }
        if self.dirty.len() >= 66 && !self.dirty.contains_key(&b) {
            return Err(Error::Limit("pages privées du moteur de contenu"));
        }
        self.dirty.insert(b, crate::private_page(*data));
        Ok(())
    }
    fn flush(&mut self) -> Result<()> {
        Ok(())
    }
}

struct Source<'a>(&'a dyn Device);
impl Device for Source<'_> {
    fn blocks(&self) -> u64 {
        self.0.blocks()
    }
    fn read(&self, b: u64) -> Result<Block> {
        self.0.read(b)
    }
    fn write(&mut self, _: u64, _: &Block) -> Result<()> {
        Err(Error::ReadOnly)
    }
    fn flush(&mut self) -> Result<()> {
        Err(Error::ReadOnly)
    }
}

fn is_data_page(map: &[u64; 16], lba: u64) -> bool {
    lba < 1024 && map[lba as usize / 64] & (1u64 << (lba % 64)) != 0
}

struct Cached {
    root: PageRef,
    ids: Ids,
    blocks: u64,
    engine: Engine<()>,
    cursor: crate::fs::ReadCursor,
}

struct PreparedContent {
    base: Entry,
    state: Engine<()>,
    dirty: BTreeMap<u64, crate::PrivatePage>,
    data_refs: Vec<Ref>,
}
/// Le descripteur de périphérique reste exclusif. Les lectures empruntent le
/// pool et les publications requièrent un emprunt mutable jusqu'à F2.
pub struct SharedPool<D> {
    dev: D,
    graph: Graph,
    cached: Mutex<Option<Cached>>,
    health: AtomicU8,
}

/// Graphe entièrement contrôlé dans la section exclusive P7 courante.
/// Le prêt du verrou interdit de conserver ce résultat au-delà de la transaction.
pub(crate) struct VerifiedGraph<'guard> {
    graph: Graph,
    _exclusive: &'guard std::sync::MutexGuard<'guard, ()>,
}

impl VerifiedGraph<'_> {
    /// L'appelant fournit la vue inscriptible du même support et du même préfixe.
    pub(crate) fn attach<D: Device>(self, dev: D) -> SharedPool<D> {
        SharedPool::from_graph(dev, self.graph)
    }
}

impl<D: Device> SharedPool<D> {
    pub fn format(mut dev: D, pool: [u8; 16]) -> Result<Self> {
        let graph = Graph::format(&mut dev, pool)?;
        Ok(Self {
            dev,
            graph,
            cached: Mutex::default(),
            health: AtomicU8::new(0),
        })
    }
    pub fn format_p6(mut dev: D, pool: [u8; 16]) -> Result<Self> {
        let graph = Graph::format_p6(&mut dev, pool)?;
        Ok(Self {
            dev,
            graph,
            cached: Mutex::default(),
            health: AtomicU8::new(0),
        })
    }
    pub fn is_p6(&self) -> bool {
        self.graph.is_p6()
    }
    pub fn open(dev: D) -> Result<Self> {
        let graph = Self::verify_graph(&dev, false)?;
        Ok(Self::from_graph(dev, graph))
    }
    pub(crate) fn verify_exclusive<'g>(
        dev: &D,
        exclusive: &'g std::sync::MutexGuard<'g, ()>,
    ) -> Result<VerifiedGraph<'g>> {
        Ok(VerifiedGraph {
            graph: Self::verify_graph(dev, true)?,
            _exclusive: exclusive,
        })
    }
    fn verify_graph(dev: &D, indexed: bool) -> Result<Graph> {
        let graph = Graph::open(dev)?;
        let mut checked = Vec::new();
        for catalog in graph.protected_catalogs() {
            Self::validate_admission(&graph, catalog, dev.blocks(), None)
                .map_err(|_| Error::Corrupt("réservations ou descripteurs P5 incohérents"))?;
            for e in &catalog.entries {
                let key = (e.root, e.ids, e.blocks);
                if checked.contains(&key) {
                    continue;
                }
                mount_verified_volume(dev, &graph, e, indexed)?;
                checked.push(key);
            }
        }
        Ok(graph)
    }
    fn from_graph(dev: D, graph: Graph) -> Self {
        Self {
            dev,
            graph,
            cached: Mutex::default(),
            health: AtomicU8::new(0),
        }
    }
    pub fn into_device(self) -> D {
        self.dev
    }
    pub fn pool_id(&self) -> [u8; 16] {
        self.graph.catalog().pool
    }
    pub fn generation(&self) -> u64 {
        self.graph.catalog().generation
    }
    pub fn list(&self) -> Vec<ObjectInfo> {
        self.graph
            .catalog()
            .entries
            .iter()
            .map(ObjectInfo::from)
            .collect()
    }

    fn healthy(&self) -> Result<()> {
        match self.health.load(Ordering::Acquire) {
            0 if !self.graph.failed() => Ok(()),
            0 => Err(Error::Unavailable),
            1 => Err(Error::Unavailable),
            _ => Err(Error::UnknownOutcome),
        }
    }
    fn observe<T>(&self, result: Result<T>) -> Result<T> {
        let fault = match &result {
            Err(Error::Publication { .. } | Error::UnknownOutcome) => 2,
            Err(Error::Io(_) | Error::Corrupt(_) | Error::Unavailable) => 1,
            _ => 0,
        };
        if fault != 0 {
            self.health.fetch_max(fault, Ordering::AcqRel);
        }
        let value = result?;
        self.healthy()?;
        Ok(value)
    }
    fn entry(&self, id: u64) -> Result<&Entry> {
        self.healthy()?;
        self.graph
            .catalog()
            .entries
            .iter()
            .find(|e| e.id == id)
            .ok_or(Error::NotFound)
    }
    pub fn info(&self, id: u64) -> Result<ObjectInfo> {
        Ok(ObjectInfo::from(self.entry(id)?))
    }
    pub fn receipt(&self, id: u64) -> Result<Receipt> {
        let e = self.entry(id)?;
        let mut hash = Sha256::new();
        if self.is_p6() {
            hash.update(b"NovaFS.P6.receipt.v1");
            hash.update([e.policy as u8]);
        } else {
            hash.update(b"NovaFS.P5.receipt.v1");
        }
        hash.update(self.pool_id());
        hash.update(id.to_le_bytes());
        hash.update(e.owner);
        hash.update(e.ids.fs);
        hash.update(e.ids.lv);
        hash.update(e.blocks.to_le_bytes());
        hash.update([match e.kind {
            Kind::Origin => 1,
            Kind::Snapshot => 2,
            Kind::Clone => 3,
        }]);
        hash.update(e.root.hash);
        Ok(Receipt {
            pool: self.pool_id(),
            id,
            root: hash.finalize().into(),
        })
    }
    pub fn verify_receipt(&self, receipt: &Receipt) -> Result<()> {
        if self.receipt(receipt.id)? != *receipt {
            return Err(Error::Invalid("reçu de branche différent"));
        }
        Ok(())
    }

    fn charge(graph: &Graph, catalog: &Catalog, pending: Option<(PageRef, u64)>) -> Result<u64> {
        let mut cost = graph::FIXED
            + if catalog.entries.is_empty() {
                0
            } else {
                TRANSIENT
            };
        for (i, e) in catalog.entries.iter().enumerate() {
            let blocks = if e.kind == Kind::Snapshot {
                if catalog.entries[..i]
                    .iter()
                    .any(|p| p.kind == Kind::Snapshot && p.root == e.root)
                {
                    continue;
                }
                match pending {
                    Some((root, weight)) if root == e.root => weight,
                    _ => graph.root_pages(e.root)?,
                }
            } else {
                graph::full_cost(e.blocks)
            };
            cost = cost
                .checked_add(blocks)
                .ok_or(Error::Limit("crédit physique"))?;
        }
        Ok(cost)
    }
    fn validate_admission(
        graph: &Graph,
        catalog: &Catalog,
        size: u64,
        pending: Option<(PageRef, u64)>,
    ) -> Result<()> {
        if catalog.entries.len() > MAX_ENTRIES {
            return Err(Error::Limit("catalogue plein"));
        }
        for e in &catalog.entries {
            if !(256..=1024).contains(&e.blocks)
                || e.blocks % 256 != 0
                || e.owner == [0; 16]
                || e.ids.fs == [0; 16]
                || e.ids.lv == [0; 16]
            {
                return Err(Error::Invalid("géométrie ou identité de la vue"));
            }
            let writable = catalog
                .entries
                .iter()
                .filter(|p| p.owner == e.owner && p.kind != Kind::Snapshot)
                .map(|p| p.blocks)
                .sum::<u64>();
            if writable > 1024 {
                return Err(Error::QuotaExceeded);
            }
        }
        if Self::charge(graph, catalog, pending)? > size {
            return Err(Error::PoolFull);
        }
        Ok(())
    }
    /// Blocs physiques occupés (union), dont différés, et crédit physique admis.
    pub fn pool_usage(&self) -> Result<(u64, u64, u64)> {
        self.healthy()?;
        Ok((
            self.graph.occupied(),
            self.graph.deferred(),
            Self::charge(&self.graph, self.graph.catalog(), None)?,
        ))
    }
    pub fn usage(&self, id: u64) -> Result<Usage> {
        let e = self.entry(id)?;
        let (exclusive, shared) = self.graph.usage(id)?;
        let (occupied, deferred, reserved) = self.pool_usage()?;
        Ok(Usage {
            logical_blocks: e.blocks,
            exclusive,
            shared,
            occupied,
            deferred,
            reserved,
        })
    }
    pub fn compression_usage(&self, id: u64) -> Result<CompressionUsage> {
        let entry = self.entry(id)?;
        let (mapped, units, bytes) = self.graph.compression_usage(entry.root)?;
        let saved = mapped
            .checked_sub(units)
            .ok_or(Error::Corrupt("poids des packs"))?;
        Ok(CompressionUsage {
            mapped_compressed_pages: mapped,
            compressed_units: units,
            encoded_bytes: bytes,
            physical_bytes: units * 4096,
            saved_bytes: saved * 4096,
        })
    }

    pub fn set_compression(&mut self, id: u64, policy: Compression) -> Result<()> {
        let entry = self.entry(id)?;
        if entry.kind == Kind::Snapshot {
            return Err(Error::ReadOnly);
        }
        if entry.policy == policy {
            return Ok(());
        }
        if !self.is_p6() {
            return Err(Error::Invalid("la compression nécessite un pool P6"));
        }
        let mut next = self.candidate()?;
        next.entries
            .iter_mut()
            .find(|e| e.id == id)
            .ok_or(Error::NotFound)?
            .policy = policy;
        self.publish(next)
    }

    fn candidate(&self) -> Result<Catalog> {
        self.healthy()?;
        let mut next = self.graph.catalog().clone();
        next.generation = next
            .generation
            .checked_add(1)
            .ok_or(Error::Limit("générations du pool"))?;
        Ok(next)
    }
    fn append(
        &self,
        next: &mut Catalog,
        owner: [u8; 16],
        ids: Ids,
        blocks: u64,
        kind: Kind,
        root: PageRef,
    ) -> Result<u64> {
        if next.entries.len() >= MAX_ENTRIES {
            return Err(Error::Limit("catalogue plein"));
        }
        for e in &next.entries {
            if (e.ids.fs == ids.fs || e.ids.lv == ids.lv)
                && (e.ids != ids || e.blocks != blocks || kind == Kind::Origin)
            {
                return Err(Error::Exists);
            }
        }
        if kind == Kind::Origin
            && self
                .graph
                .protected_catalogs()
                .flat_map(|c| &c.entries)
                .any(|e| e.ids.fs == ids.fs || e.ids.lv == ids.lv)
        {
            return Err(Error::Busy);
        }
        let id = next.next_id;
        next.next_id = id
            .checked_add(1)
            .ok_or(Error::Limit("identifiants de vue épuisés"))?;
        next.entries.push(Entry {
            id,
            owner,
            ids,
            blocks,
            kind,
            root,
            policy: Compression::None,
        });
        Ok(id)
    }
    fn prepare(&mut self) -> Result<()> {
        self.healthy()?;
        let result = self.graph.prepare(&mut self.dev);
        self.observe(result)
    }
    fn finish<T>(&mut self, result: Result<T>) -> Result<T> {
        if result.is_err() {
            self.graph.reset_pending();
        }
        self.observe(result)
    }
    fn publish(&mut self, next: Catalog) -> Result<()> {
        let result = self.graph.commit(&mut self.dev, next);
        self.finish(result)
    }
    pub fn advance(&mut self) -> Result<()> {
        let next = self.candidate()?;
        self.publish(next)
    }
    pub fn snapshot(&mut self, id: u64) -> Result<u64> {
        let source = self.entry(id)?.clone();
        if source.kind == Kind::Snapshot {
            return Err(Error::Invalid("origine inscriptible attendue"));
        }
        self.derive(source, Kind::Snapshot, None)
    }
    pub fn clone_snapshot(&mut self, id: u64, owner: [u8; 16]) -> Result<u64> {
        let source = self.entry(id)?.clone();
        if source.kind != Kind::Snapshot {
            return Err(Error::Invalid("snapshot attendu"));
        }
        self.derive(source, Kind::Clone, Some(owner))
    }
    fn derive(&mut self, e: Entry, kind: Kind, owner: Option<[u8; 16]>) -> Result<u64> {
        let mut next = self.candidate()?;
        let id = self.append(
            &mut next,
            owner.unwrap_or(e.owner),
            e.ids,
            e.blocks,
            kind,
            e.root,
        )?;
        next.entries.last_mut().ok_or(Error::NotFound)?.policy = e.policy;
        Self::validate_admission(&self.graph, &next, self.dev.blocks(), None)?;
        self.publish(next)?;
        Ok(id)
    }
    pub fn remove(&mut self, id: u64) -> Result<()> {
        self.entry(id)?;
        let mut next = self.candidate()?;
        next.entries.retain(|e| e.id != id);
        self.publish(next)
    }

    pub fn read_device(&self, id: u64) -> Result<impl Device + '_> {
        let e = self.entry(id)?;
        Ok(VerifiedView {
            pool: self,
            volume: Volume {
                p6: self.graph.is_p6(),
                disk: &self.dev,
                index: None,
                root: e.root,
                blocks: e.blocks,
            },
        })
    }
    fn mount_cached<E: Device>(&self, e: &Entry, dev: E, mode: Mode) -> Result<Engine<E>> {
        Ok(self.mount_with_cursor(e, dev, mode)?.0)
    }

    fn mount_with_cursor<E: Device>(
        &self,
        e: &Entry,
        dev: E,
        mode: Mode,
    ) -> Result<(Engine<E>, crate::fs::ReadCursor)> {
        let saved = self.cached.lock().map_err(|_| Error::Unavailable)?.take();
        if let Some(c) =
            saved.filter(|c| c.root == e.root && c.ids == e.ids && c.blocks == e.blocks)
        {
            return Ok((c.engine.replace_device(dev, mode).0, c.cursor));
        }
        Ok((Engine::mount(dev, e.ids, mode)?, Default::default()))
    }
    fn remember(&self, e: &Entry, engine: Engine<()>) -> Result<()> {
        self.remember_read(e, engine, Default::default())
    }
    fn remember_read(
        &self,
        e: &Entry,
        engine: Engine<()>,
        cursor: crate::fs::ReadCursor,
    ) -> Result<()> {
        *self.cached.lock().map_err(|_| Error::Unavailable)? = Some(Cached {
            root: e.root,
            ids: e.ids,
            blocks: e.blocks,
            engine,
            cursor,
        });
        Ok(())
    }
    /// Rend au tas le montage de contenu et toutes ses pages propres.
    /// Les références du pool et les plans privés de tickets gardent leur durée de vie.
    pub fn reclaim_cache(&mut self) -> Result<()> {
        self.cached
            .get_mut()
            .map_err(|_| Error::Unavailable)?
            .take();
        Ok(())
    }
    pub fn read_file(&self, id: u64, path: &str, offset: u64, len: usize) -> Result<Vec<u8>> {
        let e = self.entry(id)?;
        let result = (|| {
            let (fs, mut cursor) = self.mount_with_cursor(
                e,
                Volume {
                    p6: self.graph.is_p6(),
                    disk: &self.dev,
                    index: Some(&self.graph),
                    root: e.root,
                    blocks: e.blocks,
                },
                Mode::ReadOnly,
            )?;
            let bytes = fs.read_with_cursor(path, offset, len, Some(&mut cursor))?;
            self.remember_read(e, fs.replace_device((), Mode::ReadOnly).0, cursor)?;
            Ok(bytes)
        })();
        self.observe(result)
    }

    pub fn create(&mut self, owner: [u8; 16], ids: Ids, blocks: u64) -> Result<u64> {
        self.create_with_compression(owner, ids, blocks, Compression::None)
    }
    pub fn create_with_compression(
        &mut self,
        owner: [u8; 16],
        ids: Ids,
        blocks: u64,
        policy: Compression,
    ) -> Result<u64> {
        self.check_policy(policy)?;
        let mut next = self.candidate()?;
        let id = self.append(&mut next, owner, ids, blocks, Kind::Origin, PageRef::EMPTY)?;
        next.entries.last_mut().ok_or(Error::NotFound)?.policy = policy;
        Self::validate_admission(&self.graph, &next, self.dev.blocks(), None)?;
        let overlay = Overlay {
            base: Volume {
                p6: self.graph.is_p6(),
                disk: &self.dev,
                index: None,
                root: PageRef::EMPTY,
                blocks,
            },
            dirty: BTreeMap::new(),
        };
        let fs = Engine::format(overlay, ids)?;
        let (state, overlay) = fs.replace_device((), Mode::ReadWrite);
        self.publish_content(next, id, PageRef::EMPTY, state, overlay.dirty, &[])
    }
    fn check_policy(&self, policy: Compression) -> Result<()> {
        if policy == Compression::Lz4 && !self.is_p6() {
            return Err(Error::Invalid("la compression nécessite un pool P6"));
        }
        Ok(())
    }
    fn publish_content(
        &mut self,
        mut next: Catalog,
        id: u64,
        old: PageRef,
        mut state: Engine<()>,
        dirty: BTreeMap<u64, crate::PrivatePage>,
        data_refs: &[Ref],
    ) -> Result<u64> {
        self.prepare()?;
        let result = (|| {
            let mut refs = BTreeMap::new();
            let policy = next
                .entries
                .iter()
                .find(|e| e.id == id)
                .ok_or(Error::NotFound)?
                .policy;
            let mut group =
                (policy == Compression::Lz4 && data_refs.len() >= 2).then(packing::Group::default);
            for (&lba, page) in &dirty {
                if let Some(pack) = &mut group
                    && data_refs.iter().any(|r| r.block == lba)
                {
                    pack.add(&mut self.graph, &mut self.dev, &mut refs, lba, page)?;
                } else {
                    refs.insert(
                        lba,
                        graph::store_fs_page(
                            &mut self.graph,
                            &mut self.dev,
                            lba,
                            page,
                            data_refs.iter().any(|r| r.block == lba),
                        )?,
                    );
                }
            }
            if let Some(pack) = &mut group {
                pack.publish(&mut self.graph, &mut self.dev, &mut refs)?;
            }
            for lba in state.newly_unprotected() {
                refs.insert(lba, PageRef::EMPTY);
            }
            let root = graph::patch_root(&mut self.graph, &mut self.dev, old, &refs)?;
            let e = next
                .entries
                .iter_mut()
                .find(|e| e.id == id)
                .ok_or(Error::NotFound)?;
            e.root = root;
            let e = e.clone();
            self.publish(next)?;
            state.cache_published_data(data_refs, &dirty);
            self.remember(&e, state)?;
            Ok(id)
        })();
        self.finish(result)
    }
    fn stage_change(
        &self,
        id: u64,
        op: impl FnOnce(&mut Engine<Overlay<'_, D>>) -> Result<Vec<Ref>>,
    ) -> Result<PreparedContent> {
        let e = self.entry(id)?.clone();
        if e.kind == Kind::Snapshot {
            return Err(Error::ReadOnly);
        }
        let overlay = Overlay {
            base: Volume {
                p6: self.graph.is_p6(),
                disk: &self.dev,
                index: Some(&self.graph),
                root: e.root,
                blocks: e.blocks,
            },
            dirty: BTreeMap::new(),
        };
        let fs = self.mount_cached(&e, overlay, Mode::ReadWrite);
        let mut fs = self.observe(fs)?;
        let data_refs = self.observe(op(&mut fs))?;
        let (state, overlay) = fs.replace_device((), Mode::ReadWrite);
        Ok(PreparedContent {
            base: e,
            state,
            dirty: overlay.dirty,
            data_refs,
        })
    }
    fn commit_prepared(&mut self, plan: PreparedContent) -> Result<()> {
        let PreparedContent {
            base: e,
            dirty,
            state,
            data_refs,
        } = plan;
        if self.entry(e.id)? != &e {
            return Err(Error::Invalid("racine de préparation périmée"));
        }
        if dirty.is_empty() {
            return self.remember(&e, state);
        }
        self.publish_content(self.candidate()?, e.id, e.root, state, dirty, &data_refs)
            .map(|_| ())
    }
    fn plan_write(&self, id: u64, path: &str, offset: u64, data: &[u8]) -> Result<PreparedContent> {
        self.stage_change(id, |fs| {
            let (plan, refs) = fs.prepare_write_data(path, offset, data)?;
            plan.commit()?;
            Ok(refs)
        })
    }
    fn change(
        &mut self,
        id: u64,
        op: impl FnOnce(&mut Engine<Overlay<'_, D>>) -> Result<()>,
    ) -> Result<()> {
        let plan = self.stage_change(id, |fs| {
            op(fs)?;
            Ok(Vec::new())
        })?;
        self.commit_prepared(plan)
    }
    pub fn write_file(&mut self, id: u64, path: &str, offset: u64, data: &[u8]) -> Result<()> {
        let plan = self.plan_write(id, path, offset, data)?;
        self.commit_prepared(plan)
    }
    pub fn create_file(&mut self, id: u64, path: &str) -> Result<()> {
        self.change(id, |fs| fs.create(path))
    }
    pub fn mkdir(&mut self, id: u64, path: &str) -> Result<()> {
        self.change(id, |fs| fs.mkdir(path))
    }
    pub fn rename(&mut self, id: u64, from: &str, to: &str) -> Result<()> {
        self.change(id, |fs| fs.rename(from, to))
    }
    pub fn unlink(&mut self, id: u64, path: &str) -> Result<()> {
        self.change(id, |fs| fs.unlink(path))
    }
    pub fn collect(&mut self, id: u64) -> Result<bool> {
        let mut collected = false;
        self.change(id, |fs| {
            collected = fs.collect()?;
            Ok(())
        })?;
        Ok(collected)
    }

    fn stage_import(
        &mut self,
        next: &mut Catalog,
        owner: [u8; 16],
        ids: Ids,
        source: &dyn Device,
        kind: Kind,
        policy: Compression,
    ) -> Result<u64> {
        self.check_policy(policy)?;
        let blocks = source.blocks();
        if !(256..=1024).contains(&blocks) || blocks % 256 != 0 {
            return Err(Error::Invalid("taille de la vue importée"));
        }
        let id = self.append(next, owner, ids, blocks, kind, PageRef::EMPTY)?;
        next.entries.last_mut().ok_or(Error::NotFound)?.policy = policy;
        Self::validate_admission(&self.graph, next, self.dev.blocks(), None)?;
        let fs = Engine::mount(Source(source), ids, Mode::ReadOnly)?;
        if self.is_p6() && fs.accounting()?.logical_bytes > u128::from(blocks) * 4096 {
            return Err(Error::QuotaExceeded);
        }
        let file_map = if policy == Compression::Lz4 || self.dev.typed_fs_pages() {
            Some(fs.file_page_map()?)
        } else {
            None
        };
        let mut compression = if policy == Compression::Lz4 {
            Some((
                file_map.ok_or(Error::Invalid("carte FS absente"))?,
                packing::Group::default(),
            ))
        } else {
            None
        };
        self.prepare()?;
        let mut refs = BTreeMap::new();
        for lba in 0..blocks {
            if fs.protected_lba(lba) {
                let page = source.read(lba)?;
                if let Some((map, group)) = &mut compression
                    && is_data_page(map, lba)
                {
                    group.add(&mut self.graph, &mut self.dev, &mut refs, lba, &page)?;
                } else if page != [0; 4096] {
                    refs.insert(
                        lba,
                        graph::store_fs_page(
                            &mut self.graph,
                            &mut self.dev,
                            lba,
                            &page,
                            file_map.as_ref().is_some_and(|m| is_data_page(m, lba)),
                        )?,
                    );
                }
            }
        }
        if let Some((_, group)) = &mut compression {
            group.publish(&mut self.graph, &mut self.dev, &mut refs)?;
        }
        refs.retain(|_, r| *r != PageRef::EMPTY);
        let root = graph::patch_root(&mut self.graph, &mut self.dev, PageRef::EMPTY, &refs)?;
        // Relire le contenu importé avant de l'engager, même si le média source
        // a fourni deux lectures différentes pendant la copie.
        let copied = Engine::mount(
            Volume {
                p6: self.graph.is_p6(),
                disk: &self.dev,
                index: None,
                root,
                blocks,
            },
            ids,
            Mode::ReadOnly,
        )?;
        if copied.receipt() != fs.receipt() {
            return Err(Error::Corrupt(
                "source de migration modifiée pendant la copie",
            ));
        }
        if let Some((expected, _)) = &compression
            && copied.file_page_map()? != *expected
        {
            return Err(Error::Corrupt(
                "classification de données modifiée pendant la copie",
            ));
        }
        next.entries.last_mut().ok_or(Error::NotFound)?.root = root;
        let mut leaves = 0u16;
        for lba in refs.keys() {
            leaves |= 1 << (lba / 64);
        }
        let pages = if compression.is_some() {
            refs.values()
                .map(|r| r.block)
                .collect::<std::collections::BTreeSet<_>>()
                .len()
        } else {
            refs.len()
        };
        let weight = 1 + u64::from(leaves.count_ones()) + pages as u64;
        Self::validate_admission(&self.graph, next, self.dev.blocks(), Some((root, weight)))?;
        Ok(id)
    }
    pub fn import_image(
        &mut self,
        owner: [u8; 16],
        ids: Ids,
        source: &impl Device,
        kind: ObjectKind,
    ) -> Result<u64> {
        self.import_with_compression(owner, ids, source, kind, Compression::None)
    }
    pub fn import_with_compression(
        &mut self,
        owner: [u8; 16],
        ids: Ids,
        source: &impl Device,
        kind: ObjectKind,
        policy: Compression,
    ) -> Result<u64> {
        let mut next = self.candidate()?;
        let result = self.stage_import(&mut next, owner, ids, source, kind.into(), policy);
        let id = self.finish(result)?;
        self.publish(next)?;
        Ok(id)
    }
    pub fn migrate_legacy(&mut self, source: &impl Device) -> Result<Vec<u64>> {
        let mut next = self.candidate()?;
        if !next.entries.is_empty() || next.next_id != 1 {
            return Err(Error::Invalid("destination de migration neuve requise"));
        }
        let mut ids = Vec::new();
        let result = crate::pool::visit_ready(source, |owner, lineage, device| {
            ids.push(self.stage_import(
                &mut next,
                owner,
                lineage,
                device,
                Kind::Origin,
                Compression::None,
            )?);
            Ok(())
        });
        self.finish(result)?;
        self.publish(next)?;
        Ok(ids)
    }
}
