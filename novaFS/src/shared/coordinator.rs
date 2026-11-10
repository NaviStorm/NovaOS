//! La file FIFO utilise les mêmes règles d'admission que les volumes P4. Un
//! ticket administratif peut invalider un plan préparé pour une vue disparue.
use super::{PreparedContent, SharedPool};
use crate::pool::coordinator::{Action, Kind, Queue};
use crate::{Device, Error, Result};
use std::sync::{Mutex, RwLock, TryLockError};

pub struct SharedCoordinator<D: Device> {
    pool: RwLock<SharedPool<D>>,
    queue: Mutex<Queue>,
}

impl<D: Device> SharedCoordinator<D> {
    pub fn mount(pool: SharedPool<D>) -> Result<Self> {
        pool.healthy()?;
        Ok(Self {
            pool: RwLock::new(pool),
            queue: Mutex::new(Queue::new()?),
        })
    }

    fn admit(&self, view: u64, kind: Kind) -> Result<Ticket<'_, D>> {
        // L'admission ne retient jamais le verrou de publication. Un ticket
        // admis pendant F1/F2 revérifie la santé sous verrou à l'exécution.
        match self.pool.try_read() {
            Ok(pool) => {
                pool.healthy()?;
            }
            Err(TryLockError::WouldBlock) => {}
            Err(TryLockError::Poisoned(_)) => return Err(Error::Unavailable),
        }
        let id = self
            .queue
            .lock()
            .map_err(|_| Error::Unavailable)?
            .admit(view, kind)?;
        Ok(Ticket {
            owner: self,
            view,
            id,
            active: true,
        })
    }

    pub fn request_read(&self, view: u64) -> Result<SharedRead<'_, D>> {
        if view == 0 {
            return Err(Error::Invalid("identifiant de vue nul"));
        }
        Ok(SharedRead(self.admit(view, Kind::Read)?))
    }

    pub fn request_write(&self, view: u64) -> Result<SharedWrite<'_, D>> {
        if view == 0 {
            return Err(Error::Invalid("identifiant de vue nul"));
        }
        Ok(SharedWrite {
            plan: None,
            ticket: self.admit(view, Kind::Write)?,
        })
    }

    /// Le numéro zéro est réservé aux changements de catalogue. Une
    /// préparation sur une autre vue peut commencer derrière ce ticket.
    pub fn request_admin(&self) -> Result<SharedAdmin<'_, D>> {
        Ok(SharedAdmin(self.admit(0, Kind::Write)?))
    }

    pub fn into_pool(self) -> Result<SharedPool<D>> {
        self.pool.into_inner().map_err(|_| Error::Unavailable)
    }
}

struct Ticket<'a, D: Device> {
    owner: &'a SharedCoordinator<D>,
    view: u64,
    id: u64,
    active: bool,
}

impl<D: Device> Ticket<'_, D> {
    fn gate(&mut self, action: Action) -> Result<()> {
        if !self.active {
            return Err(Error::Invalid("ticket P5 terminé"));
        }
        let result = self
            .owner
            .queue
            .lock()
            .map_err(|_| Error::Unavailable)
            .and_then(|mut q| q.gate(self.id, self.view, action));
        if result.as_ref().is_err_and(|e| !matches!(e, Error::Busy)) {
            self.finish();
        }
        result
    }

    fn finish(&mut self) {
        if self.active {
            self.owner
                .queue
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .finish(self.id);
            self.active = false;
        }
    }

    fn publish<T>(&mut self, f: impl FnOnce(&mut SharedPool<D>) -> Result<T>) -> Result<T> {
        self.gate(Action::Publish)?;
        let owner = self.owner;
        let _done = Finished(self);
        let mut pool = owner.pool.write().map_err(|_| Error::Unavailable)?;
        pool.healthy()?;
        f(&mut pool)
    }
}

impl<D: Device> Drop for Ticket<'_, D> {
    fn drop(&mut self) {
        self.finish();
    }
}

// Libérer le rang même si une fermeture administrative interrompt la pile.
struct Finished<'r, 'c, D: Device>(&'r mut Ticket<'c, D>);

impl<D: Device> Drop for Finished<'_, '_, D> {
    fn drop(&mut self) {
        self.0.finish();
    }
}

#[must_use]
pub struct SharedRead<'a, D: Device>(Ticket<'a, D>);

impl<D: Device> SharedRead<'_, D> {
    pub fn read(&mut self, path: &str, offset: u64, length: usize) -> Result<Vec<u8>> {
        self.0.gate(Action::Read)?;
        let owner = self.0.owner;
        let view = self.0.view;
        let _done = Finished(&mut self.0);
        owner
            .pool
            .read()
            .map_err(|_| Error::Unavailable)?
            .read_file(view, path, offset, length)
    }

    pub fn cancel(self) {}
}

#[must_use]
pub struct SharedWrite<'a, D: Device> {
    // Libérer la mémoire des pages privées avant le retrait du ticket.
    plan: Option<PreparedContent>,
    ticket: Ticket<'a, D>,
}

impl<D: Device> SharedWrite<'_, D> {
    pub fn prepare(&mut self, path: &str, offset: u64, bytes: &[u8]) -> Result<()> {
        if self.plan.is_some() {
            return Err(Error::Invalid("ticket P5 déjà préparé"));
        }
        self.ticket.gate(Action::Prepare)?;
        let result = self
            .ticket
            .owner
            .pool
            .read()
            .map_err(|_| Error::Unavailable)
            .and_then(|pool| pool.plan_write(self.ticket.view, path, offset, bytes));
        match result {
            Ok(plan) => {
                self.plan = Some(plan);
                Ok(())
            }
            Err(e) => {
                self.ticket.finish();
                Err(e)
            }
        }
    }

    /// Un refus de la file conserve le rang et le plan. Un plan devenu caduc
    /// après une publication administrative doit faire l'objet d'une nouvelle demande.
    pub fn commit(&mut self) -> Result<()> {
        if self.plan.is_none() {
            return Err(Error::Invalid("ticket P5 sans plan"));
        }
        let result = self.ticket.publish(|pool| {
            let plan = self.plan.take().expect("plan validé avant publication");
            pool.commit_prepared(plan)
        });
        if !self.ticket.active {
            self.plan.take();
        }
        result
    }

    pub fn cancel(self) {}
}

#[must_use]
pub struct SharedAdmin<'a, D: Device>(Ticket<'a, D>);

impl<D: Device> SharedAdmin<'_, D> {
    /// La fermeture est exécutée une fois, en tête, sous le verrou exclusif.
    /// En cas de Busy à l'admission, présenter à nouveau l'opération.
    pub fn publish<T>(&mut self, f: impl FnOnce(&mut SharedPool<D>) -> Result<T>) -> Result<T> {
        self.0.publish(f)
    }

    pub fn cancel(self) {}
}
