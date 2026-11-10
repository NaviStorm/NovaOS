//! Fixture de validation de P01/P02. Le moteur de fichiers ne connaît que sa vue LV.
//! Catalogue mono-PD simulé, avec A/B physiques et trois bandes CoW distinctes.
use novafs::{Block, Device, Error, Result};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

pub type Uid = [u8; 16];
pub const POOL: Uid = [0x51; 16];
pub const PD: Uid = [0xd3; 16];
pub const GRAIN: u64 = 256;
const BAND_BLOCKS: usize = 16;
const BAND_BYTES: usize = BAND_BLOCKS * 4096;
type Hash = [u8; 32];
// Domaine du formateur et quota d’allocation épaisse choisis par le banc.
const ADMIN: Uid = [0x72; 16];
const DOMAIN_QUOTA: u64 = 4;

fn digest(domain: &[u8], bytes: &[u8]) -> Hash {
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
        let mut ids = BTreeSet::new();
        let mut fs_ids = BTreeSet::new();
        let mut physical = Vec::new();
        let mut charged: BTreeMap<Uid, u64> = BTreeMap::new();
        for e in &self.entries {
            let total = charged.entry(e.owner).or_default();
            *total = total
                .checked_add(e.grains)
                .ok_or(Error::Corrupt("quota débordant"))?;
            if e.owner == [0; 16] || *total > DOMAIN_QUOTA {
                return Err(Error::Corrupt("quota de domaine"));
            }
            if e.id == [0; 16]
                || !ids.insert(e.id)
                || (e.fs != [0; 16] && !fs_ids.insert(e.fs))
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
    fn bytes(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(BAND_BYTES);
        b.extend(b"NP0POOLC");
        b.extend(POOL);
        b.extend(PD);
        b.extend(self.generation.to_le_bytes());
        b.extend(self.previous);
        b.push(self.band);
        b.push(1);
        b.push(self.entries.len() as u8);
        for e in &self.entries {
            b.extend(e.id);
            b.extend(e.owner);
            b.extend(e.fs);
            b.push(e.kind as u8);
            b.extend(e.grains.to_le_bytes());
            b.extend((e.segments.len() as u16).to_le_bytes());
            for s in &e.segments {
                for n in [s.logical, s.physical, s.count] {
                    b.extend(n.to_le_bytes());
                }
            }
        }
        b.resize(BAND_BYTES, 0);
        b
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
            let mut segments = Vec::with_capacity(n);
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
    fn reachable(&self) -> impl Iterator<Item = &Segment> {
        self.entries.iter().flat_map(|e| &e.segments)
    }
}

fn band_start(band: u8) -> u64 {
    2 + u64::from(band) * BAND_BLOCKS as u64
}
fn anchor(slot: u64, c: &Catalog, hash: Hash) -> Block {
    let mut b = [0; 4096];
    b[..8].copy_from_slice(b"NP0POOLA");
    b[8..24].copy_from_slice(&POOL);
    b[24..40].copy_from_slice(&PD);
    b[40..48].copy_from_slice(&c.generation.to_le_bytes());
    b[48] = c.band;
    b[49] = slot as u8;
    b[50..82].copy_from_slice(&hash);
    let sum = digest(b"PoolP0.anchor", &b);
    b[82..114].copy_from_slice(&sum);
    b
}
fn write_band<D: Device>(dev: &mut D, c: &Catalog) -> Result<Hash> {
    let b = c.bytes();
    for (i, chunk) in b.chunks_exact(4096).enumerate() {
        dev.write(band_start(c.band) + i as u64, chunk.try_into().unwrap())?;
    }
    Ok(digest(b"PoolP0.catalog", &b))
}
fn load<D: Device>(dev: &D, slot: u64) -> Result<(Catalog, Hash)> {
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
        || digest(b"PoolP0.anchor", &a) != sum
    {
        return Err(Error::Corrupt("ancrage du pool"));
    }
    let mut bytes = Vec::with_capacity(BAND_BYTES);
    for i in 0..BAND_BLOCKS {
        bytes.extend(dev.read(band_start(a[48]) + i as u64)?);
    }
    let hash = digest(b"PoolP0.catalog", &bytes);
    if a[50..82] != hash {
        return Err(Error::Corrupt("checksum du catalogue"));
    }
    Ok((Catalog::decode(&bytes, a[48], g, dev.blocks())?, hash))
}

pub struct Pool<D: Device> {
    pub disk: D,
    pub head: Catalog,
    older: Catalog,
    hash: Hash,
    frozen: bool,
    exported: bool,
}

impl<D: Device> Pool<D> {
    pub fn format(mut disk: D) -> Result<Self> {
        if disk.blocks() < 2 * GRAIN {
            return Err(Error::Invalid("PD trop court"));
        }
        for index in 0..GRAIN {
            if disk.read(index)? != [0; 4096] {
                return Err(Error::Invalid("métadonnées PD déjà écrites"));
            }
        }
        let c = Catalog {
            generation: 1,
            band: 0,
            previous: [0; 32],
            entries: Vec::new(),
        };
        let h = write_band(&mut disk, &c)?;
        disk.flush()?;
        disk.write(0, &anchor(0, &c, h))?;
        disk.write(1, &anchor(1, &c, h))?;
        disk.flush()?;
        Self::import(disk)
    }
    pub fn import(mut disk: D) -> Result<Self> {
        let left = load(&disk, 0)?;
        let right = load(&disk, 1)?;
        let ((older, old_hash), (head, hash)) = if left.0.generation <= right.0.generation {
            (left, right)
        } else {
            (right, left)
        };
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
                                || x.logical + (first - x.physical)
                                    != y.logical + (first - y.physical))
                        {
                            return Err(Error::Corrupt("réattribution précoce"));
                        }
                    }
                }
            }
        }
        disk.flush()?;
        Ok(Self {
            disk,
            head,
            older,
            hash,
            frozen: false,
            exported: false,
        })
    }
    fn publish(&mut self, entries: Vec<Entry>, erase: &[Segment]) -> Result<()> {
        if self.frozen || self.exported {
            return Err(Error::Busy);
        }
        let band = (0..3)
            .find(|&n| n != self.head.band && n != self.older.band)
            .unwrap();
        let c = Catalog {
            generation: self
                .head
                .generation
                .checked_add(1)
                .ok_or(Error::Limit("générations"))?,
            previous: self.hash,
            band,
            entries,
        };
        c.validate(self.disk.blocks())?;
        self.frozen = true;
        for s in erase {
            for i in s.physical * GRAIN..(s.physical + s.count) * GRAIN {
                self.disk.write(i, &[0; 4096])?;
            }
        }
        let h = write_band(&mut self.disk, &c)?;
        self.disk.flush()?;
        let slot = c.generation % 2;
        self.disk.write(slot, &anchor(slot, &c, h))?;
        self.disk.flush()?;
        self.older = std::mem::replace(&mut self.head, c);
        self.hash = h;
        self.frozen = false;
        Ok(())
    }
    pub fn create(&mut self, id: Uid, fs: Option<Uid>, grains: u64) -> Result<()> {
        self.create_for(ADMIN, id, fs, grains)
    }
    pub fn create_for(&mut self, owner: Uid, id: Uid, fs: Option<Uid>, grains: u64) -> Result<()> {
        if owner == [0; 16] {
            return Err(Error::Invalid("domaine requis"));
        }
        let charged: u64 = self
            .head
            .entries
            .iter()
            .filter(|e| e.owner == owner)
            .map(|e| e.grains)
            .sum();
        if grains.checked_add(charged).is_none_or(|n| n > DOMAIN_QUOTA) {
            return Err(Error::Limit("quota du domaine"));
        }
        if self.head.entries.iter().any(|e| e.id == id) {
            return Err(Error::Exists);
        }
        if self.head.entries.len() == 16 {
            return Err(Error::Limit("quota LV"));
        }
        if grains == 0 {
            return Err(Error::Invalid("LV vide"));
        }
        let end = self.disk.blocks() / GRAIN;
        let mut remaining = grains;
        let mut segments: Vec<Segment> = Vec::new();
        for physical in 1..end {
            if self
                .head
                .reachable()
                .chain(self.older.reachable())
                .any(|s| (s.physical..s.physical + s.count).contains(&physical))
            {
                continue;
            }
            if let Some(last) = segments
                .last_mut()
                .filter(|s| s.physical + s.count == physical)
            {
                last.count += 1;
            } else {
                if segments.len() >= 256 {
                    return Err(Error::Limit("segments"));
                }
                segments.push(Segment {
                    logical: grains - remaining,
                    physical,
                    count: 1,
                });
            }
            remaining -= 1;
            if remaining == 0 {
                break;
            }
        }
        if remaining != 0 {
            return Err(Error::NoSpace);
        }
        let mut list = self.head.entries.clone();
        list.push(Entry {
            id,
            owner,
            fs: fs.unwrap_or([0; 16]),
            kind: if fs.is_some() {
                Type::Preparing
            } else {
                Type::Raw
            },
            grains,
            segments: segments.clone(),
        });
        self.publish(list, &segments)
    }
    pub fn remove(&mut self, id: Uid) -> Result<()> {
        let mut list = self.head.entries.clone();
        let i = list
            .iter()
            .position(|e| e.id == id)
            .ok_or(Error::NotFound)?;
        list.remove(i);
        self.publish(list, &[])
    }
    pub fn advance(&mut self) -> Result<()> {
        self.publish(self.head.entries.clone(), &[])
    }
    pub fn mark_ready(&mut self, id: Uid, fs: Uid) -> Result<()> {
        {
            let view = self.view(id, Type::Preparing, fs)?;
            let engine =
                novafs::Engine::mount(view, novafs::Ids { lv: id, fs }, novafs::Mode::ReadOnly)?;
            engine.verify()?;
        }
        let mut list = self.head.entries.clone();
        list.iter_mut().find(|e| e.id == id).unwrap().kind = Type::Ready;
        self.publish(list, &[])
    }
    /// Le dispatcher du banc attache le principal au canal admis ; le corps
    /// fourni par le client ne choisit que le volume demandé.
    pub fn raw_for(&mut self, principal: Uid, id: Uid) -> Result<View<'_, D>> {
        if !self
            .head
            .entries
            .iter()
            .any(|e| e.id == id && e.owner == principal && e.kind == Type::Raw)
        {
            return Err(Error::Invalid("autorité RAW absente"));
        }
        self.view(id, Type::Raw, [0; 16])
    }
    pub fn view(&mut self, id: Uid, kind: Type, fs: Uid) -> Result<View<'_, D>> {
        if self.exported || self.frozen {
            return Err(Error::Busy);
        }
        let e = self
            .head
            .entries
            .iter()
            .find(|e| e.id == id)
            .ok_or(Error::NotFound)?;
        if e.kind != kind || e.fs != fs {
            return Err(Error::Invalid("type ou identité LV"));
        }
        self.exported = true;
        Ok(View {
            disk: &mut self.disk,
            entry: e,
            exported: &mut self.exported,
        })
    }
}

/// Le droit d'export de test est exclusif. Le prêt mutable empêche un
/// changement de géométrie jusqu'à la fermeture de la dernière E/S synchrone.
pub struct View<'a, D: Device> {
    disk: &'a mut D,
    entry: &'a Entry,
    exported: &'a mut bool,
}
impl<D: Device> Drop for View<'_, D> {
    fn drop(&mut self) {
        *self.exported = false;
    }
}
impl<D: Device> View<'_, D> {
    fn physical(&self, i: u64) -> Result<u64> {
        if i >= self.blocks() {
            return Err(Error::Invalid("hors LV"));
        }
        let grain = i / GRAIN;
        let s = self
            .entry
            .segments
            .iter()
            .find(|s| (s.logical..s.logical + s.count).contains(&grain))
            .ok_or(Error::Corrupt("trou"))?;
        Ok((s.physical + (grain - s.logical)) * GRAIN + i % GRAIN)
    }
}
impl<D: Device> Device for View<'_, D> {
    fn blocks(&self) -> u64 {
        self.entry.grains * GRAIN
    }
    fn read(&self, i: u64) -> Result<Block> {
        self.disk.read(self.physical(i)?)
    }
    fn write(&mut self, i: u64, b: &Block) -> Result<()> {
        self.disk.write(self.physical(i)?, b)
    }
    fn flush(&mut self) -> Result<()> {
        self.disk.flush()
    }
}

#[test]
fn p01_rejects_forged_catalogues_even_with_valid_digests() -> Result<()> {
    use novafs::device::SimDevice;
    let d = SimDevice::new(5 * GRAIN, 512)?;
    let h = d.control();
    let mut p = Pool::format(d)?;
    p.create([1; 16], None, 1)?;
    p.create([2; 16], Some([3; 16]), 1)?;
    let cap = h.snapshot();
    for mutate in 0..7 {
        let mut d = cap.device();
        let mut c = p.head.clone();
        match mutate {
            0 => c.entries[1].id = c.entries[0].id,
            1 => c.entries[1].segments[0].physical = c.entries[0].segments[0].physical,
            2 => c.entries[0].segments[0].physical = 5,
            3 => c.entries[0].segments[0].physical = 0,
            4 => c.entries[1].segments[0].logical = 1,
            5 => c.entries[0].segments[0].count = u64::MAX,
            6 => {}
            _ => unreachable!(),
        }
        let mut b = c.bytes();
        if mutate == 6 {
            b[81] = 2;
        }
        let hash = digest(b"PoolP0.catalog", &b);
        for (i, blk) in b.chunks_exact(4096).enumerate() {
            d.write(band_start(c.band) + i as u64, blk.try_into().unwrap())?;
        }
        d.flush()?;
        let slot = c.generation % 2;
        d.write(slot, &anchor(slot, &c, hash))?;
        d.flush()?;
        assert!(Pool::import(d).is_err(), "catalogue forgé {mutate}");
    }
    // Un export oublié simule un détenteur qui ne rend pas son bail. Même si son
    // emprunt Rust se termine, le droit d'écrire le catalogue reste suspendu.
    {
        let v = p.view([1; 16], Type::Raw, [0; 16])?;
        std::mem::forget(v);
        assert!(matches!(p.advance(), Err(Error::Busy)));
    }
    Ok(())
}
