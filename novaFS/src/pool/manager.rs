//! Administration hôte du catalogue P0 : le premier grain est réservé au pool.
use super::catalog::{
    BAND_BYTES, Catalog, DOMAIN_QUOTA, Entry, Hash, PD, POOL, Segment, band_start, digest, load,
    validate_pair,
};
use super::{GRAIN, Type, Uid};
use crate::{Block, Device, Error, Result};
use std::sync::{Arc, Mutex};
const ADMIN: Uid = [0x72; 16];

impl Catalog {
    fn reachable(&self) -> impl Iterator<Item = &Segment> {
        self.entries.iter().flat_map(|e| &e.segments)
    }
    fn bytes(&self) -> Result<Vec<u8>> {
        let mut b = Vec::new();
        b.try_reserve_exact(BAND_BYTES)
            .map_err(|_| Error::NoMemory)?;
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
        Ok(b)
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
    let b = c.bytes()?;
    for (i, chunk) in b.chunks_exact(4096).enumerate() {
        dev.write(band_start(c.band) + i as u64, chunk.try_into().unwrap())?;
    }
    Ok(digest(b"PoolP0.catalog", &b))
}
#[derive(Clone, Copy)]
enum Health {
    Healthy,
    Failed,
    Publishing,
}

pub struct Pool<D: Device> {
    disk: D,
    head: Catalog,
    older: Catalog,
    hash: Hash,
    health: Health,
    pending: Arc<Mutex<Pending>>,
    exported: bool,
}

impl<D: Device> Pool<D> {
    fn healthy(&self) -> Result<()> {
        match self.health {
            Health::Healthy => Ok(()),
            Health::Failed => Err(Error::Unavailable),
            Health::Publishing => Err(Error::UnknownOutcome),
        }
    }

    fn writable(&self) -> Result<()> {
        self.healthy()?;
        if self.exported {
            return Err(Error::Busy);
        }
        Ok(())
    }

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
        for slot in [0, 1] {
            disk.write(slot, &anchor(slot, &c, h))
                .map_err(|cause| Error::Publication {
                    phase: "ancre initiale du pool",
                    cause: Box::new(cause),
                })?;
        }
        disk.flush().map_err(|cause| Error::Publication {
            phase: "F2 initial du pool",
            cause: Box::new(cause),
        })?;
        // F2 a été acquittée. Une erreur de relecture empêche de livrer un
        // écrivain ; elle ne retire pas l'acquittement de la barrière.
        let (older, head, hash) = checked_catalogs(&disk)?;
        if head.generation != 1 || hash != h {
            return Err(Error::Corrupt("genèse du pool différente du reçu"));
        }
        Ok(Self::from_catalogs(disk, older, head, hash))
    }
    pub fn import(mut disk: D) -> Result<Self> {
        let (older, head, hash) = checked_catalogs(&disk)?;
        disk.flush()?;
        Ok(Self::from_catalogs(disk, older, head, hash))
    }
    fn from_catalogs(disk: D, older: Catalog, head: Catalog, hash: Hash) -> Self {
        Self {
            disk,
            head,
            older,
            hash,
            health: Health::Healthy,
            pending: Arc::new(Mutex::new(Pending::default())),
            exported: false,
        }
    }
    fn publish(&mut self, entries: Vec<Entry>, new_volume: Option<Uid>) -> Result<()> {
        self.writable()?;
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
        let band_image = c.bytes()?;
        let h = digest(b"PoolP0.catalog", &band_image);
        validate_pair(&self.head, self.hash, &c, h)?;
        let erase = match new_volume {
            Some(id) => c
                .entries
                .iter()
                .find(|e| e.id == id)
                .ok_or(Error::Invalid("nouveau LV absent du plan"))?
                .segments
                .as_slice(),
            None => &[],
        };
        // Le candidat est prêt. Avant de réutiliser des grains, relire les
        // deux catalogues et vérifier que la tête correspond encore au reçu.
        // Les lectures sont bornées à deux bandes de 16 blocs chacune.
        self.health = Health::Failed;
        let (_, observed, observed_hash) = checked_catalogs(&self.disk)?;
        if observed_hash != self.hash || observed.generation != self.head.generation {
            return Err(Error::Corrupt("tête du pool différente du reçu"));
        }
        for s in erase {
            // c.validate garantit que la fin du segment est dans le PD.
            for i in s.physical * GRAIN..(s.physical + s.count) * GRAIN {
                self.disk.write(i, &[0; 4096])?;
            }
        }
        for (i, block) in band_image.as_chunks::<4096>().0.iter().enumerate() {
            self.disk.write(band_start(c.band) + i as u64, block)?;
        }
        self.disk.flush()?;
        let slot = c.generation % 2;
        self.health = Health::Publishing;
        self.disk
            .write(slot, &anchor(slot, &c, h))
            .map_err(|cause| Error::Publication {
                phase: "ancre du pool",
                cause: Box::new(cause),
            })?;
        self.disk.flush().map_err(|cause| Error::Publication {
            phase: "F2 du pool",
            cause: Box::new(cause),
        })?;
        self.older = std::mem::replace(&mut self.head, c);
        self.hash = h;
        self.health = Health::Healthy;
        Ok(())
    }
    pub fn create(&mut self, id: Uid, fs: Option<Uid>, grains: u64) -> Result<()> {
        self.create_for(ADMIN, id, fs, grains)
    }
    pub fn create_for(&mut self, owner: Uid, id: Uid, fs: Option<Uid>, grains: u64) -> Result<()> {
        let ticket = self.reserve_volume(owner, id, fs, grains)?;
        self.commit_volume(ticket)
    }
    pub fn remove(&mut self, id: Uid) -> Result<()> {
        let mut list = self.head.entries.clone();
        let i = list
            .iter()
            .position(|e| e.id == id)
            .ok_or(Error::NotFound)?;
        list.remove(i);
        self.publish(list, None)
    }
    pub fn advance(&mut self) -> Result<()> {
        self.publish(self.head.entries.clone(), None)
    }
    pub fn mark_ready(&mut self, id: Uid, fs: Uid) -> Result<()> {
        {
            let view = self.view(id, Type::Preparing, fs)?;
            let engine =
                crate::Engine::mount(view, crate::Ids { lv: id, fs }, crate::Mode::ReadOnly)?;
            engine.verify()?;
        }
        let mut list = self.head.entries.clone();
        list.iter_mut().find(|e| e.id == id).unwrap().kind = Type::Ready;
        self.publish(list, None)
    }
    /// L’administrateur du pool fournit l’identité déjà authentifiée du détenteur RAW.
    pub fn raw_for(&mut self, principal: Uid, id: Uid) -> Result<View<'_, D>> {
        if !self
            .head
            .entries
            .iter()
            .any(|e| e.id == id && e.owner == principal && e.kind == Type::Raw)
        {
            return Err(Error::Invalid("autorité RAW absente"));
        }
        self.export(id, Type::Raw, [0; 16])
    }
    /// Export FS pour le formateur ou le backend de l’instance désignée.
    pub fn view(&mut self, id: Uid, kind: Type, fs: Uid) -> Result<View<'_, D>> {
        if kind == Type::Raw {
            return Err(Error::Invalid("un volume RAW exige son détenteur"));
        }
        self.export(id, kind, fs)
    }
    fn export(&mut self, id: Uid, kind: Type, fs: Uid) -> Result<View<'_, D>> {
        self.writable()?;
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

/// L’export du LV est exclusif. Le prêt mutable empêche un
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
    fn logical_quota_bytes(&self) -> Option<u64> {
        // Au plus quatre grains validés par le profil P0.
        (self.entry.kind != Type::Raw).then(|| self.blocks() * 4096)
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

#[cfg(test)]
#[path = "../../tests/support/pool.rs"]
mod tests;

/// Partition de l'espace du PD, en blocs NovaFS de 4 Kio.
/// La somme de metadata, tail, assigned, deferred, reserved et available égale total.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PoolSpaceStats {
    pub total_blocks: u64,
    pub metadata_blocks: u64,
    pub tail_blocks: u64,
    pub assigned_blocks: u64,
    pub deferred_blocks: u64,
    pub reserved_blocks: u64,
    pub available_blocks: u64,
    pub shared_blocks: u64,
    pub pending_volumes: usize,
}

#[derive(Clone, Copy, Debug)]
pub struct VolumeInfo {
    pub id: Uid,
    pub fs: Uid,
    pub owner: Uid,
    pub kind: Type,
    pub guaranteed_blocks: u64,
}

impl VolumeInfo {
    fn of(e: &Entry) -> Self {
        Self {
            id: e.id,
            fs: e.fs,
            owner: e.owner,
            kind: e.kind,
            guaranteed_blocks: e.grains * GRAIN,
        }
    }
}

#[derive(Default)]
struct Pending {
    entries: [Option<Entry>; 16],
}

fn checked_catalogs<D: Device>(dev: &D) -> Result<(Catalog, Catalog, Hash)> {
    let left = load(dev, 0)?;
    let right = load(dev, 1)?;
    let ((older, old_hash), (head, hash)) = if left.0.generation <= right.0.generation {
        (left, right)
    } else {
        (right, left)
    };
    validate_pair(&older, old_hash, &head, hash)?;
    Ok((older, head, hash))
}

fn covers(segment: &Segment, grain: u64) -> bool {
    grain
        .checked_sub(segment.physical)
        .is_some_and(|distance| distance < segment.count)
}

fn space(total: u64, older: &Catalog, head: &Catalog, pending: &Pending) -> Result<PoolSpaceStats> {
    let mut deferred_grains = 0;
    // Au plus 16 LV de 4 grains. Compter les grains de la racine précédente
    // qui ont perdu leur propriétaire dans la tête. Les autres sont déjà imputés.
    for segment in older.reachable() {
        for grain in segment.physical..segment.physical + segment.count {
            if !head.reachable().any(|s| covers(s, grain)) {
                deferred_grains += 1;
            }
        }
    }
    let active = pending.entries.iter().flatten();
    let assigned_blocks = head.entries.iter().map(|e| e.grains * GRAIN).sum();
    let deferred_blocks = deferred_grains * GRAIN;
    let reserved_blocks = active.clone().map(|e| e.grains * GRAIN).sum();
    let tail_blocks = total % GRAIN;
    let available_blocks = [
        GRAIN,
        tail_blocks,
        assigned_blocks,
        deferred_blocks,
        reserved_blocks,
    ]
    .into_iter()
    .try_fold(total, |left, amount| left.checked_sub(amount))
    .ok_or(Error::Corrupt("espace physique du pool excédé"))?;
    Ok(PoolSpaceStats {
        total_blocks: total,
        metadata_blocks: GRAIN,
        tail_blocks,
        assigned_blocks,
        deferred_blocks,
        reserved_blocks,
        available_blocks,
        shared_blocks: 0,
        pending_volumes: active.count(),
    })
}

/// Lecture hors ligne des seuls catalogues. Les réservations en RAM sont absentes.
pub fn inspect_space<D: Device>(dev: &D) -> Result<PoolSpaceStats> {
    let (older, head, _) = checked_catalogs(dev)?;
    space(dev.blocks(), &older, &head, &Pending::default())
}

/// Vérifie aussi le contenu des LV prêts. Les deux générations sont lues en
/// RO ; une erreur interdit de présenter des compteurs de fichiers incomplets.
pub fn inspect_usage<D: Device>(
    dev: &D,
) -> Result<(
    PoolSpaceStats,
    Vec<(VolumeInfo, Option<crate::VolumeAccounting>)>,
)> {
    let (older, head, _) = checked_catalogs(dev)?;
    let stats = space(dev.blocks(), &older, &head, &Pending::default())?;
    let mut volumes = Vec::new();
    volumes
        .try_reserve_exact(head.entries.len())
        .map_err(|_| Error::NoMemory)?;
    for entry in &head.entries {
        let accounting = if entry.kind == Type::Ready {
            let fs = crate::Engine::mount(
                super::ReadVolume { disk: dev, entry },
                crate::Ids {
                    lv: entry.id,
                    fs: entry.fs,
                },
                crate::Mode::ReadOnly,
            )?;
            Some(fs.accounting()?)
        } else {
            None
        };
        volumes.push((VolumeInfo::of(entry), accounting));
    }
    Ok((stats, volumes))
}

/// Réservation exclusive liée à une instance de montage. Sa destruction annule
/// l'admission. Le ticket peut être confié à un autre thread pour la publication.
#[must_use = "la destruction du ticket annule la réservation"]
pub struct VolumeReservation {
    ledger: Arc<Mutex<Pending>>,
    slot: Option<usize>,
}

impl VolumeReservation {
    fn take(&mut self) -> Option<Entry> {
        let slot = self.slot.take()?;
        // Aucune E/S ni code appelant ne s'exécute dans cette section critique.
        // Une annulation doit aussi restituer le slot après une panique d'un pair.
        self.ledger
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .entries[slot]
            .take()
    }
}

impl Drop for VolumeReservation {
    fn drop(&mut self) {
        self.take();
    }
}

impl<D: Device> Pool<D> {
    pub fn into_device(self) -> D {
        self.disk
    }

    /// Dernière tête acquittée par cette instance ; après une E/S incertaine,
    /// seul un nouvel import peut qualifier la génération réellement accessible.
    pub fn generation(&self) -> u64 {
        self.head.generation
    }

    pub fn volumes(&self) -> impl Iterator<Item = VolumeInfo> + '_ {
        self.head.entries.iter().map(VolumeInfo::of)
    }

    pub fn space_stats(&self) -> Result<PoolSpaceStats> {
        self.healthy()?;
        let pending = self.pending.lock().map_err(|_| Error::Unavailable)?;
        space(self.disk.blocks(), &self.older, &self.head, &pending)
    }

    pub fn reserve_volume(
        &self,
        owner: Uid,
        id: Uid,
        fs: Option<Uid>,
        grains: u64,
    ) -> Result<VolumeReservation> {
        self.writable()?;
        if owner == [0; 16] || id == [0; 16] || fs == Some([0; 16]) || grains == 0 {
            return Err(Error::Invalid("identité ou capacité de réservation"));
        }
        let mut pending = self.pending.lock().map_err(|_| Error::Unavailable)?;
        let committed_and_pending = self
            .head
            .entries
            .iter()
            .chain(pending.entries.iter().flatten());
        if committed_and_pending
            .clone()
            .any(|e| e.id == id || fs == Some(e.fs))
        {
            return Err(Error::Exists);
        }
        // Les identités figurant encore dans le catalogue précédent doivent
        // sortir de la fenêtre de récupération avant d'être réattribuées.
        if self
            .older
            .entries
            .iter()
            .any(|e| e.id == id || fs == Some(e.fs))
        {
            return Err(Error::Busy);
        }
        if committed_and_pending.clone().count() >= 16 {
            return Err(Error::Limit("nombre de LV du catalogue"));
        }
        let charged = committed_and_pending
            .clone()
            .filter(|e| e.owner == owner)
            .try_fold(grains, |n, e| n.checked_add(e.grains))
            .ok_or(Error::QuotaExceeded)?;
        if charged > DOMAIN_QUOTA {
            return Err(Error::QuotaExceeded);
        }

        let mut segments: Vec<Segment> = Vec::new();
        segments
            .try_reserve_exact(grains as usize)
            .map_err(|_| Error::NoMemory)?;
        let mut assigned = 0;
        // Dès le quatrième grain libre au maximum la recherche termine. Tout
        // autre grain examiné appartient à un catalogue borné ou à un ticket.
        for physical in 1..self.disk.blocks() / GRAIN {
            if self
                .older
                .reachable()
                .chain(
                    committed_and_pending
                        .clone()
                        .flat_map(|e| e.segments.iter()),
                )
                .any(|s| covers(s, physical))
            {
                continue;
            }
            if let Some(last) = segments
                .last_mut()
                .filter(|s| s.physical + s.count == physical)
            {
                last.count += 1;
            } else {
                segments.push(Segment {
                    logical: assigned,
                    physical,
                    count: 1,
                });
            }
            assigned += 1;
            if assigned == grains {
                break;
            }
        }
        if assigned != grains {
            return Err(Error::PoolFull);
        }
        let slot = pending
            .entries
            .iter()
            .position(Option::is_none)
            .ok_or(Error::Limit("tickets du pool"))?;
        pending.entries[slot] = Some(Entry {
            id,
            owner,
            kind: if fs.is_some() {
                Type::Preparing
            } else {
                Type::Raw
            },
            fs: fs.unwrap_or([0; 16]),
            grains,
            segments,
        });
        Ok(VolumeReservation {
            ledger: Arc::clone(&self.pending),
            slot: Some(slot),
        })
    }

    /// Consomme un ticket même en cas d'échec. L'emprunt exclusif du pool
    /// sérialise les publications, sans garder le mutex du registre pendant les E/S.
    pub fn commit_volume(&mut self, mut ticket: VolumeReservation) -> Result<()> {
        self.writable()?;
        if !Arc::ptr_eq(&ticket.ledger, &self.pending) {
            return Err(Error::Invalid("ticket d’une autre incarnation du pool"));
        }
        let entry = ticket
            .take()
            .ok_or(Error::Invalid("ticket déjà consommé"))?;
        let id = entry.id;
        let mut entries = self.head.entries.clone();
        entries.try_reserve_exact(1).map_err(|_| Error::NoMemory)?;
        entries.push(entry);
        self.publish(entries, Some(id))
    }
}
