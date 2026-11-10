//! Lecteur du catalogue mono-PD de laboratoire P0, partagé avec sa fixture.
use super::{Device, Error, Result};
use sha2::{Digest, Sha256};

pub type Uid = [u8; 16];
pub const POOL: Uid = [0x51; 16];
pub const PD: Uid = [0xd3; 16];
pub const GRAIN: u64 = 256;
pub(super) const BAND_BLOCKS: usize = 16;
pub(super) const BAND_BYTES: usize = BAND_BLOCKS * 4096;
pub(super) type Hash = [u8; 32];
pub(super) const DOMAIN_QUOTA: u64 = 4;

pub(super) fn digest(domain: &[u8], bytes: &[u8]) -> Hash {
    let mut sha = Sha256::new();
    sha.update(domain);
    sha.update(bytes);
    sha.finalize().into()
}
fn le(bytes: &[u8]) -> u64 {
    u64::from_le_bytes(bytes.try_into().expect("champ de huit octets"))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Type {
    Raw = 1,
    Preparing = 2,
    Ready = 3,
}

#[derive(Clone, Debug)]
pub struct Segment {
    pub logical: u64,
    pub physical: u64,
    pub count: u64,
}
#[derive(Clone, Debug)]
pub struct Entry {
    pub id: Uid,
    pub owner: Uid,
    pub fs: Uid,
    pub kind: Type,
    pub grains: u64,
    pub segments: Vec<Segment>,
}

impl Entry {
    /// Le nombre de grains a déjà été validé contre le quota (4) et les bornes du PD.
    pub(super) fn physical(&self, index: u64) -> Result<u64> {
        let grain = index / GRAIN;
        if grain >= self.grains {
            return Err(Error::Invalid("hors LV"));
        }
        for seg in &self.segments {
            if let Some(offset) = grain.checked_sub(seg.logical)
                && offset < seg.count
            {
                return seg
                    .physical
                    .checked_add(offset)
                    .and_then(|p| p.checked_mul(GRAIN))
                    .and_then(|p| p.checked_add(index % GRAIN))
                    .ok_or(Error::Corrupt("adresse PD débordante"));
            }
        }
        Err(Error::Corrupt("trou dans la table des segments"))
    }
}

#[derive(Clone, Debug)]
pub struct Catalog {
    pub generation: u64,
    pub band: u8,
    pub previous: Hash,
    pub entries: Vec<Entry>,
}

impl Catalog {
    pub fn validate(&self, disk_blocks: u64) -> Result<()> {
        if self.generation == 0 || self.band >= 3 || self.entries.len() > 16 {
            return Err(Error::Corrupt("catalogue hors profil"));
        }
        if self.generation == 1
            && (self.previous != [0; 32] || self.band != 0 || !self.entries.is_empty())
        {
            return Err(Error::Corrupt("genèse du catalogue de pool"));
        }
        let mut physical = Vec::new();
        physical
            .try_reserve_exact(256)
            .map_err(|_| Error::NoMemory)?;
        for (i, e) in self.entries.iter().enumerate() {
            let previous = &self.entries[..i];
            let total = previous
                .iter()
                .filter(|v| v.owner == e.owner)
                .try_fold(e.grains, |n, v| n.checked_add(v.grains))
                .ok_or(Error::Corrupt("quota débordant"))?;
            if e.owner == [0; 16] || total > DOMAIN_QUOTA {
                return Err(Error::Corrupt("quota de domaine"));
            }
            if e.id == [0; 16]
                || previous
                    .iter()
                    .any(|v| v.id == e.id || (e.fs != [0; 16] && v.fs == e.fs))
                || (e.kind == Type::Raw) != (e.fs == [0; 16])
                || e.grains == 0
            {
                return Err(Error::Corrupt("identité ou type LV"));
            }
            let mut next = 0;
            for seg in &e.segments {
                let end = seg
                    .physical
                    .checked_add(seg.count)
                    .ok_or(Error::Corrupt("fin PD"))?;
                if seg.count == 0
                    || seg.physical == 0
                    || seg.logical != next
                    || end > disk_blocks / GRAIN
                    || physical.len() >= 256
                {
                    return Err(Error::Corrupt("segment LV invalide"));
                }
                next = next
                    .checked_add(seg.count)
                    .ok_or(Error::Corrupt("fin LV"))?;
                physical.push((seg.physical, end));
            }
            if next != e.grains {
                return Err(Error::Corrupt("taille LV"));
            }
        }
        physical.sort_unstable();
        if physical.windows(2).any(|w| w[0].1 > w[1].0) {
            return Err(Error::Corrupt("chevauchement de LV"));
        }
        Ok(())
    }
    fn decode(b: &[u8], band: u8, generation: u64, blocks: u64) -> Result<Self> {
        if b.len() != BAND_BYTES
            || &b[..8] != b"NP0POOLC"
            || b[8..24] != POOL
            || b[24..40] != PD
            || le(&b[40..48]) != generation
            || b[80] != band
            || b[81] != 1
            || b[82] > 16
        {
            return Err(Error::Corrupt("identité, membres ou version du pool"));
        }
        let mut pos = 83;
        let mut entries = Vec::new();
        entries
            .try_reserve_exact(usize::from(b[82]))
            .map_err(|_| Error::NoMemory)?;
        let mut count = 0;
        for _ in 0..b[82] {
            let raw = b.get(pos..pos + 59).ok_or(Error::Corrupt("LV tronqué"))?;
            pos += 59;
            let n = u16::from_le_bytes(raw[57..59].try_into().unwrap()) as usize;
            count += n;
            if count > 256 {
                return Err(Error::Corrupt("trop de segments"));
            }
            let kind = match raw[48] {
                1 => Type::Raw,
                2 => Type::Preparing,
                3 => Type::Ready,
                _ => return Err(Error::Corrupt("type LV")),
            };
            let mut segments = Vec::new();
            segments.try_reserve_exact(n).map_err(|_| Error::NoMemory)?;
            for _ in 0..n {
                let s = b
                    .get(pos..pos + 24)
                    .ok_or(Error::Corrupt("segment tronqué"))?;
                pos += 24;
                segments.push(Segment {
                    logical: le(&s[..8]),
                    physical: le(&s[8..16]),
                    count: le(&s[16..24]),
                });
            }
            entries.push(Entry {
                id: raw[..16].try_into().unwrap(),
                owner: raw[16..32].try_into().unwrap(),
                fs: raw[32..48].try_into().unwrap(),
                kind,
                grains: le(&raw[49..57]),
                segments,
            });
        }
        if b[pos..].iter().any(|&x| x != 0) {
            return Err(Error::Corrupt("suffixe du catalogue"));
        }
        let catalog = Self {
            generation: generation,
            band,
            previous: b[48..80].try_into().unwrap(),
            entries,
        };
        catalog.validate(blocks)?;
        Ok(catalog)
    }
}

pub(super) fn band_start(band: u8) -> u64 {
    2 + u64::from(band) * BAND_BLOCKS as u64
}
pub(super) fn load<D: Device>(dev: &D, slot: u64) -> Result<(Catalog, Hash)> {
    if dev.blocks() < 2 * GRAIN || slot > 1 {
        return Err(Error::Corrupt("géométrie du catalogue de pool"));
    }
    let mut a = dev.read(slot)?;
    let sum: Hash = a[82..114].try_into().unwrap();
    a[82..114].fill(0);
    let g = le(&a[40..48]);
    if &a[..8] != b"NP0POOLA"
        || a[8..24] != POOL
        || a[24..40] != PD
        || a[49] != slot as u8
        || a[48] >= 3
        || g == 0
        || (g != 1 && g % 2 != slot)
        || a[114..].iter().any(|&x| x != 0)
    {
        return Err(Error::Corrupt(
            "identité, génération, position ou réserves de l’ancre du pool",
        ));
    }
    if digest(b"PoolP0.anchor", &a) != sum {
        return Err(Error::Corrupt("checksum de l’ancre du pool"));
    }
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(BAND_BYTES)
        .map_err(|_| Error::NoMemory)?;
    for i in 0..BAND_BLOCKS {
        bytes.extend(dev.read(band_start(a[48]) + i as u64)?);
    }
    let hash = digest(b"PoolP0.catalog", &bytes);
    if a[50..82] != hash {
        return Err(Error::Corrupt("checksum du catalogue"));
    }
    Ok((Catalog::decode(&bytes, a[48], g, dev.blocks())?, hash))
}

pub(super) fn validate_pair(
    older: &Catalog,
    old_hash: Hash,
    head: &Catalog,
    hash: Hash,
) -> Result<()> {
    if head.generation == 1 {
        if old_hash != hash || head.previous != [0; 32] {
            return Err(Error::Corrupt("init du pool divergente"));
        }
    } else if older.generation.checked_add(1) != Some(head.generation)
        || old_hash != head.previous
        || older.band == head.band
    {
        return Err(Error::Corrupt("filiation du pool"));
    }
    for new in &head.entries {
        let old = older.entries.iter().find(|v| v.id == new.id);
        if let Some(old) = old {
            if old.owner != new.owner
                || old.fs != new.fs
                || old.grains != new.grains
                || (old.kind != new.kind && (old.kind, new.kind) != (Type::Preparing, Type::Ready))
            {
                return Err(Error::Corrupt(
                    "changement d’identité, géométrie ou état du LV",
                ));
            }
            // Le profil P0 ne migre pas les grains d'un LV déjà attribué.
            for g in 0..new.grains {
                if old.physical(g * GRAIN)? != new.physical(g * GRAIN)? {
                    return Err(Error::Corrupt("déplacement d’un grain du LV"));
                }
            }
        } else if new.kind == Type::Ready {
            return Err(Error::Corrupt("LV prêt sans phase de préparation"));
        }
    }
    // Une plage visible des deux générations garde son propriétaire et la
    // même adresse logique. Traversée bornée par les 256 segments de P0.
    for a in &older.entries {
        for b in &head.entries {
            for x in &a.segments {
                for y in &b.segments {
                    let first = x.physical.max(y.physical);
                    if first < (x.physical + x.count).min(y.physical + y.count)
                        && (a.id != b.id
                            || a.owner != b.owner
                            || a.fs != b.fs
                            || x.logical + (first - x.physical) != y.logical + (first - y.physical))
                    {
                        return Err(Error::Corrupt("réattribution précoce"));
                    }
                }
            }
        }
    }
    Ok(())
}
