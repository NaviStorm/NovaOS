//! Nœuds CoW P0. Les parcours conservent au plus huit niveaux ; une mutation
//! réécrit le chemin modifié et, lors d’une fusion, un voisin du même parent.
use crate::codec::{HEADER, Ids, Ref, TREE, check_meta, finish_meta, le16, le64};
use crate::{Block, Error, Result};
use alloc::vec::Vec;
use core::cmp::Ordering;

#[repr(u16)]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Family {
    Objects = 1,
    Names,
    Extents,
    Space,
    Garbage,
}
impl Family {
    pub const ALL: [Self; 5] = [
        Self::Objects,
        Self::Names,
        Self::Extents,
        Self::Space,
        Self::Garbage,
    ];
    pub fn index(self) -> usize {
        self as usize - 1
    }

    fn valid_key(self, key: &[u8], search: bool) -> bool {
        match self {
            Self::Names => {
                if !(10..=265).contains(&key.len())
                    || usize::from(le16(&key[8..10])) != key.len() - 10
                {
                    return false;
                }
                let name = &key[10..];
                (search || !name.is_empty())
                    && !name.contains(&0)
                    && !name.contains(&b'/')
                    && name != b"."
                    && name != b".."
                    && core::str::from_utf8(name).is_ok()
            }
            Self::Extents => key.len() == 16,
            _ => key.len() == 8,
        }
    }

    // Appelé seulement sur des clés dont la forme a été validée à l’entrée.
    fn cmp(self, a: &[u8], b: &[u8]) -> Ordering {
        le64(&a[..8]).cmp(&le64(&b[..8])).then_with(|| match self {
            Self::Names => a[10..].cmp(&b[10..]),
            Self::Extents => le64(&a[8..]).cmp(&le64(&b[8..])),
            _ => Ordering::Equal,
        })
    }
}

pub type Record = (Vec<u8>, Vec<u8>);

fn copy_field(bytes: &[u8]) -> Result<Vec<u8>> {
    let mut field = Vec::new();
    field
        .try_reserve_exact(bytes.len())
        .map_err(|_| Error::NoMemory)?;
    field.extend_from_slice(bytes);
    Ok(field)
}

#[derive(Clone, Debug)]
pub struct Node {
    pub family: Family,
    pub level: u8,
    pub entries: Vec<Record>,
}
const CELLS: usize = HEADER + 8;
const PAGE: usize = 4096;
const MAX_LEVEL: u8 = 7;
// Le plus grand record P0 contient les empreintes des 16 blocs d’un extent.
const MAX_VALUE: usize = 18 + 16 * 32;

impl Node {
    pub fn encoded_len(&self) -> usize {
        self.entries.iter().fold(CELLS, |len, (k, v)| {
            len.saturating_add(6 + k.len() + v.len())
        })
    }

    fn check_value(&self, value: &[u8], birth: u64) -> Result<()> {
        if self.level > 0 {
            let child = Ref::from_bytes(value)?;
            if child == Ref::ZERO || child.birth > birth {
                return Err(Error::Corrupt("naissance du nœud fils"));
            }
        } else if self.family == Family::Extents {
            if value.len() < 18 {
                return Err(Error::Corrupt("extent tronqué"));
            }
            let blocks = usize::from(le16(&value[8..10]));
            let extent_birth = le64(&value[10..18]);
            if !(1..=16).contains(&blocks)
                || value.len() != 18 + 32 * blocks
                || extent_birth == 0
                || extent_birth > birth
            {
                return Err(Error::Corrupt("longueur ou naissance d’extent"));
            }
        }
        if value.len() > MAX_VALUE {
            return Err(Error::Corrupt("valeur trop longue pour P0"));
        }
        Ok(())
    }

    pub fn encode(&self, ids: Ids, block: u64, birth: u64) -> Result<(Ref, Block)> {
        if self.encoded_len() > PAGE
            || self.level > MAX_LEVEL
            || self.entries.is_empty()
            || (self.level != 0 && self.entries.len() < 2)
        {
            return Err(Error::Limit("taille ou niveau du nœud"));
        }
        let mut bytes = [0; PAGE];
        bytes[96..98].copy_from_slice(&(self.family as u16).to_le_bytes());
        bytes[98] = self.level;
        bytes[100..102].copy_from_slice(&(self.entries.len() as u16).to_le_bytes());
        let mut offset = CELLS + 2 * self.entries.len();
        let mut previous: Option<&[u8]> = None;
        for (i, (key, value)) in self.entries.iter().enumerate() {
            if !self.family.valid_key(key, false)
                || previous.is_some_and(|p| self.family.cmp(p, key) != Ordering::Less)
            {
                return Err(Error::Corrupt("clé ou ordre de nœud"));
            }
            self.check_value(value, birth)?;
            bytes[CELLS + 2 * i..CELLS + 2 * i + 2].copy_from_slice(&(offset as u16).to_le_bytes());
            bytes[offset..offset + 2].copy_from_slice(&(key.len() as u16).to_le_bytes());
            bytes[offset + 2..offset + 4].copy_from_slice(&(value.len() as u16).to_le_bytes());
            offset += 4;
            bytes[offset..offset + key.len()].copy_from_slice(key);
            offset += key.len();
            bytes[offset..offset + value.len()].copy_from_slice(value);
            offset += value.len();
            previous = Some(key);
        }
        finish_meta(bytes, TREE, ids, block, birth)
    }

    pub fn decode(bytes: &Block, ids: Ids, r: Ref, family: Family) -> Result<Self> {
        check_meta(bytes, TREE, ids, r)?;
        let count = usize::from(le16(&bytes[100..102]));
        let level = bytes[98];
        // Chaque cellule, même vide, exigerait 2 octets de table, 4
        // d’en-tête et au moins 8 de clé. Aucun Vec ne dépend d’un u16 brut.
        if le16(&bytes[96..98]) != family as u16
            || bytes[99] != 0
            || bytes[102..104] != [0; 2]
            || level > MAX_LEVEL
            || count == 0
            || count > (PAGE - CELLS) / 14
            || (level != 0 && count < 2)
        {
            return Err(Error::Corrupt("en-tête de nœud"));
        }
        let mut node = Self {
            family,
            level,
            entries: Vec::new(),
        };
        node.entries
            .try_reserve_exact(count)
            .map_err(|_| Error::NoMemory)?;
        let mut offset = CELLS + 2 * count;
        for i in 0..count {
            let pointer = usize::from(le16(&bytes[CELLS + 2 * i..CELLS + 2 * i + 2]));
            if pointer != offset || offset > PAGE - 4 {
                return Err(Error::Corrupt("table de cellules"));
            }
            let kl = usize::from(le16(&bytes[offset..offset + 2]));
            let vl = usize::from(le16(&bytes[offset + 2..offset + 4]));
            offset += 4;
            if kl + vl > PAGE - offset {
                return Err(Error::Corrupt("cellule débordante"));
            }
            let key = &bytes[offset..offset + kl];
            let value = &bytes[offset + kl..offset + kl + vl];
            if !family.valid_key(key, false)
                || node
                    .entries
                    .last()
                    .is_some_and(|(p, _)| family.cmp(p, key) != Ordering::Less)
            {
                return Err(Error::Corrupt("clés non canoniques ou non ordonnées"));
            }
            node.check_value(value, r.birth)?;
            node.entries.push((copy_field(key)?, copy_field(value)?));
            offset += kl + vl;
        }
        if bytes[offset..].iter().any(|&b| b != 0) {
            return Err(Error::Corrupt("suffixe de nœud non nul"));
        }
        Ok(node)
    }

    fn slot(&self, key: &[u8]) -> usize {
        self.entries
            .partition_point(|(k, _)| self.family.cmp(k, key) == Ordering::Less)
    }

    fn child_slot(&self, key: &[u8]) -> usize {
        self.entries
            .partition_point(|(k, _)| self.family.cmp(k, key) != Ordering::Greater)
            .saturating_sub(1)
    }

    fn child(&self, slot: usize) -> Result<Ref> {
        Ref::from_bytes(&self.entries[slot].1)
    }

    fn check_child(&self, slot: usize, child: &Node) -> Result<()> {
        if child.level.checked_add(1) != Some(self.level)
            || child.family != self.family
            || child.entries.first().map(|v| &v.0) != Some(&self.entries[slot].0)
        {
            return Err(Error::Corrupt("niveau, famille ou minimum du fils"));
        }
        if let Some((upper, _)) = self.entries.get(slot + 1) {
            if self.family.cmp(&child.entries.last().unwrap().0, upper) != Ordering::Less {
                return Err(Error::Corrupt("clés hors du séparateur parent"));
            }
        }
        Ok(())
    }
}

pub trait ReadTree {
    /// Le fournisseur valide l’enveloppe et le nœud via Node::decode, y
    /// compris pour les pages lues depuis le cache.
    fn read(&self, r: Ref, family: Family) -> Result<Node>;
}
pub trait WriteTree: ReadTree {
    /// Créer un nœud CoW ou réviser un nœud encore privé de la transaction.
    /// Une ancienne référence fournie est retirée du nouvel état.
    fn store(&mut self, old: Option<Ref>, node: &Node) -> Result<Ref>;
    fn retire(&mut self, old: Ref) -> Result<()>;
}

fn load_child(s: &impl ReadTree, parent: &Node, i: usize) -> Result<(Ref, Node)> {
    let r = parent.child(i)?;
    let child = s.read(r, parent.family)?;
    parent.check_child(i, &child)?;
    Ok((r, child))
}

#[derive(Clone, Copy)]
enum Seek {
    Exact,
    Floor,
    AtLeast,
    After,
    First,
}

fn descend(s: &impl ReadTree, node: Node, key: &[u8], mode: Seek) -> Result<Option<Record>> {
    if node.level == 0 {
        let slot = if matches!(mode, Seek::First) {
            0
        } else {
            node.slot(key)
        };
        let equal = node.entries.get(slot).is_some_and(|(k, _)| k == key);
        let index = match mode {
            Seek::Exact if !equal => return Ok(None),
            Seek::Floor if !equal => match slot.checked_sub(1) {
                Some(i) => i,
                None => return Ok(None),
            },
            Seek::After if equal => slot + 1,
            _ => slot,
        };
        return Ok(node.entries.into_iter().nth(index));
    }
    let slot = if matches!(mode, Seek::First) {
        0
    } else {
        node.child_slot(key)
    };
    let (_, child) = load_child(s, &node, slot)?;
    let found = descend(s, child, key, mode)?;
    if found.is_some()
        || !matches!(mode, Seek::AtLeast | Seek::After)
        || slot + 1 == node.entries.len()
    {
        return Ok(found);
    }
    let (_, right) = load_child(s, &node, slot + 1)?;
    descend(s, right, &[], Seek::First)
}

fn seek(s: &impl ReadTree, root: Ref, f: Family, key: &[u8], mode: Seek) -> Result<Option<Record>> {
    if !matches!(mode, Seek::First) && !f.valid_key(key, true) {
        return Err(Error::Invalid("clé de recherche"));
    }
    if root == Ref::ZERO {
        return Ok(None);
    }
    descend(s, s.read(root, f)?, key, mode)
}
pub fn get(s: &impl ReadTree, root: Ref, f: Family, key: &[u8]) -> Result<Option<Vec<u8>>> {
    Ok(seek(s, root, f, key, Seek::Exact)?.map(|(_, v)| v))
}
pub fn first(s: &impl ReadTree, root: Ref, f: Family) -> Result<Option<Record>> {
    seek(s, root, f, &[], Seek::First)
}
pub fn floor(s: &impl ReadTree, root: Ref, f: Family, key: &[u8]) -> Result<Option<Record>> {
    seek(s, root, f, key, Seek::Floor)
}
pub fn lower_bound(s: &impl ReadTree, root: Ref, f: Family, key: &[u8]) -> Result<Option<Record>> {
    seek(s, root, f, key, Seek::AtLeast)
}
pub fn next(s: &impl ReadTree, root: Ref, f: Family, key: &[u8]) -> Result<Option<Record>> {
    seek(s, root, f, key, Seek::After)
}

/// Parcours ordonné avec les bornes exactes des ancêtres. Le rappel de bloc
/// précède sa lecture, de sorte qu’un propriétaire dupliqué est détecté avant
/// de revisiter un sous-arbre. Le niveau décroît strictement à chaque appel.
pub fn walk(
    s: &impl ReadTree,
    root: Ref,
    f: Family,
    mut visit: impl FnMut(Ref) -> Result<()>,
    mut entry: impl FnMut(&[u8], &[u8]) -> Result<()>,
) -> Result<()> {
    fn scan(
        s: &impl ReadTree,
        n: &Node,
        upper: Option<&[u8]>,
        visit: &mut impl FnMut(Ref) -> Result<()>,
        entry: &mut impl FnMut(&[u8], &[u8]) -> Result<()>,
    ) -> Result<()> {
        if let Some(limit) = upper {
            if n.family.cmp(&n.entries.last().unwrap().0, limit) != Ordering::Less {
                return Err(Error::Corrupt("sous-arbre hors intervalle"));
            }
        }
        for (i, (k, v)) in n.entries.iter().enumerate() {
            if n.level == 0 {
                entry(k, v)?;
            } else {
                let r = n.child(i)?;
                visit(r)?;
                let child = s.read(r, n.family)?;
                n.check_child(i, &child)?;
                let cap = n.entries.get(i + 1).map(|r| r.0.as_slice()).or(upper);
                scan(s, &child, cap, visit, entry)?;
            }
        }
        Ok(())
    }
    if root != Ref::ZERO {
        visit(root)?;
        scan(s, &s.read(root, f)?, None, &mut visit, &mut entry)?;
    }
    Ok(())
}

// Une scission choisit une frontière proche de la moitié des octets, en
// réservant deux fils à chaque nœud interne et une entrée à chaque feuille.
fn pack(mut n: Node) -> Result<Vec<Node>> {
    if n.entries.is_empty() {
        return Ok(Vec::new());
    }
    let bytes = n.encoded_len();
    if bytes <= PAGE {
        return Ok(vec![n]);
    }
    let min = if n.level == 0 { 1 } else { 2 };
    let mut left = CELLS;
    let mut best = None;
    for (i, (key, value)) in n.entries.iter().enumerate() {
        left += 6 + key.len() + value.len();
        let cut = i + 1;
        let right = CELLS + bytes - left;
        if cut >= min && n.entries.len() - cut >= min && left <= PAGE && right <= PAGE {
            let distance = left.abs_diff(right);
            if best.is_none_or(|(_, d)| distance < d) {
                best = Some((cut, distance));
            }
        }
    }
    let (cut, _) = best.ok_or(Error::Limit("cellules incompatibles avec une scission"))?;
    let right = Node {
        family: n.family,
        level: n.level,
        entries: n.entries.split_off(cut),
    };
    Ok(vec![n, right])
}

fn persist_children(s: &mut impl WriteTree, olds: &[Ref], news: &[Node]) -> Result<Vec<Record>> {
    let mut ptrs = Vec::with_capacity(news.len());
    for (i, n) in news.iter().enumerate() {
        let r = s.store(olds.get(i).copied(), n)?;
        ptrs.push((n.entries[0].0.clone(), r.to_bytes().to_vec()));
    }
    for &unused in olds.iter().skip(news.len()) {
        s.retire(unused)?;
    }
    Ok(ptrs)
}

enum Change {
    Unchanged,
    Changed(Vec<Node>),
}

fn edit(s: &mut impl WriteTree, mut n: Node, key: &[u8], value: Option<&[u8]>) -> Result<Change> {
    if n.level == 0 {
        let slot = n.slot(key);
        let exists = n.entries.get(slot).is_some_and(|(k, _)| k == key);
        match (exists, value) {
            (false, None) => return Ok(Change::Unchanged),
            (true, Some(v)) if n.entries[slot].1 == v => return Ok(Change::Unchanged),
            (true, None) => {
                n.entries.remove(slot);
            }
            (true, Some(v)) => n.entries[slot].1 = v.to_vec(),
            (false, Some(v)) => n.entries.insert(slot, (key.to_vec(), v.to_vec())),
        }
        return Ok(Change::Changed(pack(n)?));
    }
    let slot = n.child_slot(key);
    let (old, target) = load_child(s, &n, slot)?;
    let Change::Changed(mut children) = edit(s, target, key, value)? else {
        return Ok(Change::Unchanged);
    };
    let mut start = slot;
    let mut olds = vec![old];
    // Redistribuer un nœud interne réduit à un seul fils, ou compacter une
    // page presque vide après suppression. La racine peut se contracter.
    if value.is_none()
        && children.len() == 1
        && n.entries.len() > 1
        && (children[0].encoded_len() < PAGE / 3
            || (children[0].level > 0 && children[0].entries.len() == 1))
    {
        let neighbor = if slot == 0 { 1 } else { slot - 1 };
        let (other_ref, mut other) = load_child(s, &n, neighbor)?;
        let mut changed = children.remove(0);
        if neighbor < slot {
            other.entries.append(&mut changed.entries);
            olds.insert(0, other_ref);
            start = neighbor;
            children = pack(other)?;
        } else {
            changed.entries.append(&mut other.entries);
            olds.push(other_ref);
            children = pack(changed)?;
        }
    }
    let ptrs = persist_children(s, &olds, &children)?;
    n.entries.splice(start..start + olds.len(), ptrs);
    Ok(Change::Changed(pack(n)?))
}

fn change(
    s: &mut impl WriteTree,
    root: Ref,
    f: Family,
    key: &[u8],
    value: Option<&[u8]>,
) -> Result<Ref> {
    if !f.valid_key(key, false) || value.is_some_and(|v| v.len() > MAX_VALUE) {
        return Err(Error::Invalid("clé ou valeur du B+Tree"));
    }
    let old = if root == Ref::ZERO { None } else { Some(root) };
    let n = match old {
        Some(r) => s.read(r, f)?,
        None => Node {
            family: f,
            level: 0,
            entries: vec![],
        },
    };
    let Change::Changed(mut nodes) = edit(s, n, key, value)? else {
        return Ok(root);
    };
    if nodes.is_empty() {
        if let Some(r) = old {
            s.retire(r)?;
        }
        return Ok(Ref::ZERO);
    }
    if nodes.len() == 1 {
        let n = nodes.remove(0);
        if n.level > 0 && n.entries.len() == 1 {
            let child = n.child(0)?;
            if let Some(r) = old {
                s.retire(r)?;
            }
            return Ok(child);
        }
        return s.store(old, &n);
    }
    let level = nodes[0].level + 1;
    if level > MAX_LEVEL {
        return Err(Error::Limit("hauteur du B+Tree"));
    }
    let ptrs = persist_children(s, old.as_slice(), &nodes)?;
    s.store(
        None,
        &Node {
            family: f,
            level,
            entries: ptrs,
        },
    )
}
pub fn put(s: &mut impl WriteTree, root: Ref, f: Family, key: &[u8], value: &[u8]) -> Result<Ref> {
    change(s, root, f, key, Some(value))
}
pub fn remove(s: &mut impl WriteTree, root: Ref, f: Family, key: &[u8]) -> Result<Ref> {
    change(s, root, f, key, None)
}

#[cfg(test)]
mod tests;
