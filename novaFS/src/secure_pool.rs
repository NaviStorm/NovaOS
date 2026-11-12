//! Pool P7. Le préfixe de contrôle du graphe est privé à la transaction.
//! Le registre CoW P6 demeure l'unique allocateur des paires (en-tête, contenu).
use crate::crypto::{Credential, Entropy, HostEntropy, SLOTS_BYTES, Secret, Slots};
use crate::io::{Read, Write};
use crate::secure_commit::{CommitLog, Digest, Head};
use crate::secure_unit::{self as unit, Context, Keys, Kind};
use crate::shared::{ObjectInfo, SharedPool, VerifiedGraph};
use crate::{Block, Compression, Device, Error, Ids, ObjectRole, Result};
use alloc::collections::BTreeMap;
use alloc::{boxed::Box, vec::Vec};
use sha2::{Digest as _, Sha256};
mod inspect;
pub mod migration;
pub mod scrub;
pub use inspect::{StructuralReport, structural_check, verify_commit_replicas};

type Id = [u8; 16];
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ViewParent {
    Origin,
    HistoricalUnknown,
    Exact(u64),
    External(Id, u64),
}

const MAGIC: &[u8; 8] = b"NP7POOLH";
const P9_MAGIC: &[u8; 8] = b"NP9POOLH";
const CONTROL: usize = 7;
const BANK_BLOCKS: u64 = 9;
const FIRST_BANK: u64 = 10;
const FIRST_UNIT: u64 = FIRST_BANK + 4 * BANK_BLOCKS;
const DOMAIN_SIZE: usize = 232;
const LIMIT: usize = 17;

fn corrupt() -> Error {
    Error::Corrupt("format ou liaison du pool P7")
}
fn coord(blocks: u64) -> Result<u64> {
    if blocks < FIRST_UNIT + 2 || blocks > crate::MAX_BLOCKS {
        return Err(Error::Invalid("géométrie du pool P7"));
    }
    Ok(CONTROL as u64 + (blocks - FIRST_UNIT) / 2)
}

#[derive(Clone)]
struct Domain {
    id: Id,
    ids: Ids,
    slots: Option<Slots>,
}

#[derive(Clone)]
struct State {
    p9: bool,
    // Une entrée absente sur un dérivé importé désigne une origine historique inconnue.
    parents: BTreeMap<u64, (Id, u64)>,
    imports: BTreeMap<Id, (Id, Id)>,
    prefix: Vec<Block>,
    domains: Vec<Domain>,
    proofs: Box<Block>,
}
impl State {
    fn empty() -> Self {
        Self {
            p9: false,
            parents: BTreeMap::new(),
            imports: BTreeMap::new(),
            prefix: vec![[0; 4096]; CONTROL],
            domains: Vec::new(),
            proofs: Box::new([0; 4096]),
        }
    }
    fn directory(&self, pool: Id, g: u64) -> Result<Block> {
        if self.domains.len() > LIMIT {
            return Err(Error::Limit("domaines P7"));
        }
        let mut p = [0; 4096];
        p[..8].copy_from_slice(if self.p9 { b"NP9DIR01" } else { b"NP7DIR01" });
        p[8..24].copy_from_slice(&pool);
        p[24..32].copy_from_slice(&g.to_le_bytes());
        p[32] = self.domains.len() as u8;
        for (b, d) in p[64..].chunks_exact_mut(DOMAIN_SIZE).zip(&self.domains) {
            b[..16].copy_from_slice(&d.id);
            b[16..32].copy_from_slice(&d.ids.fs);
            b[32..48].copy_from_slice(&d.ids.lv);
            if let Some(slots) = &d.slots {
                b[48] = 1;
                b[56..].copy_from_slice(slots.as_bytes());
            }
        }
        Ok(p)
    }
    fn save(&self, d: &mut impl Device, pool: Id, g: u64) -> Result<Digest> {
        let base = FIRST_BANK + g % 4 * BANK_BLOCKS;
        let mut hash = Sha256::new();
        for (i, page) in core::iter::once(&self.directory(pool, g)?)
            .chain(&self.prefix)
            .chain(core::iter::once(self.proofs.as_ref()))
            .enumerate()
        {
            d.write(base + i as u64, page)?;
            hash.update(page);
        }
        Ok(hash.finalize().into())
    }
    fn load(d: &impl Device, pool: Id, h: &Head) -> Result<Self> {
        let base = FIRST_BANK + h.generation % 4 * BANK_BLOCKS;
        let p = d.read(base)?;
        let mut hash = Sha256::new();
        hash.update(p);
        let mut state = Self::empty();
        state.p9 = &p[..8] == b"NP9DIR01";
        if state.p9 != (&read_header(d)?[..8] == P9_MAGIC) {
            return Err(corrupt());
        }
        for (i, page) in state.prefix.iter_mut().enumerate() {
            *page = d.read(base + 1 + i as u64)?;
            hash.update(*page);
        }
        *state.proofs = d.read(base + 8)?;
        hash.update(state.proofs.as_ref());
        if state.p9 {
            let proof = &state.proofs[2048..];
            if &proof[..8] != b"NP9PAR01" || proof[8] > 32 {
                return Err(corrupt());
            }
            for pair in proof[16..].chunks_exact(32).take(proof[8] as usize) {
                let child = u64::from_le_bytes(pair[..8].try_into().unwrap());
                let parent = u64::from_le_bytes(pair[8..16].try_into().unwrap());
                let origin: Id = pair[16..32].try_into().unwrap();
                if parent == 0
                    || (origin == [0; 16] && child <= parent)
                    || state.parents.insert(child, (origin, parent)).is_some()
                {
                    return Err(corrupt());
                }
            }
            let n = usize::from(proof[1152]);
            if n > LIMIT {
                return Err(corrupt());
            }
            for row in proof[1168..].chunks_exact(48).take(n) {
                let dest: Id = row[..16].try_into().unwrap();
                let src: Id = row[16..32].try_into().unwrap();
                let old: Id = row[32..48].try_into().unwrap();
                if [dest, src, old].contains(&[0; 16])
                    || state.imports.values().any(|v| *v == (src, old))
                    || state.imports.insert(dest, (src, old)).is_some()
                {
                    return Err(corrupt());
                }
            }
            let mut canonical = [0; 2048];
            state.write_parents(&mut canonical)?;
            if canonical != proof {
                return Err(corrupt());
            }
        }
        if <[u8; 32]>::from(hash.finalize()) != h.digest || usize::from(p[32]) > LIMIT {
            return Err(corrupt());
        }
        for b in p[64..].chunks_exact(DOMAIN_SIZE).take(usize::from(p[32])) {
            let d = Domain {
                id: b[..16].try_into().unwrap(),
                ids: Ids {
                    fs: b[16..32].try_into().unwrap(),
                    lv: b[32..48].try_into().unwrap(),
                },
                slots: match b[48] {
                    0 => None,
                    1 => Some(Slots::decode(&b[56..56 + SLOTS_BYTES])?),
                    _ => return Err(corrupt()),
                },
            };
            if d.id == [0; 16]
                || d.ids.lv == [0; 16]
                || d.ids.fs == [0; 16]
                || state
                    .domains
                    .iter()
                    .any(|a| a.id == d.id || a.ids.lv == d.ids.lv || a.ids.fs == d.ids.fs)
            {
                return Err(corrupt());
            }
            state.domains.push(d);
        }
        if state.imports.keys().any(|id| {
            !state
                .domains
                .iter()
                .any(|d| d.id == *id && d.slots.is_some())
        }) || state.directory(pool, h.generation)? != p
        {
            return Err(corrupt());
        }
        let catalogs = crate::shared_graph::Graph::visible_catalogs(
            &Prefix {
                state: &state,
                size: coord(d.blocks())?,
            },
            pool,
        )?;
        if catalogs.first().map(|c| c.generation) != Some(h.generation)
            || state.domains.iter().any(|domain| {
                !catalogs
                    .iter()
                    .flat_map(|c| &c.entries)
                    .any(|e| e.ids == domain.ids)
            })
        {
            return Err(corrupt());
        }
        for entry in catalogs.iter().flat_map(|c| &c.entries) {
            state.domain(entry.ids)?;
        }
        Ok(state)
    }
    fn write_parents(&self, out: &mut [u8]) -> Result<()> {
        if self.imports.len() > LIMIT {
            return Err(corrupt());
        }
        out[1152] = self.imports.len() as u8;
        for (row, (dest, (pool, domain))) in out[1168..].chunks_exact_mut(48).zip(&self.imports) {
            row[..16].copy_from_slice(dest);
            row[16..32].copy_from_slice(pool);
            row[32..48].copy_from_slice(domain);
        }
        if self.parents.len() > 32 {
            return Err(corrupt());
        }
        out[..8].copy_from_slice(b"NP9PAR01");
        out[8] = self.parents.len() as u8;
        for (pair, (child, (origin, parent))) in out[16..].chunks_exact_mut(32).zip(&self.parents) {
            pair[..8].copy_from_slice(&child.to_le_bytes());
            pair[8..16].copy_from_slice(&parent.to_le_bytes());
            pair[16..32].copy_from_slice(origin);
        }
        Ok(())
    }
    /// Chaque génération comporte au plus seize vues. Le HMAC lie la racine
    /// du domaine à sa vue publique et à l'ensemble des emplacements autorisés.
    fn signed_views(
        &self,
        size: u64,
        pool: Id,
        keys: &BTreeMap<Id, Keys>,
        only: Option<Id>,
        verify: bool,
    ) -> Result<Box<Block>> {
        let source = Prefix { state: self, size };
        let catalogs = crate::shared_graph::Graph::visible_catalogs(&source, pool)?;
        let mut page = Box::new([0; 4096]);
        if self.p9 {
            self.write_parents(&mut page[2048..])?;
            for (&child, &(origin, parent)) in &self.parents {
                let find = |id| {
                    catalogs
                        .iter()
                        .flat_map(|cat| &cat.entries)
                        .find(|e| e.id == id)
                };
                let e = find(child).ok_or_else(corrupt)?;
                if parent == 0
                    || e.kind == crate::shared_graph::Kind::Origin
                    || (origin == [0; 16]
                        && (parent >= child
                            || find(parent).is_some_and(|p| {
                                p.ids != e.ids
                                    || (e.kind == crate::shared_graph::Kind::Clone
                                        && p.kind != crate::shared_graph::Kind::Snapshot)
                            })))
                {
                    return Err(corrupt());
                }
            }
        }
        let mut pos = 0;
        for catalog in catalogs {
            for entry in catalog.entries {
                let d = self.domain(entry.ids)?;
                let end = pos + 32;
                let tag: &Digest = self.proofs[pos..end].try_into().unwrap();
                if let Some(slots) = &d.slots {
                    if only.is_none_or(|id| id == d.id) {
                        let key = &keys.get(&d.id).ok_or(Error::ReadOnly)?.view;
                        let parts: &[&[u8]] = &[
                            b"NovaFS/P7/vue/v1",
                            &pool,
                            &d.id,
                            &entry.ids.fs,
                            &entry.ids.lv,
                            &catalog.generation.to_le_bytes(),
                            &entry.id.to_le_bytes(),
                            &entry.owner,
                            &[entry.kind as u8, entry.policy as u8],
                            &entry.blocks.to_le_bytes(),
                            &entry.root.block.to_le_bytes(),
                            &entry.root.hash,
                            slots.as_bytes(),
                        ];
                        let (parent_pool, parent) =
                            self.parents.get(&entry.id).copied().unwrap_or(([0; 16], 0));
                        let edge = parent.to_le_bytes();
                        let computed = if self.p9 {
                            let mut parts = parts.to_vec();
                            let (archive_pool, archive_domain) = self
                                .imports
                                .get(&d.id)
                                .copied()
                                .unwrap_or(([0; 16], [0; 16]));
                            parts.extend([
                                b"NovaFS/P9/filiation/v1".as_slice(),
                                &parent_pool,
                                &edge,
                                &archive_pool,
                                &archive_domain,
                            ]);
                            if verify {
                                key.verify_mac(&parts, tag)?;
                            }
                            key.mac(&parts)
                        } else {
                            if verify {
                                key.verify_mac(parts, tag)?;
                            }
                            key.mac(parts)
                        };
                        page[pos..end].copy_from_slice(&computed);
                    } else {
                        page[pos..end].copy_from_slice(tag)
                    }
                }
                pos = end;
            }
        }
        if verify && page != self.proofs {
            return Err(corrupt());
        }
        Ok(page)
    }
    fn domain(&self, ids: Ids) -> Result<&Domain> {
        self.domains
            .iter()
            .find(|x| x.ids == ids)
            .ok_or_else(corrupt)
    }
}

struct ReadOnly<'a, D>(&'a D);
impl<D: Device> Device for ReadOnly<'_, D> {
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
struct Prefix<'a> {
    state: &'a State,
    size: u64,
}
impl Device for Prefix<'_> {
    fn imported_genesis(&self) -> bool {
        self.state.p9
    }
    fn blocks(&self) -> u64 {
        self.size
    }
    fn read(&self, b: u64) -> Result<Block> {
        self.state
            .prefix
            .get(usize::try_from(b).map_err(|_| corrupt())?)
            .copied()
            .ok_or_else(corrupt)
    }
    fn write(&mut self, _: u64, _: &Block) -> Result<()> {
        Err(Error::ReadOnly)
    }
    fn flush(&mut self) -> Result<()> {
        Err(Error::ReadOnly)
    }
}

fn header(pool: Id, blocks: u64) -> Block {
    let mut b = [0; 4096];
    b[..8].copy_from_slice(MAGIC);
    b[8..10].copy_from_slice(&7u16.to_le_bytes());
    b[10..12].copy_from_slice(&1u16.to_le_bytes());
    b[16..32].copy_from_slice(&pool);
    b[32..40].copy_from_slice(&blocks.to_le_bytes());
    b
}
pub fn looks_like(d: &impl Device) -> Result<bool> {
    if d.blocks() < 2 {
        return Ok(false);
    }
    for index in 0..2 {
        let b = d.read(index)?;
        if &b[..8] == MAGIC || &b[..8] == P9_MAGIC {
            return Ok(true);
        }
    }
    Ok(false)
}
/// Identité et géométrie publiques. L'appelant conserve séparément son identité attendue.
pub fn inspect_header(d: &impl Device) -> Result<Id> {
    Ok(read_header(d)?[16..32].try_into().unwrap())
}

fn read_header(d: &impl Device) -> Result<Block> {
    coord(d.blocks())?;
    let p = d.read(0)?;
    let pool = p[16..32].try_into().unwrap();
    let mut normalized = p;
    normalized[40..112].fill(0);
    let mut expected = header(pool, d.blocks());
    if &p[..8] == P9_MAGIC {
        expected[..8].copy_from_slice(P9_MAGIC);
        expected[8..10].copy_from_slice(&9u16.to_le_bytes());
    }
    if &p[..8] == P9_MAGIC {
        if p[12] > 1 || (p[12] == 1) != migration::receipt_field(&p)?.is_some() {
            return Err(corrupt());
        }
        expected[12] = p[12];
        normalized[112..].fill(0);
    }
    if pool == [0; 16] || normalized != expected || p != d.read(1)? {
        return Err(corrupt());
    }
    Ok(p)
}

fn new_header(
    pool: Id,
    blocks: u64,
    administration: &Secret,
    entropy: &mut impl Entropy,
    p9: bool,
    migrated: bool,
) -> Result<(Block, Secret)> {
    let master = Secret::generate(entropy)?;
    let admin = master.derive(&pool, b"pool-publication-v1")?;
    let wrap = administration.derive(&pool, b"pool-enveloppe-v1")?;
    let mut h = header(pool, blocks);
    if p9 {
        h[..8].copy_from_slice(P9_MAGIC);
        h[8..10].copy_from_slice(&9u16.to_le_bytes());
    }
    h[12] = u8::from(migrated);
    let mut secret = zeroize::Zeroizing::new(*master.as_bytes());
    let (nonce, tag) = wrap.seal(entropy, &h[..40], &mut secret[..])?;
    h[40..64].copy_from_slice(&nonce);
    h[64..80].copy_from_slice(&tag);
    h[80..112].copy_from_slice(&secret[..]);
    Ok((h, admin))
}
pub(super) fn publication_key(h: &Block, pool: Id, administration: &Secret) -> Result<Secret> {
    let wrap = administration.derive(&pool, b"pool-enveloppe-v1")?;
    let mut secret = zeroize::Zeroizing::new(<[u8; 32]>::try_from(&h[80..112]).unwrap());
    wrap.open(
        h[40..64].try_into().unwrap(),
        h[64..80].try_into().unwrap(),
        &h[..40],
        &mut secret[..],
    )?;
    let key = Secret::from_bytes(*secret).derive(&pool, b"pool-publication-v1")?;
    if &h[..8] == P9_MAGIC {
        migration::verify_destination_header(h, &key)?;
    }
    Ok(key)
}

struct Media<'a, D> {
    disk: &'a mut D,
    state: &'a mut State,
    keys: &'a BTreeMap<Id, Keys>,
    pool: Id,
    generation: u64,
    /// Une lecture ciblée impose aussi le domaine attendu, indépendant du disque.
    read_scope: Option<Id>,
    write_scope: Option<Id>,
    writer: Option<(&'a mut dyn Entropy, &'a core::cell::Cell<bool>)>,
}
impl<'a, D: Device> Media<'a, D> {
    fn physical(&self, b: u64) -> Result<u64> {
        if b < CONTROL as u64 || b >= self.blocks() {
            return Err(corrupt());
        }
        Ok(FIRST_UNIT + 2 * (b - CONTROL as u64))
    }
    fn key(&self, id: Id) -> Result<Option<&'a Keys>> {
        let d = self
            .state
            .domains
            .iter()
            .find(|x| x.id == id)
            .ok_or_else(corrupt)?;
        if d.slots.is_none() {
            Ok(None)
        } else {
            Ok(Some(self.keys.get(&id).ok_or(Error::ReadOnly)?))
        }
    }
}
fn role(r: ObjectRole) -> (Kind, u64) {
    match r {
        ObjectRole::FsMetadata(i) => (Kind::Metadata, u64::from(i)),
        ObjectRole::Root => (Kind::Metadata, 1 << 32),
        ObjectRole::Leaf(i) => (Kind::Metadata, (1 << 32) + 1 + u64::from(i)),
        ObjectRole::Data(i) => (Kind::Data, u64::from(i)),
        ObjectRole::Packed(i) => (Kind::Packed, u64::from(i / 64) * 64),
    }
}
impl<D: Device> Device for Media<'_, D> {
    fn imported_genesis(&self) -> bool {
        self.state.p9
    }
    fn blocks(&self) -> u64 {
        CONTROL as u64 + (self.disk.blocks() - FIRST_UNIT) / 2
    }
    fn read(&self, b: u64) -> Result<Block> {
        self.state
            .prefix
            .get(usize::try_from(b).map_err(|_| corrupt())?)
            .copied()
            .ok_or_else(corrupt)
    }
    fn write(&mut self, b: u64, p: &Block) -> Result<()> {
        *self
            .state
            .prefix
            .get_mut(usize::try_from(b).map_err(|_| corrupt())?)
            .ok_or_else(corrupt)? = *p;
        Ok(())
    }
    // Les sept blocs de contrôle sont un overlay en mémoire. Seul le journal externe acquitte.
    fn flush(&mut self) -> Result<()> {
        Ok(())
    }
    fn typed_fs_pages(&self) -> bool {
        true
    }
    fn check_file_class(&self, b: u64, data: bool) -> Result<()> {
        let ctx = unit::context(&self.disk.read(self.physical(b)?)?)?;
        if (ctx.kind != Kind::Metadata) != data {
            return Err(corrupt());
        }
        Ok(())
    }
    fn check_lineage(&self, b: u64, ids: Ids) -> Result<()> {
        if unit::context(&self.disk.read(self.physical(b)?)?)?.domain != self.state.domain(ids)?.id
        {
            return Err(corrupt());
        }
        Ok(())
    }
    fn read_object(&self, b: u64, r: ObjectRole, hash: &Digest) -> Result<Block> {
        let addr = self.physical(b)?;
        let e = self.disk.read(addr)?;
        let c = unit::context(&e)?;
        let (kind, logical) = role(r);
        if c.pool != self.pool
            || c.physical != addr
            || (c.kind != kind && !(matches!(r, ObjectRole::Data(_)) && c.kind == Kind::Metadata))
            || c.logical != logical
            || c.generation == 0
            || c.generation > self.generation
            || self.read_scope.is_some_and(|id| id != c.domain)
        {
            return Err(corrupt());
        }
        let body = self.disk.read(addr + 1)?;
        let page = unit::open(c, self.key(c.domain)?, &e, &body, hash)?;
        Ok(page)
    }
    fn write_object(&mut self, b: u64, page: &Block, r: ObjectRole) -> Result<Digest> {
        let addr = self.physical(b)?;
        let domain = self.write_scope.ok_or(Error::ReadOnly)?;
        let (kind, logical) = role(r);
        let c = Context {
            pool: self.pool,
            domain,
            physical: addr,
            logical,
            generation: self.generation,
            kind,
        };
        let keys = self.key(domain)?;
        let (rng, dirty) = self.writer.as_mut().ok_or(Error::ReadOnly)?;
        let (e, body, hash) = unit::seal(c, keys, &mut **rng, page)?;
        dirty.set(true);
        self.disk.write(addr, &e)?;
        self.disk.write(addr + 1, &body)?;
        Ok(hash)
    }
}

/// Une sélection lit d’abord la banque du parent, puis la banque de tête.
/// Seuls les objets des catalogues g et g−1 restent épinglés par le format P7.
struct Validation<'a, 'guard> {
    expected: Head,
    pool: Id,
    admin: &'a Secret,
    header_mac: &'a Digest,
    keys: &'a BTreeMap<Id, Keys>,
    parent: Option<crate::shared_graph::Catalog>,
    parent_edges: BTreeMap<u64, (Id, u64)>,
    parent_imports: BTreeMap<Id, (Id, Id)>,
    state: Option<State>,
    full_graph: bool,
    graph: Option<VerifiedGraph<'guard>>,
}
impl<'a, 'guard> Validation<'a, 'guard> {
    fn new(
        expected: Head,
        pool: Id,
        admin: &'a Secret,
        header_mac: &'a Digest,
        keys: &'a BTreeMap<Id, Keys>,
    ) -> Self {
        Self {
            expected,
            pool,
            admin,
            header_mac,
            keys,
            parent: None,
            parent_edges: BTreeMap::new(),
            parent_imports: BTreeMap::new(),
            state: None,
            full_graph: true,
            graph: None,
        }
    }
    fn visit(
        &mut self,
        disk: &impl Device,
        h: &Head,
        exclusive: Option<&'guard crate::sync::MutexGuard<'guard, ()>>,
    ) -> Result<()> {
        let mut state = State::load(disk, self.pool, h)?;
        let size = coord(disk.blocks())?;
        let prefix = Prefix {
            state: &state,
            size,
        };
        let mut catalogs =
            crate::shared_graph::Graph::visible_catalogs(&prefix, self.pool)?.into_iter();
        if h != &self.expected {
            if h.generation.checked_add(1) != Some(self.expected.generation)
                || self.expected.parent != h.digest
                || self.parent.is_some()
            {
                return Err(corrupt());
            }
            self.parent = catalogs.next();
            self.parent_edges = state.parents;
            self.parent_imports = state.imports;
            return Ok(());
        }
        if catalogs.nth(1) != self.parent {
            return Err(Error::Corrupt("catalogue précédent P7 divergent"));
        }
        if state.p9
            && let Some(previous) = &self.parent
        {
            for view in &previous.entries {
                let domain = state.domain(view.ids)?.id;
                if state.parents.get(&view.id) != self.parent_edges.get(&view.id)
                    || state.imports.get(&domain) != self.parent_imports.get(&domain)
                {
                    return Err(Error::Corrupt("filiation P9 divergente entre publications"));
                }
            }
        }
        let header = read_header(disk)?;
        migration::check_checkpoint(&header, *h)?;
        self.admin
            .verify_mac(&[b"NovaFS/P7/session/header", &header], self.header_mac)?;
        if self.full_graph {
            state.signed_views(size, self.pool, self.keys, None, true)?;
            let media = Media {
                disk: &mut ReadOnly(disk),
                state: &mut state,
                keys: self.keys,
                pool: self.pool,
                generation: h.generation,
                read_scope: None,
                write_scope: None,
                writer: None,
            };
            if let Some(guard) = exclusive {
                self.graph = Some(SharedPool::verify_exclusive(&media, guard)?);
            } else {
                SharedPool::open(media)?;
            }
        }
        self.state = Some(state);
        Ok(())
    }
}

/// Le secret administratif est provisionné par le propriétaire, indépendamment des
/// secrets de volume. La fermeture par into_device rend le support à son propriétaire
/// et libère les clés. Une vue déverrouillée reste limitée à la durée de son appel.
pub struct SecurePool<D, E = HostEntropy> {
    clock: Option<alloc::sync::Arc<dyn crate::time::Clock>>,
    entropy: E,
    disk: D,
    pool: Id,
    admin: Secret,
    header_mac: Digest,
    log: CommitLog,
    state: State,
    keys: BTreeMap<Id, Keys>,
    failed: bool,
}
impl<D: Device> SecurePool<D> {
    pub fn format(disk: D, pool: Id, administration: &Secret) -> Result<Self> {
        Self::format_with_entropy(disk, pool, administration, HostEntropy)
    }
    pub fn open(disk: D, pool: Id, administration: &Secret) -> Result<Self> {
        Self::open_with_entropy(disk, pool, administration, HostEntropy)
    }
}
impl<D: Device, E: Entropy> SecurePool<D, E> {
    /// L'intégrateur fournit un CSPRNG fiable dont l'état ne provient jamais du pool.
    pub fn format_with_entropy(
        disk: D,
        pool: Id,
        administration: &Secret,
        entropy: E,
    ) -> Result<Self> {
        Self::format_profile(disk, pool, administration, entropy, false)
    }
    pub fn format_p9(disk: D, pool: Id, administration: &Secret, entropy: E) -> Result<Self> {
        Self::format_profile(disk, pool, administration, entropy, true)
    }
    fn format_profile(
        mut disk: D,
        pool: Id,
        administration: &Secret,
        mut entropy: E,
        p9: bool,
    ) -> Result<Self> {
        let gate = disk.mutation_lock().ok_or(Error::ReadOnly)?;
        let _exclusive = gate.try_lock().map_err(|_| Error::Busy)?;
        coord(disk.blocks())?;
        if pool == [0; 16] {
            return Err(Error::Invalid("identité du pool P7"));
        }
        for b in 0..FIRST_UNIT {
            if disk.read(b)? != [0; 4096] {
                return Err(Error::Exists);
            }
        }
        let (h, admin) = new_header(pool, disk.blocks(), administration, &mut entropy, p9, false)?;
        let mut state = State::empty();
        state.p9 = p9;
        if p9 {
            let mut buf = [0; 2048];
            state.write_parents(&mut buf)?;
            state.proofs[2048..].copy_from_slice(&buf);
        }
        let keys = BTreeMap::new();
        let log = CommitLog::format(&mut disk, 2, pool, &admin, |d, g| {
            d.write(0, &h)?;
            d.write(1, &h)?;
            SharedPool::format_p6(
                Media {
                    disk: d,
                    state: &mut state,
                    keys: &keys,
                    pool,
                    generation: g,
                    read_scope: None,
                    write_scope: None,
                    writer: None,
                },
                pool,
            )?;
            state.save(d, pool, g)
        })?;
        Ok(Self {
            disk,
            pool,
            clock: None,
            header_mac: admin.mac(&[b"NovaFS/P7/session/header", &h]),
            admin,
            entropy,
            log,
            state,
            keys,
            failed: false,
        })
    }
    pub fn open_with_entropy(
        disk: D,
        expected_pool: Id,
        administration: &Secret,
        entropy: E,
    ) -> Result<Self> {
        let gate = disk.mutation_lock();
        let _guard = gate
            .as_ref()
            .map(|g| g.try_lock().map_err(|_| Error::Busy))
            .transpose()?;
        let h = read_header(&disk)?;
        if h[16..32] != expected_pool {
            return Err(corrupt());
        }
        let pool = expected_pool;
        let admin = publication_key(&h, pool, administration)?;
        let log = CommitLog::open(&disk, 2, pool, &admin, |d, h| {
            State::load(d, pool, h).map(drop)
        })?;
        migration::check_checkpoint(&h, log.head())?;
        let state = State::load(&disk, pool, &log.head())?;
        Ok(Self {
            disk,
            pool,
            clock: None,
            header_mac: admin.mac(&[b"NovaFS/P7/session/header", &h]),
            admin,
            entropy,
            log,
            state,
            keys: BTreeMap::new(),
            failed: false,
        })
    }
    pub fn into_device(self) -> D {
        self.disk
    }
    pub fn generation(&self) -> u64 {
        self.log.head().generation
    }
    fn media(&mut self, scope: Option<Id>) -> Result<Media<'_, D>> {
        if self.failed {
            return Err(Error::Unavailable);
        }
        Ok(Media {
            disk: &mut self.disk,
            state: &mut self.state,
            keys: &self.keys,
            pool: self.pool,
            generation: self.log.head().generation,
            read_scope: scope,
            write_scope: None,
            writer: None,
        })
    }
    fn local_views(&mut self) -> Result<Vec<ObjectInfo>> {
        let pool = self.pool;
        crate::shared::views_at_head(&self.media(None)?, pool)
    }
    pub fn list(&mut self) -> Result<Vec<ObjectInfo>> {
        self.with_reader(|p| {
            p.check_physical(false)?;
            p.local_views()
        })
    }
    fn with_reader<T>(&mut self, read: impl FnOnce(&mut Self) -> Result<T>) -> Result<T> {
        if self.failed {
            return Err(Error::Unavailable);
        }
        let gate = self.disk.mutation_lock();
        let _guard = gate
            .as_ref()
            .map(|g| g.try_lock().map_err(|_| Error::Busy))
            .transpose()?;
        let result = read(self);
        if matches!(result, Err(Error::Io(_) | Error::Corrupt(_))) {
            self.failed = true;
        }
        result
    }
    fn scope(&mut self, id: u64) -> Result<Id> {
        let ids = self
            .local_views()?
            .iter()
            .find(|v| v.id == id)
            .ok_or(Error::NotFound)?
            .ids;
        Ok(self.state.domain(ids)?.id)
    }
    /// Domaines chiffrés à déverrouiller pour vérifier les deux générations,
    /// y compris une origine supprimée encore retenue par le prédécesseur.
    pub fn encrypted_domains(&self) -> Vec<Id> {
        self.state
            .domains
            .iter()
            .filter(|d| d.slots.is_some())
            .map(|d| d.id)
            .collect()
    }
    pub fn domain(&mut self, id: u64) -> Result<Id> {
        self.with_reader(|p| {
            p.check_physical(false)?;
            p.scope(id)
        })
    }
    pub fn unlock(&mut self, domain: Id, cred: Credential<'_>) -> Result<()> {
        if self.failed {
            return Err(Error::Unavailable);
        }
        let gate = self.disk.mutation_lock();
        let _guard = gate
            .as_ref()
            .map(|g| g.try_lock().map_err(|_| Error::Busy))
            .transpose()?;
        if let Err(e) = self.check_physical(false) {
            self.failed = true;
            return Err(e);
        }
        let slots = self
            .state
            .domains
            .iter()
            .find(|d| d.id == domain)
            .and_then(|d| d.slots.as_ref())
            .ok_or(Error::NotFound)?;
        let master = slots.unlock(&domain, cred)?;
        self.keys.insert(domain, Keys::derive(&master, &domain)?);
        let checked = self.state.signed_views(
            coord(self.disk.blocks())?,
            self.pool,
            &self.keys,
            Some(domain),
            true,
        );
        if let Err(e) = checked {
            self.keys.remove(&domain);
            self.failed = true;
            return Err(e);
        }
        Ok(())
    }
    pub fn lock(&mut self, domain: Id) {
        self.keys.remove(&domain);
    }
    fn available(&self) -> Result<()> {
        if self.failed {
            return Err(Error::Unavailable);
        }
        if self
            .state
            .domains
            .iter()
            .any(|d| d.slots.is_some() && !self.keys.contains_key(&d.id))
        {
            return Err(Error::ReadOnly);
        }
        Ok(())
    }
    pub fn verify_receipt(&mut self, receipt: &crate::shared::Receipt) -> Result<()> {
        self.with_reader(|p| {
            p.available()?;
            p.check_physical(true)?;
            SharedPool::open(p.media(None)?)?.verify_receipt(receipt)
        })
    }
    pub fn receipt(&mut self, id: u64) -> Result<crate::shared::Receipt> {
        self.with_reader(|p| {
            p.available()?;
            p.check_physical(true)?;
            SharedPool::open(p.media(None)?)?.receipt(id)
        })
    }
    fn check_physical(&self, full_graph: bool) -> Result<()> {
        let mut validation = Validation::new(
            self.log.head(),
            self.pool,
            &self.admin,
            &self.header_mac,
            &self.keys,
        );
        validation.full_graph = full_graph;
        let log = CommitLog::open(&self.disk, 2, self.pool, &self.admin, |d, h| {
            validation.visit(d, h, None)
        })?;
        if log.head() != self.log.head() {
            return Err(corrupt());
        }
        Ok(())
    }
    pub fn check(&mut self) -> Result<()> {
        self.with_reader(|p| {
            p.available()?;
            p.check_physical(true)
        })
    }
    fn change<T>(
        &mut self,
        scope: Id,
        domain: Option<Domain>,
        op: impl FnOnce(&mut SharedPool<Media<'_, D>>) -> Result<T>,
    ) -> Result<T> {
        self.change_parent(scope, domain, None, op)
    }
    fn change_parent<T>(
        &mut self,
        scope: Id,
        domain: Option<Domain>,
        parent: Option<(Id, u64)>,
        op: impl FnOnce(&mut SharedPool<Media<'_, D>>) -> Result<T>,
    ) -> Result<T> {
        self.change_from(scope, domain, parent, None, op)
    }
    fn change_from<T>(
        &mut self,
        scope: Id,
        domain: Option<Domain>,
        parent: Option<(Id, u64)>,
        origin: Option<(Id, Id)>,
        op: impl FnOnce(&mut SharedPool<Media<'_, D>>) -> Result<T>,
    ) -> Result<T> {
        self.available()?;
        let pool = self.pool;
        let gate = self.disk.mutation_lock().ok_or(Error::ReadOnly)?;
        let _exclusive = gate.try_lock().map_err(|_| Error::Busy)?;
        // Un drain peut stabiliser des commandes d’une instance antérieure.
        // Valider leur vue lisible, puis revalider le support après le drain.
        if self.log.needs_drain()
            && let Err(error) = self.check_physical(true)
        {
            self.failed = true;
            return Err(error);
        }
        let mut validation = Validation::new(
            self.log.head(),
            pool,
            &self.admin,
            &self.header_mac,
            &self.keys,
        );
        // Aucun transfert lors d'une reprise ou d'une modification du répertoire
        // de domaines. Le contrôle avant drain reste indépendant de celui d'après.
        let exclusive = (!self.log.needs_drain() && domain.is_none()).then_some(&_exclusive);
        let verified = self
            .log
            .resume(&mut self.disk, &self.admin, |d, h| {
                validation.visit(d, h, exclusive)
            })
            .and_then(|repaired| {
                let state = validation.state.ok_or_else(corrupt)?;
                let graph = if repaired { None } else { validation.graph };
                Ok((state, graph))
            });
        let (mut state, graph) = match verified {
            Ok(state) => state,
            Err(error) => {
                self.failed = true;
                return Err(error);
            }
        };
        let old = crate::shared::views_at_head(
            &Prefix {
                state: &state,
                size: coord(self.disk.blocks())?,
            },
            pool,
        )?;
        if let Some(d) = domain {
            if state
                .domains
                .iter()
                .any(|x| x.id == d.id || x.ids.fs == d.ids.fs || x.ids.lv == d.ids.lv)
            {
                return Err(Error::Exists);
            }
            if let Some(origin) = origin {
                if state.imports.values().any(|v| *v == origin) {
                    return Err(Error::Exists);
                }
                state.imports.insert(d.id, origin);
            }
            state.domains.push(d);
        }
        let generation = self
            .log
            .head()
            .generation
            .checked_add(1)
            .ok_or(Error::Limit("génération P7"))?;
        let submitted = core::cell::Cell::new(false);
        let result = (|| {
            let media = Media {
                disk: &mut self.disk,
                state: &mut state,
                keys: &self.keys,
                pool,
                generation,
                read_scope: None,
                write_scope: Some(scope),
                writer: Some((&mut self.entropy, &submitted)),
            };
            // Les références décrivent exactement la banque vérifiée sous ce
            // verrou. La génération suivante ne sert qu'aux nouvelles unités.
            let mut engine = match graph {
                Some(verified) => verified.attach(media),
                None => SharedPool::open(media)?,
            };
            engine.p9 = self.state.p9;
            engine.clock = self.clock.clone();
            let start = engine.generation();
            let value = op(&mut engine)?;
            let changed = engine.generation() != start;
            let current = engine.list();
            drop(engine);
            if changed {
                if state.p9 {
                    if let Some(parent) = parent {
                        let added = current
                            .iter()
                            .find(|e| !old.iter().any(|o| o.id == e.id))
                            .ok_or_else(corrupt)?;
                        state.parents.insert(added.id, parent);
                    }
                    state
                        .parents
                        .retain(|child, _| old.iter().chain(&current).any(|e| e.id == *child));
                }
                state
                    .domains
                    .retain(|d| old.iter().chain(&current).any(|e| e.ids == d.ids));
                state
                    .imports
                    .retain(|id, _| state.domains.iter().any(|d| d.id == *id));
                state.proofs = state.signed_views(
                    coord(self.disk.blocks())?,
                    pool,
                    &self.keys,
                    None,
                    false,
                )?;
                submitted.set(true);
                self.log
                    .publish(&mut self.disk, &self.admin, |d, g| state.save(d, pool, g))?;
                self.state = state;
                self.keys
                    .retain(|id, _| self.state.domains.iter().any(|d| d.id == *id));
            }
            Ok(value)
        })();
        if result.is_err() && submitted.get()
            || matches!(
                result,
                Err(Error::Io(_)
                    | Error::Corrupt(_)
                    | Error::Publication { .. }
                    | Error::UnknownOutcome
                    | Error::Unavailable)
            )
        {
            self.failed = true;
        }
        result
    }
    fn provision(&mut self, ids: Ids, credentials: Option<(&[u8], &Secret)>) -> Result<Domain> {
        let mut id = [0; 16];
        self.entropy.fill(&mut id)?;
        if id == [0; 16] || self.state.domains.iter().any(|d| d.id == id) {
            return Err(Error::Unavailable);
        }
        let slots = if let Some((phrase, recovery)) = credentials {
            let master = Secret::generate(&mut self.entropy)?;
            let slots = Slots::create(&master, &id, phrase, recovery, &mut self.entropy)?;
            self.keys.insert(id, Keys::derive(&master, &id)?);
            Some(slots)
        } else {
            None
        };
        Ok(Domain { id, ids, slots })
    }
    pub fn create(
        &mut self,
        owner: Id,
        ids: Ids,
        blocks: u64,
        policy: Compression,
        credentials: Option<(&[u8], &Secret)>,
    ) -> Result<u64> {
        let d = self.provision(ids, credentials)?;
        let scope = d.id;
        let result = self.change(scope, Some(d), |p| {
            p.create_with_compression(owner, ids, blocks, policy)
        });
        if result.is_err() {
            self.keys.remove(&scope);
        }
        result
    }
    /// Unités exclusives et partagées de la vue d'après le graphe vérifié.
    pub fn view_usage(&mut self, id: u64) -> Result<crate::shared::Usage> {
        self.with_reader(|p| {
            p.available()?;
            p.check_physical(true)?;
            SharedPool::open(p.media(None)?)?.usage(id)
        })
    }
    pub fn is_p9(&self) -> bool {
        self.state.p9
    }
    pub fn set_clock(&mut self, clock: alloc::sync::Arc<dyn crate::time::Clock>) {
        self.clock = Some(clock);
    }
    pub fn parent(&mut self, id: u64) -> Result<ViewParent> {
        self.with_reader(|p| {
            p.check_physical(false)?;
            let v = p
                .local_views()?
                .into_iter()
                .find(|v| v.id == id)
                .ok_or(Error::NotFound)?;
            let scope = p.state.domain(v.ids)?.id;
            p.media(Some(scope))?.key(scope)?;
            Ok(match (v.kind, p.state.parents.get(&id)) {
                (crate::shared::ObjectKind::Origin, _) => ViewParent::Origin,
                (_, Some(&(realm, parent))) if realm == [0; 16] => ViewParent::Exact(parent),
                (_, Some(&(realm, parent))) => ViewParent::External(realm, parent),
                _ => ViewParent::HistoricalUnknown,
            })
        })
    }
    pub fn times(&mut self, id: u64, path: &str) -> Result<crate::time::Times> {
        self.with_reader(|p| {
            p.check_physical(false)?;
            let scope = p.scope(id)?;
            let pool = p.pool;
            let media = p.media(Some(scope))?;
            media.key(scope)?;
            crate::shared::times_one(&media, pool, id, path)
        })
    }
    pub fn set_times(
        &mut self,
        id: u64,
        path: &str,
        atime: Option<crate::time::Timestamp>,
        mtime: Option<crate::time::Timestamp>,
    ) -> Result<()> {
        let scope = self.scope(id)?;
        self.change(scope, None, |p| {
            p.change(id, |fs| fs.set_times(path, atime, mtime))
        })
    }
    pub fn set_atime_policy(&mut self, id: u64, policy: crate::time::Atime) -> Result<()> {
        let scope = self.scope(id)?;
        self.change(scope, None, |p| {
            p.change(id, |fs| fs.set_atime_policy(policy))
        })
    }
    fn read_with_policy<T>(
        &mut self,
        id: u64,
        path: &str,
        inspect: impl FnOnce(&Media<'_, D>, Id) -> Result<T>,
        accessed: impl FnOnce(&T) -> bool,
        update: impl FnOnce(&mut SharedPool<Media<'_, D>>) -> Result<T>,
    ) -> Result<T> {
        let immediate = self.with_reader(|p| {
            p.check_physical(false)?;
            let writable = p.state.p9 && p.disk.mutation_lock().is_some();
            let scope = p.scope(id)?;
            let pool = p.pool;
            let now = writable.then(|| {
                p.clock
                    .as_deref()
                    .unwrap_or(&crate::time::SystemClock)
                    .now()
            });
            let media = p.media(Some(scope))?;
            media.key(scope)?;
            let value = inspect(&media, pool)?;
            if let Some(now) = now
                && accessed(&value)
                && crate::shared::access_needed(&media, pool, id, path, now)?
            {
                Ok(None)
            } else {
                Ok(Some(value))
            }
        })?;
        if let Some(value) = immediate {
            return Ok(value);
        }
        // Le coordinateur d'écriture vérifie de nouveau la tête, lit le contenu
        // et évalue la politique sur la même vue que la publication de l'accès.
        let scope = self.scope(id)?;
        self.change(scope, None, update)
    }
    pub fn list_dir(
        &mut self,
        id: u64,
        path: &str,
        after: Option<&str>,
        n: usize,
    ) -> Result<Vec<alloc::string::String>> {
        self.read_with_policy(
            id,
            path,
            |m, pool| crate::shared::list_one(m, pool, id, path, after, n),
            |_| n != 0,
            |p| p.access_dir(id, path, after, n),
        )
    }
    pub fn read_file(&mut self, id: u64, path: &str, offset: u64, n: usize) -> Result<Vec<u8>> {
        self.read_with_policy(
            id,
            path,
            |m, pool| crate::shared::read_one(m, pool, id, path, offset, n),
            |bytes| !bytes.is_empty(),
            |p| p.access_file(id, path, offset, n),
        )
    }
    /// Consultations immuables, sauvegarde et contrôle de la source de migration.
    pub fn inspect_file(&mut self, id: u64, path: &str, offset: u64, n: usize) -> Result<Vec<u8>> {
        self.with_reader(|p| {
            p.check_physical(false)?;
            let scope = p.scope(id)?;
            let pool = p.pool;
            let media = p.media(Some(scope))?;
            media.key(scope)?;
            crate::shared::read_one(&media, pool, id, path, offset, n)
        })
    }
    pub fn create_file(&mut self, id: u64, path: &str) -> Result<()> {
        let s = self.scope(id)?;
        self.change(s, None, |p| p.create_file(id, path))
    }
    pub fn write_file(&mut self, id: u64, path: &str, offset: u64, bytes: &[u8]) -> Result<()> {
        let s = self.scope(id)?;
        self.change(s, None, |p| p.write_file(id, path, offset, bytes))
    }
    pub fn set_compression(&mut self, id: u64, policy: Compression) -> Result<()> {
        let s = self.scope(id)?;
        self.change(s, None, |p| p.set_compression(id, policy))
    }
    pub fn advance(&mut self) -> Result<()> {
        self.change([0; 16], None, |p| p.advance())
    }
    pub fn mkdir(&mut self, id: u64, path: &str) -> Result<()> {
        let s = self.scope(id)?;
        self.change(s, None, |p| p.mkdir(id, path))
    }
    pub fn unlink(&mut self, id: u64, path: &str) -> Result<()> {
        let s = self.scope(id)?;
        self.change(s, None, |p| p.unlink(id, path))
    }
    pub fn rename(&mut self, id: u64, from: &str, to: &str) -> Result<()> {
        let s = self.scope(id)?;
        self.change(s, None, |p| p.rename(id, from, to))
    }
    pub fn snapshot(&mut self, id: u64) -> Result<u64> {
        let s = self.scope(id)?;
        self.change_parent(s, None, Some(([0; 16], id)), |p| p.snapshot(id))
    }
    pub fn clone_snapshot(&mut self, id: u64, owner: Id) -> Result<u64> {
        let s = self.scope(id)?;
        self.change_parent(s, None, Some(([0; 16], id)), |p| {
            p.clone_snapshot(id, owner)
        })
    }
    pub fn remove(&mut self, id: u64) -> Result<()> {
        let s = self.scope(id)?;
        self.change(s, None, |p| p.remove(id))
    }
    pub fn export(
        &mut self,
        id: u64,
        sector: u32,
        backup: &Secret,
        out: impl Write,
    ) -> Result<crate::shared_export::ExportInfo> {
        self.with_reader(|p| {
            p.available()?;
            let domain = p.scope(id)?;
            p.check_physical(true)?;
            use crate::secure_export::ExportProfile;
            let provenance = if p.state.p9 {
                let parent = p
                    .state
                    .parents
                    .get(&id)
                    .map(|&(realm, id)| (if realm == [0; 16] { p.pool } else { realm }, id));
                ExportProfile::P9 { parent }
            } else {
                ExportProfile::P7
            };
            let engine = SharedPool::open(Media {
                disk: &mut p.disk,
                state: &mut p.state,
                keys: &p.keys,
                pool: p.pool,
                generation: p.log.head().generation,
                read_scope: None,
                write_scope: None,
                writer: None,
            })?;
            crate::secure_export::export_with_parent(
                provenance,
                &engine,
                id,
                sector,
                domain,
                backup,
                &mut p.entropy,
                out,
            )
        })
    }
    pub fn restore(
        &mut self,
        input: impl Read,
        backup: &Secret,
        owner: Id,
        phrase: &[u8],
        recovery: &Secret,
        policy: Compression,
    ) -> Result<u64> {
        let (info, archive, image) = crate::secure_export::verified_with_origin(input, backup)?;
        if archive.p9 && !self.state.p9 {
            return Err(Error::Invalid("destination P9 requise pour cette archive"));
        }
        let origin = (info.pool, archive.domain);
        if self.state.p9
            && let Some(d) = self.state.domains.iter().find(|d| d.ids == info.ids)
        {
            if self.state.imports.get(&d.id) != Some(&origin) {
                return Err(Error::Exists);
            }
            let slots = d.slots.as_ref().ok_or(Error::ReadOnly)?;
            // Un ré-enrôlement implicite ne doit pas changer l'autorité sur les vues existantes.
            slots.unlock(&d.id, Credential::Recovery(recovery))?;
            slots.unlock(&d.id, Credential::Passphrase(phrase))?;
            let scope = d.id;
            return self.change_parent(scope, None, archive.parent, |p| {
                p.import_with_compression(owner, info.ids, &image, info.kind, policy)
            });
        }
        let d = self.provision(info.ids, Some((phrase, recovery)))?;
        let scope = d.id;
        let result = self.change_from(
            scope,
            Some(d),
            archive.parent,
            self.state.p9.then_some(origin),
            |p| p.import_with_compression(owner, info.ids, &image, info.kind, policy),
        );
        if result.is_err() {
            self.keys.remove(&scope);
        }
        result
    }
}
#[cfg(test)]
mod qualification_ancres;
