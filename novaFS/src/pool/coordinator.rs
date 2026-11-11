//! Admission FIFO et publication exclusive sur un pool de géométrie fixe.
use super::catalog::Entry;
use super::{Pool, Uid};
use crate::engine::Plan;
use crate::sync::{Arc, Mutex, RwLock};
use crate::{Block, CacheStats, Device, Engine, Error, Ids, Mode, Ref, Result, VolumePolicy};
use alloc::collections::VecDeque;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU8, Ordering};

const QUEUE_LIMIT: usize = 16;
const PLAN_LIMIT: usize = 2;
const CACHE_BUDGET: usize = 1024 * 1024;
const MAX_POOL_CACHE: usize = 2 * 1024 * 1024;

// Aucune vue n'est exportée. Seul le commit en tête invoque write/flush
// après le montage. Le verrou du PD permet plusieurs lectures positionnelles.
struct Volume<D> {
    disk: Arc<RwLock<D>>,
    entry: Entry,
}

impl<D: Device> Device for Volume<D> {
    fn blocks(&self) -> u64 {
        self.entry.grains * super::GRAIN
    }
    fn logical_quota_bytes(&self) -> Option<u64> {
        Some(self.blocks() * 4096)
    }
    fn read(&self, i: u64) -> Result<Block> {
        self.disk
            .read()
            .map_err(|_| Error::Unavailable)?
            .read(self.entry.physical(i)?)
    }
    fn write(&mut self, i: u64, data: &Block) -> Result<()> {
        self.disk
            .write()
            .map_err(|_| Error::Unavailable)?
            .write(self.entry.physical(i)?, data)
    }
    fn flush(&mut self) -> Result<()> {
        self.disk.write().map_err(|_| Error::Unavailable)?.flush()
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Read,
    Write,
}

struct Item {
    id: u64,
    volume: u64,
    kind: Kind,
}

pub(crate) struct Queue {
    order: VecDeque<Item>,
    next: u64,
    publishing: Option<u64>,
}

impl Queue {
    pub(crate) fn new() -> Result<Self> {
        let mut order = VecDeque::new();
        order
            .try_reserve_exact(QUEUE_LIMIT)
            .map_err(|_| Error::NoMemory)?;
        Ok(Self {
            order,
            next: 1,
            publishing: None,
        })
    }
    pub(crate) fn admit(&mut self, volume: u64, kind: Kind) -> Result<u64> {
        if self.order.len() == QUEUE_LIMIT {
            return Err(Error::Busy);
        }
        let id = self.next;
        self.next = id
            .checked_add(1)
            .ok_or(Error::Limit("numéros de requêtes épuisés"))?;
        self.order.push_back(Item { id, volume, kind });
        Ok(id)
    }
    pub(crate) fn gate(&mut self, id: u64, volume: u64, action: Action) -> Result<()> {
        if self.publishing.is_some() {
            return Err(Error::Busy);
        }
        if action == Action::Publish {
            if self.order.front().map(|item| item.id) != Some(id) {
                return Err(Error::Busy);
            }
            self.publishing = Some(id);
        } else {
            let mut writers = 0;
            for earlier in self.order.iter().take_while(|item| item.id != id) {
                if earlier.kind == Kind::Write {
                    writers += 1;
                    if action == Action::Read || earlier.volume == volume || writers >= PLAN_LIMIT {
                        return Err(Error::Busy);
                    }
                }
            }
        }
        Ok(())
    }

    pub(crate) fn finish(&mut self, id: u64) {
        self.order.retain(|item| item.id != id);
        if self.publishing == Some(id) {
            self.publishing = None;
        }
    }
}

/// Possède l'unique PD et les moteurs des LV prêts. Les réservations de
/// provisionnement et changements de catalogue nécessitent un nouvel import.
pub struct PoolCoordinator<D: Device> {
    disk: Arc<RwLock<D>>,
    volumes: Vec<(Uid, RwLock<Engine<Volume<D>>>)>,
    queue: Mutex<Queue>,
    // Les lectures et préparations s'exécutent ensemble. F1/ancre/F2 forme
    // une phase exclusive, sans mutex de file tenu pendant les E/S.
    phase: RwLock<()>,
    health: AtomicU8,
}

impl<D: Device> PoolCoordinator<D> {
    pub fn mount(pool: Pool<D>) -> Result<Self> {
        Self::mount_with_cache_budget(pool, CACHE_BUDGET)
    }

    pub fn mount_with_cache_budget(pool: Pool<D>, bytes: usize) -> Result<Self> {
        if bytes > MAX_POOL_CACHE {
            return Err(Error::Limit("cache total du pool supérieur à 2 Mio"));
        }
        let (disk, entries) = pool.into_ready_devices()?;
        let budget = bytes / entries.len().max(1);
        let disk = Arc::new(RwLock::new(disk));
        let mut volumes = Vec::new();
        volumes
            .try_reserve_exact(entries.len())
            .map_err(|_| Error::NoMemory)?;
        for entry in entries {
            let ids = Ids {
                lv: entry.id,
                fs: entry.fs,
            };
            let volume = Volume {
                disk: Arc::clone(&disk),
                entry,
            };
            let fs = Engine::mount(volume, ids, Mode::ReadWrite)?;
            fs.set_cache_budget(budget)?;
            volumes.push((ids.lv, RwLock::new(fs)));
        }
        Ok(Self {
            disk,
            volumes,
            queue: Mutex::new(Queue::new()?),
            phase: RwLock::new(()),
            health: AtomicU8::new(0),
        })
    }

    fn volume(&self, id: Uid) -> Result<usize> {
        self.volumes
            .iter()
            .position(|v| v.0 == id)
            .ok_or(Error::NotFound)
    }

    fn healthy(&self) -> Result<()> {
        match self.health.load(Ordering::Acquire) {
            0 => Ok(()),
            1 => Err(Error::Unavailable),
            _ => Err(Error::UnknownOutcome),
        }
    }

    fn observe<T>(&self, result: Result<T>) -> Result<T> {
        let fault = match &result {
            Err(Error::Publication { .. } | Error::UnknownOutcome) => 2,
            Err(Error::Corrupt(_) | Error::Io(_) | Error::Unavailable) => 1,
            _ => 0,
        };
        if fault != 0 {
            self.health.fetch_max(fault, Ordering::AcqRel);
        }
        let value = result?;
        self.healthy()?;
        Ok(value)
    }

    fn admit(&self, lv: Uid, kind: Kind) -> Result<Ticket<'_, D>> {
        self.healthy()?;
        let volume = self.volume(lv)?;
        let mut q = self.queue.lock().map_err(|_| Error::Unavailable)?;
        let id = q.admit(volume as u64, kind)?;
        Ok(Ticket {
            coordinator: self,
            volume,
            id,
            active: true,
        })
    }

    pub fn request_read(&self, lv: Uid) -> Result<ReadRequest<'_, D>> {
        Ok(ReadRequest(self.admit(lv, Kind::Read)?))
    }

    pub fn request_write(&self, lv: Uid) -> Result<WriteRequest<'_, D>> {
        Ok(WriteRequest {
            ticket: self.admit(lv, Kind::Write)?,
            prepared: None,
        })
    }

    pub fn set_volume_policy(&mut self, lv: Uid, policy: VolumePolicy) -> Result<()> {
        self.healthy()?;
        if !self
            .queue
            .get_mut()
            .map_err(|_| Error::Unavailable)?
            .order
            .is_empty()
        {
            return Err(Error::Busy);
        }
        let index = self.volume(lv)?;
        self.volumes[index]
            .1
            .get_mut()
            .map_err(|_| Error::Unavailable)?
            .set_volume_policy(policy)
    }

    pub fn reclaim_caches(&self) -> Result<usize> {
        let mut pages = 0;
        for (_, fs) in &self.volumes {
            pages += fs.read().map_err(|_| Error::Unavailable)?.reclaim_cache();
        }
        Ok(pages)
    }

    pub fn cache_stats(&self, lv: Uid) -> Result<CacheStats> {
        Ok(self.volumes[self.volume(lv)?]
            .1
            .read()
            .map_err(|_| Error::Unavailable)?
            .cache_stats())
    }

    /// Rend le périphérique même après un résultat de publication inconnu.
    /// Le prochain écrivain doit réimporter le pool et vérifier les LV.
    pub fn into_device(self) -> Result<D> {
        drop(self.volumes);
        Arc::try_unwrap(self.disk)
            .map_err(|_| Error::Busy)?
            .into_inner()
            .map_err(|_| Error::Unavailable)
    }
}

struct Ticket<'a, D: Device> {
    coordinator: &'a PoolCoordinator<D>,
    volume: usize,
    id: u64,
    active: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Action {
    Read,
    Prepare,
    Publish,
}

impl<D: Device> Ticket<'_, D> {
    fn gate(&self, action: Action) -> Result<()> {
        if !self.active {
            return Err(Error::Invalid("requête terminée"));
        }
        self.coordinator.healthy()?;
        let mut q = self
            .coordinator
            .queue
            .lock()
            .map_err(|_| Error::Unavailable)?;
        q.gate(self.id, self.volume as u64, action)
    }

    fn finish(&mut self) {
        if self.active {
            let mut q = self
                .coordinator
                .queue
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            q.finish(self.id);
            self.active = false;
        }
    }
}

impl<D: Device> Drop for Ticket<'_, D> {
    fn drop(&mut self) {
        self.finish();
    }
}

/// Ticket à usage unique ; Busy conserve le rang. Drop annule le ticket.
#[must_use]
pub struct ReadRequest<'a, D: Device>(Ticket<'a, D>);

impl<D: Device> ReadRequest<'_, D> {
    fn execute<T>(&mut self, f: impl FnOnce(&Engine<Volume<D>>) -> Result<T>) -> Result<T> {
        if let Err(e) = self.0.gate(Action::Read) {
            if !matches!(e, Error::Busy) {
                self.0.finish();
            }
            return Err(e);
        }
        let c = self.0.coordinator;
        let result = (|| {
            let _phase = c.phase.read().map_err(|_| Error::Unavailable)?;
            c.healthy()?;
            let fs = c.volumes[self.0.volume]
                .1
                .read()
                .map_err(|_| Error::Unavailable)?;
            c.observe(f(&fs))
        })();
        self.0.finish();
        result
    }

    pub fn read(&mut self, path: &str, offset: u64, limit: usize) -> Result<Vec<u8>> {
        self.execute(|fs| fs.read(path, offset, limit))
    }

    pub fn verify(&mut self) -> Result<()> {
        self.execute(|fs| fs.verify())
    }
    pub fn cancel(self) {}
}

/// Le plan est privé du ticket. Les tickets en attente sans plan ne
/// conservent aucune copie du chemin ou des données de la demande.
#[must_use]
pub struct WriteRequest<'a, D: Device> {
    // L'ordre de destruction libère le plan avant de rendre le rang FIFO.
    prepared: Option<(Ref, Option<Plan>)>,
    ticket: Ticket<'a, D>,
}

impl<D: Device> WriteRequest<'_, D> {
    pub fn prepare(&mut self, path: &str, offset: u64, data: &[u8]) -> Result<()> {
        if self.prepared.is_some() {
            return Err(Error::Invalid("requête déjà préparée"));
        }
        if let Err(e) = self.ticket.gate(Action::Prepare) {
            if !matches!(e, Error::Busy) {
                self.ticket.finish();
            }
            return Err(e);
        }
        let c = self.ticket.coordinator;
        let result = (|| {
            let _phase = c.phase.read().map_err(|_| Error::Unavailable)?;
            c.healthy()?;
            let mut fs = c.volumes[self.ticket.volume]
                .1
                .write()
                .map_err(|_| Error::Unavailable)?;
            c.observe(fs.prepare_write(path, offset, data).map(|p| p.detach()))
        })();
        match result {
            Ok(plan) => {
                self.prepared = Some(plan);
                Ok(())
            }
            Err(e) => {
                self.ticket.finish();
                Err(e)
            }
        }
    }

    pub fn commit(&mut self) -> Result<Ref> {
        if self.prepared.is_none() {
            return Err(Error::Invalid("requête non préparée"));
        }
        if let Err(e) = self.ticket.gate(Action::Publish) {
            if !matches!(e, Error::Busy) {
                self.prepared.take();
                self.ticket.finish();
            }
            return Err(e);
        }
        let (base, plan) = self.prepared.take().expect("plan vérifié avant admission");
        let c = self.ticket.coordinator;
        let result = (|| {
            let _phase = c.phase.write().map_err(|_| Error::Unavailable)?;
            c.healthy()?;
            let mut fs = c.volumes[self.ticket.volume]
                .1
                .write()
                .map_err(|_| Error::Unavailable)?;
            let result = fs.commit_ticket(base, plan);
            if result.is_err() {
                // Un Device peut refuser une écriture avec NoSpace ou NoMemory,
                // même après un effet partiel. C'est la phase atteinte par le
                // moteur qui impose le gel, indépendamment du nom de l'erreur.
                let _ = c.observe(fs.check_health());
            }
            c.observe(result)
        })();
        self.ticket.finish();
        result
    }

    pub fn cancel(self) {}
}
