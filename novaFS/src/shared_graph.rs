//! Graphe CoW du pool P5. Le complément de l'union des deux graphes protégés est
//! libre. Aucun compteur de référence indépendant ne doit être rejoué après panne.
use crate::{Block, Compression, Device, Error, Ids, MAX_BLOCKS, Result, compression};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) type Key = [u8; 16];
type Hash = [u8; 32];
pub(crate) const FIXED: u64 = 7;
const META: &[u8; 8] = b"NP5POOLH";
const SELECTOR: &[u8; 8] = b"NP5POOLS";
const CATALOG: &[u8; 8] = b"NP5POOLC";
const ROOT: &[u8; 8] = b"NP5FSROT";
const LEAF: &[u8; 8] = b"NP5FSLEF";
const LEAVES: usize = 16;
const WIDTH: usize = 64;
const MAX_VIEWS: usize = 16;
const ENTRY_SIZE: usize = 112;
const HEADER: usize = 128;
const CODES: [u8; 4] = [0, 1, 3, 2];
const COPIES: [usize; 4] = [256, 768, 1280, 3840];

pub(crate) fn full_cost(blocks: u64) -> u64 {
    blocks + blocks.div_ceil(WIDTH as u64) + 1
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Kind {
    Origin = 1,
    Snapshot = 2,
    Clone = 3,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(crate) struct PageRef {
    pub block: u64,
    pub hash: Hash,
    // Le codec de feuille persiste ce type dans un bitmap séparé des adresses.
    packed: bool,
}

impl PageRef {
    pub const EMPTY: Self = Self {
        block: 0,
        hash: [0; 32],
        packed: false,
    };

    fn write(self, dest: &mut [u8]) {
        dest[..8].copy_from_slice(&self.block.to_le_bytes());
        dest[8..40].copy_from_slice(&self.hash);
    }

    fn read(src: &[u8]) -> Self {
        Self {
            block: number(&src[..8]),
            hash: src[8..40].try_into().unwrap(),
            packed: false,
        }
    }

    fn validate(self, blocks: u64) -> Result<()> {
        if self != Self::EMPTY && (!(FIXED..blocks).contains(&self.block) || self.hash == [0; 32]) {
            return Err(Error::Corrupt("référence physique P5 hors domaine"));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Entry {
    pub id: u64,
    pub owner: Key,
    pub ids: Ids,
    pub kind: Kind,
    pub policy: Compression,
    pub blocks: u64,
    pub root: PageRef,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Catalog {
    p6: bool,
    pub pool: Key,
    pub generation: u64,
    pub next_id: u64,
    pub entries: Vec<Entry>,
}

fn number(src: &[u8]) -> u64 {
    u64::from_le_bytes(src.try_into().expect("champ interne de huit octets"))
}

fn checksum(domain: &[u8], address: u64, page: &Block) -> Hash {
    let mut s = Sha256::new();
    s.update(domain);
    s.update(address.to_le_bytes());
    s.update(page);
    s.finalize().into()
}

fn signature(kind: &[u8; 8]) -> Block {
    let mut b = [0; 4096];
    b[..8].copy_from_slice(kind);
    b[8..10].copy_from_slice(&1u16.to_le_bytes());
    b
}

/// Un décodeur P5 reconstruit ses en-têtes attendus octet par octet. Chaque
/// structure P6 porte un autre magic, une révision 2 et le bit obligatoire LZ4.
fn profile(kind: &[u8; 8], p6: bool) -> Block {
    let mut b = signature(kind);
    if p6 {
        b[2] = b'6';
        b[8..10].copy_from_slice(&2u16.to_le_bytes());
        b[10..12].copy_from_slice(&1u16.to_le_bytes());
    }
    b
}

fn agree(result: bool, reason: &'static str) -> Result<()> {
    if result {
        Ok(())
    } else {
        Err(Error::Corrupt(reason))
    }
}

fn header(pool: Key, blocks: u64, slot: u64, p6: bool) -> Block {
    let mut b = profile(META, p6);
    b[16..32].copy_from_slice(&pool);
    b[32..40].copy_from_slice(&blocks.to_le_bytes());
    b[40..48].copy_from_slice(&slot.to_le_bytes());
    let h = checksum(b"NovaFS.P5.header", slot, &b);
    b[48..80].copy_from_slice(&h);
    b
}

pub(crate) fn looks_like_pool(dev: &impl Device) -> Result<bool> {
    if dev.blocks() < FIXED {
        return Ok(false);
    }
    for (slot, magic) in [(0, META), (1, META), (2, SELECTOR)] {
        let page = dev.read(slot)?;
        if page[..8] == magic[..] || page[..8] == profile(magic, true)[..8] {
            return Ok(true);
        }
    }
    Ok(false)
}

fn marker(pool: Key, slot: u64, p6: bool) -> Block {
    let mut b = profile(SELECTOR, p6);
    b[16..32].copy_from_slice(&pool);
    for off in COPIES {
        b[off] = CODES[slot as usize];
    }
    b
}

/// Le mot Gray ne change que d'un bit entre deux publications consécutives.
/// Une déchirure old/new laisse chaque copie égale à l'un des deux codes.
fn read_markers(dev: &impl Device, pool: Key, p6: bool) -> Result<Vec<u64>> {
    let mut b = dev.read(2)?;
    let mut slots = Vec::with_capacity(2);
    for off in COPIES {
        let slot = CODES
            .iter()
            .position(|&v| v == b[off])
            .ok_or(Error::Corrupt("code de publication P5 inconnu"))? as u64;
        if !slots.contains(&slot) {
            slots.push(slot);
        }
        b[off] = 0;
    }
    agree(
        slots.len() <= 2 && b == marker(pool, 0, p6),
        "sélecteur P5 incompatible",
    )?;
    Ok(slots)
}

fn catalog_check(cat: &Catalog, blocks: u64) -> Result<()> {
    agree(
        cat.pool != [0; 16]
            && cat.generation > 0
            && cat.next_id > 0
            && cat.entries.len() <= MAX_VIEWS,
        "catalogue P5 hors profil",
    )?;
    if cat.generation == 1 {
        agree(
            cat.next_id == 1 && cat.entries.is_empty(),
            "genèse du pool P5",
        )?;
    }
    for (i, e) in cat.entries.iter().enumerate() {
        agree(
            e.id > 0
                && e.id < cat.next_id
                && e.owner != [0; 16]
                && e.ids.fs != [0; 16]
                && e.ids.lv != [0; 16]
                && (256..=1024).contains(&e.blocks)
                && e.blocks % 256 == 0,
            "descripteur de vue P5 hors profil",
        )?;
        e.root.validate(blocks)?;
        agree(
            !e.root.packed && (cat.p6 || e.policy == Compression::None),
            "type de racine ou politique incompatible",
        )?;
        let guaranteed: u64 = cat
            .entries
            .iter()
            .filter(|other| other.owner == e.owner && other.kind != Kind::Snapshot)
            .map(|other| other.blocks)
            .try_fold(0u64, u64::checked_add)
            .ok_or(Error::Corrupt("somme des garanties P5"))?;
        agree(guaranteed <= 1024, "quota du domaine P5")?;
        for other in &cat.entries[..i] {
            agree(other.id != e.id, "identifiant de vue P5 dupliqué")?;
            if other.ids.fs == e.ids.fs || other.ids.lv == e.ids.lv {
                agree(
                    other.ids == e.ids
                        && other.blocks == e.blocks
                        && !(other.kind == Kind::Origin && e.kind == Kind::Origin),
                    "collision d’identité de lignée P5",
                )?;
            }
        }
    }
    Ok(())
}

fn check_transition(old: &Catalog, new: &Catalog) -> Result<()> {
    agree(
        old.p6 == new.p6
            && old.pool == new.pool
            && old.generation.checked_add(1) == Some(new.generation)
            && new.next_id >= old.next_id,
        "filiation/compteur de vues P5",
    )?;
    let mut added = 0u64;
    for v in &new.entries {
        if let Some(p) = old.entries.iter().find(|p| p.id == v.id) {
            agree(
                p.owner == v.owner
                    && p.ids == v.ids
                    && p.kind == v.kind
                    && p.blocks == v.blocks
                    && (v.kind != Kind::Snapshot || (p.root == v.root && p.policy == v.policy)),
                "mutation d'une identité ou d'un snapshot P5",
            )?;
        } else {
            agree(v.id >= old.next_id, "réutilisation d'identifiant de vue P5")?;
            if v.kind == Kind::Origin {
                agree(
                    !old.entries
                        .iter()
                        .any(|p| p.ids.fs == v.ids.fs || p.ids.lv == v.ids.lv),
                    "nouvelle origine dans une lignée existante",
                )?;
            }
            added += 1;
        }
    }
    agree(
        old.next_id.checked_add(added) == Some(new.next_id),
        "compteur d'identifiants P5 différent du nombre de créations",
    )?;
    Ok(())
}

fn catalog_page(cat: &Catalog, parent: Hash, blocks: u64) -> Result<(Block, Hash)> {
    catalog_check(cat, blocks)?;
    let mut b = profile(CATALOG, cat.p6);
    b[16..32].copy_from_slice(&cat.pool);
    b[32..40].copy_from_slice(&cat.generation.to_le_bytes());
    b[40..48].copy_from_slice(&cat.next_id.to_le_bytes());
    b[48..80].copy_from_slice(&parent);
    b[112] = cat.entries.len() as u8;
    for (dst, e) in b[HEADER..].chunks_exact_mut(ENTRY_SIZE).zip(&cat.entries) {
        dst[..8].copy_from_slice(&e.id.to_le_bytes());
        dst[8..24].copy_from_slice(&e.owner);
        dst[24..40].copy_from_slice(&e.ids.fs);
        dst[40..56].copy_from_slice(&e.ids.lv);
        dst[56] = e.kind as u8;
        dst[57] = match e.policy {
            Compression::None => 0,
            Compression::Lz4 => 1,
        };
        dst[64..72].copy_from_slice(&e.blocks.to_le_bytes());
        e.root.write(&mut dst[72..112]);
    }
    let h = checksum(b"NovaFS.P5.catalog", 3 + cat.generation % 4, &b);
    b[80..112].copy_from_slice(&h);
    Ok((b, h))
}

struct Record {
    cat: Catalog,
    parent: Hash,
    hash: Hash,
}

fn read_catalog(dev: &impl Device, pool: Key, slot: u64, p6: bool) -> Result<Record> {
    let b = dev.read(3 + slot)?;
    let generation = number(&b[32..40]);
    let count = usize::from(b[112]);
    agree(
        b[..8] == profile(CATALOG, p6)[..8]
            && b[16..32] == pool
            && generation % 4 == slot
            && count <= MAX_VIEWS,
        "position ou profil du catalogue P5",
    )?;
    let mut entries = Vec::with_capacity(count);
    for src in b[HEADER..].chunks_exact(ENTRY_SIZE).take(count) {
        let kind = match src[56] {
            1 => Kind::Origin,
            2 => Kind::Snapshot,
            3 => Kind::Clone,
            _ => return Err(Error::Corrupt("type de vue P5 inconnu")),
        };
        entries.push(Entry {
            id: number(&src[..8]),
            owner: src[8..24].try_into().unwrap(),
            ids: Ids {
                fs: src[24..40].try_into().unwrap(),
                lv: src[40..56].try_into().unwrap(),
            },
            kind,
            policy: match src[57] {
                0 => Compression::None,
                1 if p6 => Compression::Lz4,
                _ => {
                    return Err(Error::Corrupt(
                        "politique persistante de compression inconnue",
                    ));
                }
            },
            blocks: number(&src[64..72]),
            root: PageRef::read(&src[72..112]),
        });
    }
    let cat = Catalog {
        p6,
        pool,
        generation,
        next_id: number(&b[40..48]),
        entries,
    };
    let parent = b[48..80].try_into().unwrap();
    let (expected, hash) = catalog_page(&cat, parent, dev.blocks())?;
    agree(
        b == expected,
        "checksum, version ou suffixe du catalogue P5",
    )?;
    agree(
        (cat.generation == 1) == (parent == [0; 32]),
        "parent de genèse P5",
    )?;
    Ok(Record { cat, parent, hash })
}

fn link(old: &Record, new: &Record) -> Result<()> {
    check_transition(&old.cat, &new.cat)?;
    agree(old.hash == new.parent, "empreinte du catalogue parent P5")
}

fn select(dev: &impl Device, pool: Key, p6: bool) -> Result<(Record, Option<Record>, bool)> {
    let slots = read_markers(dev, pool, p6)?;
    let mut head = read_catalog(dev, pool, slots[0], p6)?;
    if slots.len() == 2 {
        let mut other = read_catalog(dev, pool, slots[1], p6)?;
        if other.cat.generation > head.cat.generation {
            std::mem::swap(&mut head, &mut other);
        }
        link(&other, &head)?;
        return Ok((head, Some(other), true));
    }
    let previous = if head.cat.generation > 1 {
        let older = read_catalog(dev, pool, (head.cat.generation - 1) % 4, p6)?;
        link(&older, &head)?;
        Some(older)
    } else {
        None
    };
    Ok((head, previous, false))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Role {
    Root,
    Leaf(u16),
    Data(u16),
    Packed(u16),
}

impl Role {
    fn domain(self) -> &'static [u8] {
        match self {
            Role::Root => b"NovaFS.P5.root",
            Role::Leaf(_) => b"NovaFS.P5.leaf",
            Role::Data(_) => b"NovaFS.P5.page",
            Role::Packed(_) => b"NovaFS.P6.lz4",
        }
    }
}

fn checked_page(dev: &impl Device, r: PageRef, role: Role) -> Result<Block> {
    r.validate(dev.blocks())?;
    agree(r != PageRef::EMPTY, "page P5 vide inattendue")?;
    let b = dev.read(r.block)?;
    agree(
        checksum(role.domain(), r.block, &b) == r.hash,
        "checksum de page physique P5",
    )?;
    Ok(b)
}

fn pointers(dev: &impl Device, r: PageRef, role: Role, p6: bool) -> Result<Vec<PageRef>> {
    let (magic, index, count) = match role {
        Role::Root => (ROOT, 0, LEAVES),
        Role::Leaf(i) => (LEAF, i, WIDTH),
        _ => return Err(Error::Invalid("niveau de traduction du pool")),
    };
    if r == PageRef::EMPTY {
        return Ok(vec![PageRef::EMPTY; count]);
    }
    let b = checked_page(dev, r, role)?;
    let mut h = profile(magic, p6);
    let packed = if p6 && matches!(role, Role::Leaf(_)) {
        h[24..32].copy_from_slice(&b[24..32]);
        number(&b[24..32])
    } else {
        0
    };
    h[16..18].copy_from_slice(&index.to_le_bytes());
    agree(
        b[..32] == h[..32] && b[32 + 40 * count..].iter().all(|&x| x == 0),
        "en-tête ou suffixe d'une page de traduction P5",
    )?;
    let mut refs = Vec::with_capacity(count);
    for (i, bytes) in b[32..32 + 40 * count].chunks_exact(40).enumerate() {
        let mut p = PageRef::read(bytes);
        p.packed = packed & (1u64 << i) != 0;
        p.validate(dev.blocks())?;
        refs.push(p);
    }
    agree(
        refs.iter().any(|p| *p != PageRef::EMPTY),
        "nœud P5 vide non canonique",
    )?;
    Ok(refs)
}

fn read_content(dev: &impl Device, page: PageRef, lba: u16) -> Result<Block> {
    if page.packed {
        let bytes = checked_page(dev, page, Role::Packed(lba))?;
        compression::inspect(&bytes)?.page(lba)
    } else {
        checked_page(dev, page, Role::Data(lba))
    }
}

/// Les pages FS absentes sont définies comme zéro, sans consulter le dispositif.
pub(crate) fn read_page(dev: &impl Device, root: PageRef, lba: u64, p6: bool) -> Result<Block> {
    if lba >= (LEAVES * WIDTH) as u64 {
        return Err(Error::Invalid("page hors vue P5"));
    }
    let i = lba as usize / WIDTH;
    let leaf = pointers(dev, root, Role::Root, p6)?[i];
    if leaf == PageRef::EMPTY {
        return Ok([0; 4096]);
    }
    let data = pointers(dev, leaf, Role::Leaf(i as u16), p6)?[lba as usize % WIDTH];
    if data == PageRef::EMPTY {
        Ok([0; 4096])
    } else {
        read_content(dev, data, lba as u16)
    }
}

pub(crate) fn mapped_lbas(dev: &impl Device, root: PageRef, p6: bool) -> Result<Vec<u64>> {
    let mut out = Vec::new();
    for (i, leaf) in pointers(dev, root, Role::Root, p6)?.into_iter().enumerate() {
        if leaf != PageRef::EMPTY {
            for (j, data) in pointers(dev, leaf, Role::Leaf(i as u16), p6)?
                .into_iter()
                .enumerate()
            {
                if data != PageRef::EMPTY {
                    out.push((i * WIDTH + j) as u64);
                }
            }
        }
    }
    Ok(out)
}

struct Allocation {
    hash: Hash,
    role: Role,
    ids: Ids,
    blocks: u64,
    refs: u16,
    encoded_bytes: u16,
    weight: u64,
    /// Un bit par position de l'arête vérifiée dans la page parent immuable.
    occupied_slots: u64,
    children: Vec<u64>,
}

impl Allocation {
    fn child_at(&self, slot: usize) -> Option<u64> {
        let bit = 1u64 << slot;
        if self.occupied_slots & bit == 0 {
            return None;
        }
        let rank = (self.occupied_slots & (bit - 1)).count_ones() as usize;
        self.children.get(rank).copied()
    }
}

pub(crate) struct Graph {
    head: Catalog,
    previous: Option<Catalog>,
    hash: Hash,
    blocks: u64,
    used: BTreeMap<u64, Allocation>,
    pending: BTreeSet<u64>,
    cursor: u64,
    must_stabilize: bool,
    poisoned: bool,
}

impl Graph {
    pub(crate) fn format(dev: &mut impl Device, pool: Key) -> Result<Self> {
        Self::format_version(dev, pool, false)
    }

    pub(crate) fn format_p6(dev: &mut impl Device, pool: Key) -> Result<Self> {
        Self::format_version(dev, pool, true)
    }

    pub(crate) fn is_p6(&self) -> bool {
        self.head.p6
    }

    fn format_version(dev: &mut impl Device, pool: Key, p6: bool) -> Result<Self> {
        if pool == [0; 16] || !(FIXED + 1..=MAX_BLOCKS).contains(&dev.blocks()) {
            return Err(Error::Invalid("identité ou taille du pool P5"));
        }
        // Refus de convertir implicitement une ancienne image ou un pool initialisé.
        for slot in 0..FIXED {
            if dev.read(slot)? != [0; 4096] {
                return Err(Error::Invalid("zone de publication occupée"));
            }
        }
        let head = Catalog {
            p6,
            pool,
            generation: 1,
            next_id: 1,
            entries: Vec::new(),
        };
        let (catalog, hash) = catalog_page(&head, [0; 32], dev.blocks())?;
        for slot in [0, 1] {
            dev.write(slot, &header(pool, dev.blocks(), slot, p6))?;
        }
        dev.write(4, &catalog)?;
        dev.flush()?;
        dev.write(2, &marker(pool, 1, p6))?;
        dev.flush()?;
        Ok(Self {
            head,
            previous: None,
            hash,
            blocks: dev.blocks(),
            used: BTreeMap::new(),
            pending: BTreeSet::new(),
            cursor: FIXED,
            must_stabilize: false,
            poisoned: false,
        })
    }

    pub(crate) fn open(dev: &impl Device) -> Result<Self> {
        agree(
            (FIXED + 1..=MAX_BLOCKS).contains(&dev.blocks()),
            "géométrie physique P5",
        )?;
        let first = dev.read(0)?;
        let pool: Key = first[16..32].try_into().unwrap();
        let p6 = first[..8] == profile(META, true)[..8];
        agree(
            pool != [0; 16]
                && first == header(pool, dev.blocks(), 0, p6)
                && dev.read(1)? == header(pool, dev.blocks(), 1, p6),
            "géométrie, version ou identité du pool P5",
        )?;
        let (head, previous, _) = select(dev, pool, p6)?;
        let mut g = Self {
            head: head.cat,
            previous: previous.map(|v| v.cat),
            hash: head.hash,
            blocks: dev.blocks(),
            used: BTreeMap::new(),
            pending: BTreeSet::new(),
            cursor: FIXED,
            must_stabilize: true,
            poisoned: false,
        };
        let entries: Vec<Entry> = g
            .protected_catalogs()
            .flat_map(|c| c.entries.iter().cloned())
            .collect();
        for e in entries {
            // En cas d'erreur l'index privé tout entier est abandonné par open.
            g.pin(dev, &e, e.root, Role::Root, &mut Vec::new())?;
        }
        Ok(g)
    }

    pub(crate) fn catalog(&self) -> &Catalog {
        &self.head
    }

    pub(crate) fn failed(&self) -> bool {
        self.poisoned
    }

    /// Les parents vérifiés et immuables sont déjà indexés à l'import. Chaque
    /// feuille contient au plus 64 entrées ordonnées, même après divergence.
    /// L'audit et l'export suivent séparément les pointeurs relus sur le disque.
    pub(crate) fn read_indexed_page(
        &self,
        dev: &impl Device,
        root: PageRef,
        lba: u64,
    ) -> Result<Block> {
        match self.lookup(root, lba)? {
            Some(reference) => read_content(dev, reference, lba as u16),
            None => Ok([0; 4096]),
        }
    }

    pub(crate) fn is_packed(&self, root: PageRef, lba: u64) -> Result<bool> {
        Ok(self.lookup(root, lba)?.is_some_and(|r| r.packed))
    }

    fn lookup(&self, root: PageRef, lba: u64) -> Result<Option<PageRef>> {
        if lba >= (WIDTH * LEAVES) as u64 {
            return Err(Error::Invalid("LBA hors traduction"));
        }
        if root == PageRef::EMPTY {
            return Ok(None);
        }
        let page = self
            .used
            .get(&root.block)
            .filter(|a| !root.packed && a.role == Role::Root && a.hash == root.hash)
            .ok_or(Error::Corrupt("racine absente de l’index monté"))?;
        let Some(leaf) = page.child_at(lba as usize / WIDTH) else {
            return Ok(None);
        };
        let Some(block) = self.used[&leaf].child_at(lba as usize % WIDTH) else {
            return Ok(None);
        };
        let data = &self.used[&block];
        Ok(Some(PageRef {
            block,
            hash: data.hash,
            packed: matches!(data.role, Role::Packed(_)),
        }))
    }

    pub(crate) fn protected_catalogs(&self) -> impl Iterator<Item = &Catalog> {
        std::iter::once(&self.head).chain(self.previous.iter())
    }

    /// L'ouverture RO ne lance aucune barrière. Avant le premier réemploi RW,
    /// rendre stables les octets vus à l'import (F2 indéterminée du précédent écrivain).
    pub(crate) fn prepare(&mut self, dev: &mut impl Device) -> Result<()> {
        if self.poisoned {
            return Err(Error::Unavailable);
        }
        if self.must_stabilize {
            self.poisoned = true;
            dev.flush()?;
            let (head, previous, mixed) = select(dev, self.head.pool, self.is_p6())?;
            agree(
                head.hash == self.hash
                    && head.cat == self.head
                    && previous.as_ref().map(|p| &p.cat) == self.previous.as_ref(),
                "publication déplacée depuis l'import P5",
            )?;
            if mixed {
                dev.write(
                    2,
                    &marker(self.head.pool, self.head.generation % 4, self.is_p6()),
                )?;
                dev.flush()?;
            }
            self.must_stabilize = false;
            self.poisoned = false;
        }
        Ok(())
    }

    pub(crate) fn allocate(&mut self) -> Result<u64> {
        if self.poisoned || self.must_stabilize {
            return Err(Error::Unavailable);
        }
        let start = self.cursor;
        loop {
            let at = self.cursor;
            self.cursor = if at + 1 == self.blocks { FIXED } else { at + 1 };
            if !self.used.contains_key(&at) && self.pending.insert(at) {
                return Ok(at);
            }
            if self.cursor == start {
                return Err(Error::PoolFull);
            }
        }
    }

    pub(crate) fn reset_pending(&mut self) {
        self.pending.clear();
    }

    /// Une arête par parent physique distinct; une référence supplémentaire à la
    /// même racine ne visite pas ses descendants. Le journal ne sert qu'à annuler
    /// les incréments d'une préparation rejetée avant publication.
    fn pin(
        &mut self,
        dev: &impl Device,
        entry: &Entry,
        r: PageRef,
        role: Role,
        journal: &mut Vec<u64>,
    ) -> Result<u64> {
        if r == PageRef::EMPTY {
            return Ok(0);
        }
        r.validate(self.blocks)?;
        agree(
            r.packed == matches!(role, Role::Packed(_)) && (!r.packed || self.is_p6()),
            "type d’arête de compression",
        )?;
        if let Some(a) = self.used.get_mut(&r.block) {
            agree(
                a.hash == r.hash
                    && match (a.role, role) {
                        (Role::Packed(old), Role::Packed(lba)) => {
                            old / WIDTH as u16 == lba / WIDTH as u16
                                && a.occupied_slots & (1 << (lba as usize % WIDTH)) != 0
                        }
                        _ => a.role == role,
                    }
                    && a.ids == entry.ids
                    && a.blocks == entry.blocks,
                "allocation P5 partagée entre positions, types ou lignées incompatibles",
            )?;
            a.refs = a
                .refs
                .checked_add(1)
                .ok_or(Error::Corrupt("débordement de référence P5"))?;
            journal.push(r.block);
            return Ok(a.weight);
        }
        let mut weight = 1;
        let mut occupied_slots = 0;
        let mut children = Vec::new();
        let mut encoded_bytes = 0;
        if let Role::Packed(lba) = role {
            let bytes = checked_page(dev, r, role)?;
            let unit = compression::inspect(&bytes)?;
            for slot in unit.lbas() {
                agree(
                    u64::from(slot) < entry.blocks && slot / WIDTH as u16 == lba / WIDTH as u16,
                    "unité P6 hors volume ou feuille",
                )?;
                occupied_slots |= 1u64 << (slot as usize % WIDTH);
            }
            agree(
                occupied_slots & (1u64 << (lba as usize % WIDTH)) != 0,
                "LBA absent de l’unité P6",
            )?;
            unit.verify_all()?;
            encoded_bytes = unit.encoded_bytes() as u16;
        } else if let Role::Data(_) = role {
            checked_page(dev, r, role)?;
        } else {
            for (i, child) in pointers(dev, r, role, self.is_p6())?
                .into_iter()
                .enumerate()
            {
                if child == PageRef::EMPTY {
                    continue;
                }
                let (next, index) = match role {
                    Role::Root => (Role::Leaf(i as u16), i * WIDTH),
                    Role::Leaf(n) => {
                        let i = usize::from(n) * WIDTH + i;
                        let next = if child.packed {
                            Role::Packed(i as u16)
                        } else {
                            Role::Data(i as u16)
                        };
                        (next, i)
                    }
                    _ => unreachable!(),
                };
                agree(
                    (index as u64) < entry.blocks,
                    "traduction au-delà de la vue P5",
                )?;
                agree(child.block != r.block, "cycle de traduction P5")?;
                let child_weight = self.pin(dev, entry, child, next, journal)?;
                // Chaque LBA est une arête retenue, mais une unité d’une feuille
                // occupe un seul bloc physique même si plusieurs slots la nomment.
                if !child.packed || !children.contains(&child.block) {
                    weight += child_weight;
                }
                occupied_slots |= 1u64 << i;
                children.push(child.block);
            }
        }
        agree(
            !self.used.contains_key(&r.block),
            "substitution ou cycle d'un nœud P5",
        )?;
        self.used.insert(
            r.block,
            Allocation {
                hash: r.hash,
                role,
                ids: entry.ids,
                blocks: entry.blocks,
                refs: 1,
                encoded_bytes,
                weight,
                occupied_slots,
                children,
            },
        );
        journal.push(r.block);
        Ok(weight)
    }

    fn undo(&mut self, increments: Vec<u64>) {
        for b in increments.into_iter().rev() {
            let a = self.used.get_mut(&b).expect("incrément journalisé");
            a.refs -= 1;
            if a.refs == 0 {
                self.used.remove(&b);
            }
        }
    }

    fn release(&mut self, block: u64) {
        if block == 0 {
            return;
        }
        let a = self
            .used
            .get_mut(&block)
            .expect("racine protégée par le graphe");
        a.refs -= 1;
        if a.refs == 0 {
            let freed = self.used.remove(&block).expect("bloc présent");
            for child in freed.children {
                self.release(child);
            }
        }
    }

    pub(crate) fn commit(&mut self, dev: &mut impl Device, next: Catalog) -> Result<()> {
        self.prepare(dev)?;
        check_transition(&self.head, &next)?;
        let slot = next.generation % 4;
        let (bytes, hash) = catalog_page(&next, self.hash, self.blocks)?;
        let mut increments = Vec::new();
        for e in &next.entries {
            if let Err(cause) = self.pin(dev, e, e.root, Role::Root, &mut increments) {
                self.undo(increments);
                return Err(cause);
            }
        }
        // Les écritures de pages CoW et du nouveau catalogue ne ciblent jamais une
        // des deux générations retenues. Leur abandon avant F1 ne change pas le pool.
        let candidate = dev.write(3 + slot, &bytes).and_then(|()| dev.flush());
        if let Err(cause) = candidate {
            self.undo(increments);
            // La commande a pu rester en vol. Réimporter et stabiliser le
            // périphérique avant d'autoriser un nouvel emploi de ses cibles.
            self.poisoned = true;
            return Err(cause);
        }
        self.poisoned = true;
        dev.write(2, &marker(next.pool, slot, self.is_p6()))
            .map_err(|cause| Error::Publication {
                phase: "sélecteur P5",
                cause: Box::new(cause),
            })?;
        dev.flush().map_err(|cause| Error::Publication {
            phase: "F2 du pool P5",
            cause: Box::new(cause),
        })?;
        if let Some(old) = self.previous.take() {
            for v in old.entries {
                self.release(v.root.block);
            }
        }
        self.previous = Some(std::mem::replace(&mut self.head, next));
        self.hash = hash;
        self.poisoned = false;
        self.pending.clear();
        Ok(())
    }

    pub(crate) fn root_pages(&self, root: PageRef) -> Result<u64> {
        if root == PageRef::EMPTY {
            return Ok(0);
        }
        let a = self
            .used
            .get(&root.block)
            .ok_or(Error::Corrupt("racine P5 inconnue"))?;
        agree(
            a.role == Role::Root && a.hash == root.hash,
            "identité de racine P5",
        )?;
        Ok(a.weight)
    }

    fn mark(&self, b: u64, marked: &mut BTreeSet<u64>) {
        if b != 0 && marked.insert(b) {
            for &child in &self.used[&b].children {
                self.mark(child, marked);
            }
        }
    }

    /// LBA compressés de la vue, unités physiques distinctes et octets LZ4
    /// retenus par ces unités, y compris les slots survivant à une réécriture.
    pub(crate) fn compression_usage(&self, root: PageRef) -> Result<(u64, u64, u64)> {
        if root == PageRef::EMPTY {
            return Ok((0, 0, 0));
        }
        self.root_pages(root)?;
        let mut units = BTreeSet::new();
        let mut pages = 0;
        let mut bytes = 0;
        for leaf in &self.used[&root.block].children {
            for block in &self.used[leaf].children {
                let allocation = &self.used[block];
                if matches!(allocation.role, Role::Packed(_)) {
                    pages += 1;
                    if units.insert(*block) {
                        bytes += u64::from(allocation.encoded_bytes);
                    }
                }
            }
        }
        Ok((pages, units.len() as u64, bytes))
    }

    pub(crate) fn occupied(&self) -> u64 {
        FIXED + self.used.len() as u64
    }

    pub(crate) fn deferred(&self) -> u64 {
        let mut live = BTreeSet::new();
        for e in &self.head.entries {
            self.mark(e.root.block, &mut live);
        }
        (self.used.len() - live.len()) as u64
    }

    pub(crate) fn usage(&self, id: u64) -> Result<(u64, u64)> {
        let e = self
            .head
            .entries
            .iter()
            .find(|e| e.id == id)
            .ok_or(Error::NotFound)?;
        let mut own = BTreeSet::new();
        self.mark(e.root.block, &mut own);
        let mut others = BTreeSet::new();
        for other in self.head.entries.iter().filter(|e| e.id != id) {
            self.mark(other.root.block, &mut others);
        }
        let shared = own.intersection(&others).count() as u64;
        Ok((own.len() as u64 - shared, shared))
    }
}

fn store(graph: &mut Graph, dev: &mut impl Device, b: &Block, role: Role) -> Result<PageRef> {
    let block = graph.allocate()?;
    let hash = checksum(role.domain(), block, b);
    if let Err(cause) = dev.write(block, b) {
        graph.poisoned = true;
        return Err(cause);
    }
    Ok(PageRef {
        block,
        hash,
        packed: matches!(role, Role::Packed(_)),
    })
}

pub(crate) fn store_data(
    graph: &mut Graph,
    dev: &mut impl Device,
    data: &Block,
) -> Result<PageRef> {
    if *data == [0; 4096] {
        return Ok(PageRef::EMPTY);
    }
    store(graph, dev, data, Role::Data(0))
}

pub(crate) fn store_unit(
    graph: &mut Graph,
    dev: &mut impl Device,
    bytes: &Block,
) -> Result<PageRef> {
    if !graph.is_p6() {
        return Err(Error::Invalid("pool sans compression P6"));
    }
    store(graph, dev, bytes, Role::Packed(0))
}

fn store_pointers(
    graph: &mut Graph,
    dev: &mut impl Device,
    refs: &[PageRef],
    role: Role,
) -> Result<PageRef> {
    if refs.iter().all(|p| *p == PageRef::EMPTY) {
        return Ok(PageRef::EMPTY);
    }
    let (magic, idx) = match role {
        Role::Root => (ROOT, 0),
        Role::Leaf(n) => (LEAF, n),
        _ => return Err(Error::Invalid("type de nœud P5")),
    };
    let mut b = profile(magic, graph.is_p6());
    let mut mask = 0u64;
    for (slot, r) in refs.iter().enumerate() {
        if r.packed {
            agree(
                graph.is_p6() && matches!(role, Role::Leaf(_)),
                "unité P6 à un niveau interdit",
            )?;
            mask |= 1u64 << slot;
        }
    }
    b[24..32].copy_from_slice(&mask.to_le_bytes());
    b[16..18].copy_from_slice(&idx.to_le_bytes());
    for (dst, p) in b[32..].chunks_exact_mut(40).zip(refs) {
        p.write(dst);
    }
    store(graph, dev, &b, role)
}

pub(crate) fn patch_root(
    graph: &mut Graph,
    dev: &mut impl Device,
    old: PageRef,
    changes: &BTreeMap<u64, PageRef>,
) -> Result<PageRef> {
    if changes.is_empty() {
        return Ok(old);
    }
    if changes.last_key_value().unwrap().0 >= &((WIDTH * LEAVES) as u64) {
        return Err(Error::Invalid("modification hors vue P5"));
    }
    let mut root = pointers(dev, old, Role::Root, graph.is_p6())?;
    let mut modified = false;
    for (i, entry) in root.iter_mut().enumerate() {
        let start = (i * WIDTH) as u64;
        let mut range = changes.range(start..start + WIDTH as u64).peekable();
        if range.peek().is_none() {
            continue;
        }
        let role = Role::Leaf(i as u16);
        let mut leaf = pointers(dev, *entry, role, graph.is_p6())?;
        let mut touched = false;
        for (&lba, &r) in range {
            r.validate(dev.blocks())?;
            let dst = &mut leaf[(lba - start) as usize];
            if *dst != r {
                *dst = r;
                touched = true;
            }
        }
        if touched {
            *entry = store_pointers(graph, dev, &leaf, role)?;
            modified = true;
        }
    }
    if modified {
        store_pointers(graph, dev, &root, Role::Root)
    } else {
        Ok(old)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device::SimDevice;

    const POOL: Key = [0x42; 16];

    fn origin(id: u64, root: PageRef) -> Entry {
        Entry {
            id,
            root,
            blocks: 256,
            owner: [1; 16],
            kind: Kind::Origin,
            policy: Compression::None,
            ids: Ids {
                fs: [2; 16],
                lv: [3; 16],
            },
        }
    }

    fn advance(g: &mut Graph, dev: &mut impl Device, entries: Vec<Entry>) -> Result<()> {
        let mut c = g.head.clone();
        c.generation += 1;
        for e in &entries {
            c.next_id = c.next_id.max(e.id + 1);
        }
        c.entries = entries;
        g.commit(dev, c)
    }

    fn test_graph(sector: u32) -> Result<(Graph, SimDevice, Entry, PageRef)> {
        let mut d = SimDevice::new(512, sector)?;
        let mut g = Graph::format(&mut d, POOL)?;
        let page = store_data(&mut g, &mut d, &[0x74; 4096])?;
        let r = patch_root(
            &mut g,
            &mut d,
            PageRef::EMPTY,
            &BTreeMap::from([(42, page)]),
        )?;
        let view = origin(1, r);
        advance(&mut g, &mut d, vec![view.clone()])?;
        Ok((g, d, view, page))
    }

    #[test]
    fn rang_des_aretes_creuses_et_racines_divergentes() -> Result<()> {
        for sector in [512, 4096] {
            let mut dev = SimDevice::new(2048, sector)?;
            let mut graph = Graph::format(&mut dev, POOL)?;
            let mut changes = BTreeMap::new();
            for (i, lba) in [0, 5, 63, 64, 127, 1023].into_iter().enumerate() {
                let page = store_data(&mut graph, &mut dev, &[i as u8 + 1; 4096])?;
                changes.insert(lba, page);
            }
            let root = patch_root(&mut graph, &mut dev, PageRef::EMPTY, &changes)?;
            let mut first = origin(1, root);
            first.blocks = 1024;
            advance(&mut graph, &mut dev, vec![first.clone()])?;
            let replacement = store_data(&mut graph, &mut dev, &[0x87; 4096])?;
            let edits = BTreeMap::from([(63, PageRef::EMPTY), (127, replacement)]);
            let next = patch_root(&mut graph, &mut dev, root, &edits)?;
            first.root = next;
            advance(&mut graph, &mut dev, vec![first])?;
            let imported = Graph::open(&dev)?;
            for index in [&graph, &imported] {
                for view in [root, next] {
                    for lba in [0, 1, 5, 62, 63, 64, 65, 127, 128, 960, 1022, 1023] {
                        assert_eq!(
                            index.read_indexed_page(&dev, view, lba)?,
                            read_page(&dev, view, lba, false)?,
                            "position {lba}"
                        );
                    }
                    assert!(index.read_indexed_page(&dev, view, 1024).is_err());
                }
            }
            dev.control()
                .replace_stable(replacement.block, [0xac; 4096])?;
            assert!(matches!(
                imported.read_indexed_page(&dev, next, 127),
                Err(Error::Corrupt(_))
            ));
            assert_eq!(imported.read_indexed_page(&dev, root, 127)?, [5; 4096]);
        }
        Ok(())
    }

    /// Construire un catalogue dont la somme et le parent sont valides, sans
    /// autoriser le raccourci du writer qui empêcherait l'injection au montage.
    fn inject_catalog(d: &mut SimDevice, g: &Graph, c: &Catalog) -> Result<()> {
        let slot = c.generation % 4;
        let (bytes, _) = catalog_page(c, g.hash, d.blocks())?;
        d.write(3 + slot, &bytes)?;
        d.flush()?;
        d.write(2, &marker(POOL, slot, g.is_p6()))?;
        d.flush()
    }

    fn pack_at(positions: &[u16]) -> Result<Block> {
        let mut pack = compression::Packer::default();
        for &lba in positions {
            assert_eq!(
                pack.push(lba, &[1 + (lba % 250) as u8; 4096])?,
                compression::Push::Added
            );
        }
        Ok(pack.finish().unwrap())
    }

    #[test]
    fn p6_unite_partagee_poids_et_dernier_slot() -> Result<()> {
        for sector in [512, 4096] {
            let mut d = SimDevice::new(512, sector)?;
            let mut g = Graph::format_p6(&mut d, POOL)?;
            let slots = [17, 18, 35, 63];
            let encoded = pack_at(&slots)?;
            let payload = compression::inspect(&encoded)?.encoded_bytes() as u64;
            let pack = store_unit(&mut g, &mut d, &encoded)?;
            let changes = slots.into_iter().map(|n| (u64::from(n), pack)).collect();
            let root = patch_root(&mut g, &mut d, PageRef::EMPTY, &changes)?;
            let mut e = origin(1, root);
            e.policy = Compression::Lz4;
            advance(&mut g, &mut d, vec![e.clone()])?;
            let snap = Entry {
                id: 2,
                kind: Kind::Snapshot,
                ..e.clone()
            };
            advance(&mut g, &mut d, vec![e.clone(), snap.clone()])?;
            let mut retire = BTreeMap::new();
            for n in [17, 35, 63] {
                retire.insert(n, PageRef::EMPTY);
            }
            e.root = patch_root(&mut g, &mut d, root, &retire)?;
            advance(&mut g, &mut d, vec![e.clone(), snap.clone()])?;
            // Les deux racines et feuilles diffèrent, mais elles gardent un seul
            // et même bloc physique. La garantie du snapshot impute trois pages.
            assert_eq!(g.usage(1)?, (2, 1));
            assert_eq!(g.usage(2)?, (2, 1));
            assert_eq!(g.root_pages(root)?, 3);
            assert_eq!(g.compression_usage(root)?, (4, 1, payload));
            assert_eq!(g.compression_usage(e.root)?, (1, 1, payload));
            for graph in [&g, &Graph::open(&d)?] {
                assert!(graph.is_packed(e.root, 18)?);
                assert!(!graph.is_packed(e.root, 17)?);
                assert_eq!(graph.read_indexed_page(&d, e.root, 18)?, [19; 4096]);
                assert_eq!(graph.read_indexed_page(&d, e.root, 17)?, [0; 4096]);
                assert_eq!(read_page(&d, root, 17, true)?, [18; 4096]);
            }
            advance(&mut g, &mut d, vec![e])?;
            advance(&mut g, &mut d, vec![])?;
            assert!(g.used.contains_key(&pack.block));
            advance(&mut g, &mut d, vec![])?;
            assert!(!g.used.contains_key(&pack.block));
            assert_eq!(g.occupied(), FIXED);
            g.cursor = pack.block;
            assert_eq!(g.allocate()?, pack.block);
        }
        Ok(())
    }

    #[test]
    fn p6_refuse_unites_malformees_et_aretes_incompatibles() -> Result<()> {
        for sector in [512, 4096] {
            for scenario in 0..8 {
                let mut d = SimDevice::new(512, sector)?;
                let mut g = Graph::format_p6(&mut d, POOL)?;
                let mut bytes = pack_at(&[8, 9])?;
                let mut lba = 8;
                match scenario {
                    0 => bytes[10] = 255,                                    // algorithme inconnu
                    1 => bytes[8] = 2,                                       // version inconnue
                    2 => bytes[12..16].fill(255), // taille logique forgée
                    3 => bytes[34..36].fill(255), // longueur du premier flux
                    4 => bytes[36..38].copy_from_slice(&8u16.to_le_bytes()), // chevauchement
                    5 => bytes[96..100].fill(0),  // offset LZ4 nul
                    6 => lba = 7,                 // slot absent dans la même feuille
                    7 => lba = 72,                // même bit, feuille étrangère
                    _ => unreachable!(),
                }
                let pack = store_unit(&mut g, &mut d, &bytes)?;
                let root = patch_root(
                    &mut g,
                    &mut d,
                    PageRef::EMPTY,
                    &BTreeMap::from([(lba, pack)]),
                )?;
                let mut candidate = g.head.clone();
                candidate.generation += 1;
                candidate.next_id = 2;
                candidate.entries.push(origin(1, root));
                assert!(
                    matches!(g.commit(&mut d, candidate.clone()), Err(Error::Corrupt(_))),
                    "cas {scenario}"
                );
                assert_eq!(g.occupied(), FIXED);
                // Le banc écrit ensuite une tête correctement scellée : le
                // montage doit lui aussi refuser les octets et toutes les arêtes.
                inject_catalog(&mut d, &g, &candidate)?;
                assert!(
                    matches!(Graph::open(&d), Err(Error::Corrupt(_))),
                    "cas {scenario}"
                );
            }
        }
        Ok(())
    }

    #[test]
    fn p6_refuse_ancres_fs_empaquetees_aux_octets_identiques() -> Result<()> {
        use crate::shared::SharedPool;
        for sector in [512, 4096] {
            let mut pool = SharedPool::format_p6(SimDevice::new(2048, sector)?, POOL)?;
            let ids = Ids {
                fs: [11; 16],
                lv: [12; 16],
            };
            let id = pool.create([3; 16], ids, 256)?;
            pool.create_file(id, "/f")?;
            let mut disk = pool.into_device();
            let mut graph = Graph::open(&disk)?;
            graph.prepare(&mut disk)?;
            let mut entry = graph.head.entries[0].clone();
            let mut pack = compression::Packer::default();
            for lba in [0, 1] {
                let page = read_page(&disk, entry.root, lba, true)?;
                assert_eq!(&page[..8], b"NOVAP0FS");
                assert_eq!(pack.push(lba as u16, &page)?, compression::Push::Added);
            }
            let unit = store_unit(&mut graph, &mut disk, &pack.finish().unwrap())?;
            entry.root = patch_root(
                &mut graph,
                &mut disk,
                entry.root,
                &BTreeMap::from([(0, unit), (1, unit)]),
            )?;
            advance(&mut graph, &mut disk, vec![entry])?;
            // Tous les checksums externes et internes concordent ; le contrôle
            // des familles de pages reste nécessaire pour refuser ce catalogue.
            assert!(Graph::open(&disk).is_ok());
            assert!(matches!(
                SharedPool::open(disk),
                Err(Error::Corrupt(
                    "page compressée hors des extents de données"
                ))
            ));
        }
        Ok(())
    }

    #[test]
    fn p6_bits_obligatoires_et_masque_de_typage() -> Result<()> {
        for sector in [512, 4096] {
            let mut d = SimDevice::new(512, sector)?;
            let mut g = Graph::format_p6(&mut d, POOL)?;
            let raw_like_pack = pack_at(&[8, 9])?;
            let raw = store_data(&mut g, &mut d, &raw_like_pack)?;
            let root = patch_root(&mut g, &mut d, PageRef::EMPTY, &BTreeMap::from([(8, raw)]))?;
            advance(&mut g, &mut d, vec![origin(1, root)])?;
            assert_eq!(g.read_indexed_page(&d, root, 8)?, raw_like_pack);
            let addr = g.used[&root.block].children[0];
            let mut leaf = d.read(addr)?;
            leaf[24..32].copy_from_slice(&(1u64 << 8).to_le_bytes());
            let forged = store(&mut g, &mut d, &leaf, Role::Leaf(0))?;
            let mut refs = vec![PageRef::EMPTY; LEAVES];
            refs[0] = forged;
            let forged_root = store_pointers(&mut g, &mut d, &refs, Role::Root)?;
            assert!(matches!(
                advance(&mut g, &mut d, vec![origin(1, forged_root)]),
                Err(Error::Corrupt(_))
            ));
            d.flush()?;
            let mut h = header(POOL, d.blocks(), 0, true);
            h[10] |= 0x40;
            h[48..80].fill(0);
            let sum = checksum(b"NovaFS.P5.header", 0, &h);
            h[48..80].copy_from_slice(&sum);
            d.control().replace_stable(0, h)?;
            assert!(matches!(Graph::open(&d), Err(Error::Corrupt(_))));
        }
        Ok(())
    }

    #[test]
    fn alias_de_lba_lignee_et_cycle_avec_sommes_recalculees() -> Result<()> {
        for sector in [512, 4096] {
            for scenario in 0..3 {
                let (mut g, mut d, e, data) = test_graph(sector)?;
                let mut clone = e.clone();
                clone.id = 2;
                clone.kind = Kind::Clone;
                match scenario {
                    0 => {
                        clone.root =
                            patch_root(&mut g, &mut d, e.root, &BTreeMap::from([(43, data)]))?;
                    }
                    1 => {
                        clone.ids = Ids {
                            fs: [11; 16],
                            lv: [13; 16],
                        };
                    }
                    _ => {
                        let self_addr = g.allocate()?;
                        let mut raw = signature(ROOT);
                        // La racine est correctement scellée. L'arête invalide doit
                        // être refusée avant de tenter de suivre son empreinte.
                        PageRef {
                            block: self_addr,
                            packed: false,
                            hash: [11; 32],
                        }
                        .write(&mut raw[32..72]);
                        d.write(self_addr, &raw)?;
                        clone.root = PageRef {
                            block: self_addr,
                            packed: false,
                            hash: checksum(Role::Root.domain(), self_addr, &raw),
                        };
                    }
                }
                let mut c = g.head.clone();
                c.generation += 1;
                c.next_id += 1;
                c.entries.push(clone);
                let before = g.occupied();
                assert!(matches!(
                    g.commit(&mut d, c.clone()),
                    Err(Error::Corrupt(_))
                ));
                assert_eq!(g.occupied(), before);
                inject_catalog(&mut d, &g, &c)?;
                assert!(matches!(Graph::open(&d), Err(Error::Corrupt(_))));
            }
        }
        Ok(())
    }

    #[test]
    fn filiation_antirejeu_quota_et_identites_persistantes() -> Result<()> {
        for sector in [512, 4096] {
            for mode in 0..5 {
                let (mut g, mut d, e, _) = test_graph(sector)?;
                let mut c = g.head.clone();
                match mode {
                    0 => {
                        advance(&mut g, &mut d, vec![])?;
                        c.generation = g.head.generation + 1;
                    }
                    1 => {
                        c.generation += 1;
                        c.entries[0].owner = [0x77; 16];
                    }
                    2 => {
                        c.generation += 5;
                    }
                    3 => {
                        c.generation += 1;
                        c.next_id += 1;
                    }
                    _ => {
                        let snapshot = Entry {
                            id: 2,
                            kind: Kind::Snapshot,
                            ..e.clone()
                        };
                        advance(&mut g, &mut d, vec![e, snapshot])?;
                        c = g.head.clone();
                        c.generation += 1;
                        c.entries[1].root = PageRef::EMPTY;
                    }
                }
                inject_catalog(&mut d, &g, &c)?;
                assert!(matches!(Graph::open(&d), Err(Error::Corrupt(_))));
            }
            let (mut g, mut d, e, _) = test_graph(sector)?;
            let mut oversubscribed = vec![e.clone()];
            for n in 2..=5 {
                oversubscribed.push(Entry {
                    id: n,
                    kind: Kind::Clone,
                    ..e.clone()
                });
            }
            assert!(matches!(
                advance(&mut g, &mut d, oversubscribed),
                Err(Error::Corrupt(_))
            ));
        }
        Ok(())
    }

    #[test]
    fn publication_dechiree_reprise_et_codes_inconnus() -> Result<()> {
        for sector in [512, 4096] {
            let (mut g, mut d, e, _) = test_graph(sector)?;
            for _ in 0..4 {
                let old = g.head.generation;
                advance(&mut g, &mut d, vec![e.clone()])?;
                let new = g.head.generation;
                let stable = d.control().snapshot();
                for mask in 0..16 {
                    let mut partial = stable.device();
                    let mut word = marker(POOL, old % 4, false);
                    for (i, off) in COPIES.iter().enumerate() {
                        if mask & (1 << i) != 0 {
                            word[*off] = CODES[(new % 4) as usize];
                        }
                    }
                    partial.control().replace_stable(2, word)?;
                    let mut recovered = Graph::open(&partial)?;
                    assert_eq!(recovered.head.generation, if mask == 0 { old } else { new });
                    assert_eq!(partial.read(2)?, word); // open reste strictement RO.
                    // La normalisation éventuelle est stable avant l'écriture suivante.
                    advance(&mut recovered, &mut partial, vec![e.clone()])?;
                    assert_eq!(Graph::open(&partial)?.head, recovered.head);
                }
                let bad = stable.device();
                for (off, value) in [(COPIES[0], 17), (10, 1), (127, 32)] {
                    let mut raw = marker(POOL, new % 4, false);
                    raw[off] = value;
                    bad.control().replace_stable(2, raw)?;
                    assert!(matches!(Graph::open(&bad), Err(Error::Corrupt(_))));
                }
            }
        }
        Ok(())
    }

    struct Incertain {
        disque: SimDevice,
        ecriture: bool,
        flush: bool,
        ecritures: usize,
    }

    impl Device for Incertain {
        fn blocks(&self) -> u64 {
            self.disque.blocks()
        }
        fn read(&self, b: u64) -> Result<Block> {
            self.disque.read(b)
        }
        fn write(&mut self, b: u64, data: &Block) -> Result<()> {
            self.ecritures += 1;
            self.disque.write(b, data)?;
            if std::mem::take(&mut self.ecriture) {
                return Err(Error::NoSpace);
            }
            Ok(())
        }
        fn flush(&mut self) -> Result<()> {
            self.disque.flush()?;
            if std::mem::take(&mut self.flush) {
                return Err(Error::NoSpace);
            }
            Ok(())
        }
    }

    #[test]
    fn nospace_du_support_interdit_le_reemploi_avant_reimport() -> Result<()> {
        for scenario in 0..3 {
            let (mut g, disque, e, _) = test_graph(512)?;
            let mut d = Incertain {
                disque,
                ecriture: scenario != 2,
                flush: scenario == 2,
                ecritures: 0,
            };
            let result = if scenario == 0 {
                store_data(&mut g, &mut d, &[3; 4096]).map(|_| ())
            } else {
                advance(&mut g, &mut d, vec![e.clone()])
            };
            assert!(matches!(result, Err(Error::NoSpace)));
            g.reset_pending();
            let apres = d.ecritures;
            assert!(matches!(g.allocate(), Err(Error::Unavailable)));
            assert!(matches!(
                advance(&mut g, &mut d, vec![e.clone()]),
                Err(Error::Unavailable)
            ));
            assert_eq!(apres, d.ecritures);
            let mut importe = Graph::open(&d)?;
            assert_eq!(importe.head.generation, 2);
            importe.prepare(&mut d)?;
            advance(&mut importe, &mut d, vec![e])?;
            assert_eq!(Graph::open(&d)?.head.generation, 3);
        }
        Ok(())
    }

    #[test]
    fn deja_partage_creation_constante_et_liberation_apres_predecesseur() -> Result<()> {
        for sector in [512, 4096] {
            let (mut g, mut d, e, data) = test_graph(sector)?;
            let sn = Entry {
                id: 2,
                kind: Kind::Snapshot,
                ..e.clone()
            };
            let clone = Entry {
                id: 3,
                kind: Kind::Clone,
                ..e.clone()
            };
            assert_eq!(g.root_pages(e.root)?, 3);
            advance(&mut g, &mut d, vec![e.clone(), sn.clone()])?;
            advance(&mut g, &mut d, vec![e, sn.clone(), clone.clone()])?;
            assert_eq!(g.usage(clone.id)?, (0, 3));
            advance(&mut g, &mut d, vec![sn.clone(), clone.clone()])?;
            advance(&mut g, &mut d, vec![clone.clone()])?;
            assert_eq!(g.usage(clone.id)?, (3, 0));
            let mut reloaded = Graph::open(&d)?;
            reloaded.prepare(&mut d)?;
            assert_eq!(reloaded.occupied(), FIXED + 3);
            let mut available = BTreeSet::new();
            while let Ok(b) = reloaded.allocate() {
                assert!(available.insert(b));
            }
            assert_eq!(available.len() as u64, 512 - FIXED - 3);
            assert!(!available.contains(&data.block));
            assert_eq!(read_page(&d, clone.root, 42, false)?, [0x74; 4096]);
            reloaded.reset_pending();
            advance(&mut reloaded, &mut d, vec![])?;
            assert_eq!(reloaded.deferred(), 3);
            advance(&mut reloaded, &mut d, vec![])?;
            assert_eq!(reloaded.occupied(), FIXED);
            assert_eq!(Graph::open(&d)?.deferred(), 0);
        }
        Ok(())
    }
}
