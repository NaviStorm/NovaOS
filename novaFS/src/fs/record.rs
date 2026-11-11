//! Records de feuilles P0. Chaque scalaire est lu dans une tranche vérifiée.
use super::{Kind, Stat};
use crate::{Error, Result, codec::Ref};
use alloc::{borrow::ToOwned, string::String, vec::Vec};

pub(super) const ROOT: u64 = 1;
pub(super) const MAX_DEPTH: usize = 32;
pub(super) const BLOCK_BYTES: u64 = 4096;
pub(super) const MAX_RUN: usize = 16;
pub(super) const MAX_IO: usize = 64 * 1024;

pub(super) fn number(bytes: &[u8]) -> Result<u64> {
    Ok(u64::from_le_bytes(
        bytes
            .try_into()
            .map_err(|_| Error::Corrupt("taille entière"))?,
    ))
}

pub(super) fn valid_name(name: &str) -> bool {
    (1..=255).contains(&name.len())
        && name != "."
        && name != ".."
        && !name.bytes().any(|byte| matches!(byte, 0 | b'/'))
}

pub(super) fn name_key(parent: u64, name: &str) -> Vec<u8> {
    // Tous les appelants passent un composant validé, ou la borne vide de recherche.
    let mut key = Vec::with_capacity(10 + name.len());
    key.extend_from_slice(&parent.to_le_bytes());
    key.extend_from_slice(&(name.len() as u16).to_le_bytes());
    key.extend_from_slice(name.as_bytes());
    key
}

pub(super) fn parse_name_key(key: &[u8]) -> Result<(u64, &str)> {
    let head = key.get(..10).ok_or(Error::Corrupt("clé de nom tronquée"))?;
    let len = usize::from(u16::from_le_bytes([head[8], head[9]]));
    let name = core::str::from_utf8(&key[10..]).map_err(|_| Error::Corrupt("nom UTF-8"))?;
    if len != name.len() || !valid_name(name) {
        return Err(Error::Corrupt("nom interdit"));
    }
    Ok((number(&head[..8])?, name))
}

pub(super) fn extent_key(object: u64, logical: u64) -> [u8; 16] {
    let mut key = [0; 16];
    key[..8].copy_from_slice(&object.to_le_bytes());
    key[8..].copy_from_slice(&logical.to_le_bytes());
    key
}

pub(super) fn parse_extent_key(key: &[u8]) -> Result<(u64, u64)> {
    if key.len() != 16 {
        return Err(Error::Corrupt("clé d’extent"));
    }
    Ok((number(&key[..8])?, number(&key[8..])?))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Object {
    pub times: crate::time::Times,
    pub stat: Stat,
    pub dead: bool,
    pub parent: u64,
    pub name: String,
}

impl Object {
    pub fn new(id: u64, kind: Kind, parent: u64, name: &str, revision: u64) -> Self {
        Self {
            times: crate::time::Times::UNKNOWN,
            stat: Stat {
                id,
                kind,
                size: 0,
                revision,
            },
            dead: false,
            parent,
            name: name.to_owned(),
        }
    }

    pub fn encode(&self, p9: bool) -> Vec<u8> {
        let mut out = Vec::with_capacity(28 + self.name.len() + if p9 { 48 } else { 0 });
        out.push(match self.stat.kind {
            Kind::File => 1,
            Kind::Directory => 2,
        });
        out.extend_from_slice(&self.stat.revision.to_le_bytes());
        out.extend_from_slice(&self.stat.size.to_le_bytes());
        out.push(u8::from(self.dead));
        out.extend_from_slice(&self.parent.to_le_bytes());
        out.extend_from_slice(&(self.name.len() as u16).to_le_bytes());
        out.extend_from_slice(self.name.as_bytes());
        if p9 {
            out.extend_from_slice(&self.times.encode());
        }
        out
    }

    pub fn decode(id: u64, data: &[u8], generation: u64, p9: bool) -> Result<Self> {
        let head = data.get(..28).ok_or(Error::Corrupt("objet tronqué"))?;
        let kind = match head[0] {
            1 => Kind::File,
            2 => Kind::Directory,
            _ => return Err(Error::Corrupt("type d’objet")),
        };
        let revision = number(&head[1..9])?;
        let size = number(&head[9..17])?;
        let dead = match head[17] {
            0 => false,
            1 => true,
            _ => return Err(Error::Corrupt("état d’objet")),
        };
        let parent = number(&head[18..26])?;
        let len = usize::from(u16::from_le_bytes([head[26], head[27]]));
        let end = 28 + len;
        let name = core::str::from_utf8(data.get(28..end).ok_or(Error::Corrupt("nom tronqué"))?)
            .map_err(|_| Error::Corrupt("nom d’objet"))?;
        let times = match &data[end..] {
            [] => crate::time::Times::UNKNOWN,
            bytes if p9 => crate::time::Times::decode(bytes)?,
            _ => return Err(Error::Corrupt("suffixe d’objet historique")),
        };
        let attachment_ok = if id == ROOT {
            !dead && kind == Kind::Directory && parent == ROOT && name.is_empty()
        } else if dead {
            kind == Kind::File && parent == 0 && name.is_empty()
        } else {
            parent != 0 && parent != id && valid_name(name)
        };
        if id == 0
            || len != name.len()
            || !attachment_ok
            || size > i64::MAX as u64
            || (kind == Kind::Directory && size != 0)
            || revision == 0
            || revision > generation
        {
            return Err(Error::Corrupt("contenu d’objet"));
        }
        Ok(Self {
            times,
            stat: Stat {
                id,
                kind,
                size,
                revision,
            },
            dead,
            parent,
            name: name.to_owned(),
        })
    }
}

#[derive(Clone)]
pub(super) struct Run {
    pub first: u64,
    pub birth: u64,
    pub hashes: Vec<[u8; 32]>,
}

impl Run {
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(18 + self.hashes.len() * 32);
        out.extend_from_slice(&self.first.to_le_bytes());
        out.extend_from_slice(&(self.hashes.len() as u16).to_le_bytes());
        out.extend_from_slice(&self.birth.to_le_bytes());
        for h in &self.hashes {
            out.extend_from_slice(h);
        }
        out
    }

    pub fn decode(data: &[u8], blocks: u64, generation: u64) -> Result<Self> {
        let head = data.get(..18).ok_or(Error::Corrupt("extent tronqué"))?;
        let first = number(&head[..8])?;
        let count = usize::from(u16::from_le_bytes([head[8], head[9]]));
        let birth = number(&head[10..18])?;
        if !(1..=MAX_RUN).contains(&count)
            || data.len() != 18 + count * 32
            || first < 2
            || first
                .checked_add(count as u64)
                .is_none_or(|end| end > blocks)
            || birth == 0
            || birth > generation
        {
            return Err(Error::Corrupt("extent hors limites"));
        }
        let hashes = data[18..].as_chunks::<32>().0.to_vec();
        Ok(Self {
            first,
            birth,
            hashes,
        })
    }

    pub fn end(&self, logical: u64) -> Result<u64> {
        logical
            .checked_add(self.hashes.len() as u64)
            .ok_or(Error::Corrupt("fin d’extent"))
    }

    pub fn reference(&self, at: usize) -> Ref {
        Ref {
            block: self.first + at as u64,
            birth: self.birth,
            hash: self.hashes[at],
        }
    }

    pub fn slice(&self, range: core::ops::Range<usize>) -> Self {
        Self {
            first: self.first + range.start as u64,
            birth: self.birth,
            hashes: self.hashes[range].to_vec(),
        }
    }
}

pub(super) fn gc_value(cursor: u64) -> Vec<u8> {
    let mut out = cursor.to_le_bytes().to_vec();
    out.push(1);
    out
}
pub(super) fn gc_cursor(data: &[u8]) -> Result<u64> {
    if data.len() != 9 || data[8] != 1 {
        return Err(Error::Corrupt("curseur de collecte"));
    }
    number(&data[..8])
}
