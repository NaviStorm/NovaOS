//! Gestion et écritures réservées au banc P01/P02. Le décodeur RO est celui du vérificateur.
use novafs::{Block, Device, Error, Result};
#[path = "../../src/pool/catalog.rs"]
mod catalog;
use catalog::{
    BAND_BYTES, Catalog, DOMAIN_QUOTA, Entry, Hash, PD, POOL, Segment, band_start, digest, load,
    validate_pair,
};
pub use catalog::{GRAIN, Type, Uid};
const ADMIN: Uid = [0x72; 16];

impl Catalog {
    fn reachable(&self) -> impl Iterator<Item = &Segment> {
        self.entries.iter().flat_map(|e| &e.segments)
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
        validate_pair(&older, old_hash, &head, hash)?;
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
impl<D: Device> Device for View<'_, D> {
    fn blocks(&self) -> u64 {
        self.entry.grains * GRAIN
    }
    fn read(&self, i: u64) -> Result<Block> {
        self.disk.read(self.entry.physical(i)?)
    }
    fn write(&mut self, i: u64, b: &Block) -> Result<()> {
        self.disk.write(self.entry.physical(i)?, b)
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
        let bytes = d.control().snapshot().bytes()?;
        let diagnosis = novafs::pool::check_pool(&d);
        assert!(matches!(
            diagnosis.slots[slot as usize],
            Err(Error::Corrupt(_))
        ));
        assert!(diagnosis.slots[(1 - slot) as usize].is_ok());
        assert!(diagnosis.pair.is_none() && diagnosis.volumes.is_empty());
        assert_eq!(d.control().snapshot().bytes()?, bytes);
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

#[test]
fn p2_catalogues_valides_localement_mais_succession_impossible() -> Result<()> {
    for sector in [512, 4096] {
        let dev = novafs::device::SimDevice::new(GRAIN * 5, sector)?;
        let control = dev.control();
        let mut pool = Pool::format(dev)?;
        // Des hashes cohérents sur les deux ancres ne transforment pas un
        // catalogue muni d'un prédécesseur en genèse du format P0.
        let mut genesis = pool.head.clone();
        genesis.previous = [1; 32];
        let mut forged = control.snapshot().device();
        let digest = write_band(&mut forged, &genesis)?;
        for slot in 0..2 {
            forged.write(slot, &anchor(slot, &genesis, digest))?;
        }
        forged.flush()?;
        let c = forged.control();
        let bytes = c.snapshot().bytes()?;
        c.trace(None, None);
        let invalid = novafs::pool::check_pool(&forged);
        assert!(
            invalid
                .slots
                .iter()
                .all(|s| matches!(s, Err(Error::Corrupt("genèse du catalogue de pool"))))
        );
        assert!(invalid.pair.is_none() && invalid.volumes.is_empty());
        assert!(c.events().is_empty());
        assert_eq!(c.snapshot().bytes()?, bytes);
        pool.create([1; 16], None, 1)?;
        pool.create([2; 16], Some([3; 16]), 1)?;
        pool.advance()?;
        let base = control.snapshot();
        // Chaque mutation est encodée et signée de nouveau. Les deux bandes
        // passent le contrôle local ; aucune nouvelle attribution n'a été autorisée.
        for case in 0..5 {
            let mut candidate = pool.head.clone();
            match case {
                0 => candidate.entries[0].segments[0].physical = 3,
                1 => candidate.entries[0].owner = [9; 16],
                2 => candidate.entries[0].id = [4; 16],
                3 => {
                    candidate.entries[1].id = [4; 16];
                    candidate.entries[1].kind = Type::Ready;
                }
                4 => candidate.previous = [0; 32],
                _ => unreachable!(),
            }
            let mut d = base.device();
            let c = d.control();
            let hash = write_band(&mut d, &candidate)?;
            let slot = candidate.generation % 2;
            d.write(slot, &anchor(slot, &candidate, hash))?;
            d.flush()?;
            let bytes = c.snapshot().bytes()?;
            c.trace(None, None);
            let report = novafs::pool::check_pool(&d);
            assert!(report.slots.iter().all(Result::is_ok));
            assert!(matches!(report.pair, Some(Err(Error::Corrupt(_)))));
            assert!(report.volumes.is_empty());
            assert!(c.events().is_empty());
            assert_eq!(c.snapshot().bytes()?, bytes);
        }
    }
    Ok(())
}
