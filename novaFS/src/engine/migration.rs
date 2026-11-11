//! Migration d'une image de contenu autonome. Les enveloppes des pools sont
//! traitées par leurs propres codecs, avant toute ouverture de leur contenu.
use super::*;
use crate::Block;

struct Candidate<'a, D> {
    disk: &'a D,
    anchors: [Block; 2],
}
impl<D: Device> Device for Candidate<'_, D> {
    fn blocks(&self) -> u64 {
        self.disk.blocks()
    }
    fn read(&self, i: u64) -> Result<Block> {
        if i < 2 {
            Ok(self.anchors[i as usize])
        } else {
            self.disk.read(i)
        }
    }
    fn write(&mut self, _: u64, _: &Block) -> Result<()> {
        Err(Error::ReadOnly)
    }
    fn flush(&mut self) -> Result<()> {
        Err(Error::ReadOnly)
    }
}

impl<D: Device> Engine<D> {
    /// Conserve les deux générations et les numéros persistants. Une image
    /// autonome historique porte des checksums, sans signature de propriétaire.
    pub fn migrate_timestamps<T: Device>(&self, mut destination: T) -> Result<T> {
        if self.atime_policy().is_some() {
            return Err(Error::Invalid("contenu historique requis"));
        }
        let source_gate = self.dev.mutation_lock();
        let target_gate = destination.mutation_lock().ok_or(Error::ReadOnly)?;
        if source_gate
            .as_ref()
            .is_some_and(|g| Arc::ptr_eq(g, &target_gate))
        {
            return Err(Error::Exists);
        }
        let _source = source_gate
            .as_ref()
            .map(|g| g.try_lock().map_err(|_| Error::Busy))
            .transpose()?;
        let _target = target_gate.try_lock().map_err(|_| Error::Busy)?;
        let (source, _) = inspect_pair(&self.dev, self.ids, true)?;
        if source.reference != self.head.reference {
            return Err(Error::Busy);
        }
        if self.dev.blocks() != destination.blocks() {
            return Err(Error::Invalid("géométrie de migration du contenu"));
        }
        for i in 0..destination.blocks() {
            if destination.read(i)? != [0; 4096] {
                return Err(Error::Exists);
            }
        }
        let mut pair = [
            Head::load(&self.dev, self.ids, 0)?,
            Head::load(&self.dev, self.ids, 1)?,
        ];
        pair.sort_by_key(|h| h.reference.birth);
        let mut preparing = [0; 4096];
        preparing[..8].copy_from_slice(b"NP9PREP1");
        preparing[8..24].copy_from_slice(&self.ids.fs);
        preparing[24..40].copy_from_slice(&self.ids.lv);
        destination.write(0, &preparing)?;
        destination.write(1, &preparing)?;
        destination.flush()?;
        for i in 2..destination.blocks() {
            if self.protected_lba(i) && !pair.iter().any(|h| h.reference.block == i) {
                destination.write(i, &self.dev.read(i)?)?;
            }
        }
        let mut heads = [[0; 4096]; 2];
        let mut parent = None;
        for h in pair {
            let mut manifest = h.m;
            manifest.atime = Some(Atime::Relative);
            if let Some((g, hash)) = parent
                && g == manifest.previous
            {
                manifest.previous_hash = hash;
            }
            let (r, b) = manifest.encode(self.ids, h.reference.block, h.reference.birth)?;
            destination.write(r.block, &b)?;
            parent = Some((r.birth, r.hash));
            heads[h.slot as usize] = Anchor {
                ids: self.ids,
                blocks: destination.blocks(),
                manifest: r,
            }
            .encode(h.slot)?;
        }
        destination.flush()?;
        let (converted, _) = inspect_pair(
            &Candidate {
                disk: &destination,
                anchors: heads,
            },
            self.ids,
            true,
        )?;
        let (again, _) = inspect_pair(&self.dev, self.ids, true)?;
        if converted.reference.birth != again.reference.birth || again.reference != source.reference
        {
            return Err(Error::Busy);
        }
        let uncertain = |cause| Error::Publication {
            phase: "publication des ancres P9",
            cause: Box::new(cause),
        };
        for (slot, a) in heads.iter().enumerate() {
            destination.write(slot as u64, a).map_err(uncertain)?;
        }
        destination.flush().map_err(uncertain)?;
        inspect_pair(&destination, self.ids, true)?;
        Ok(destination)
    }
}
