//! Témoins du seul essai à deux incarnations. Les anciennes adresses sont
//! comparées comme des identifiants. L'accès CPU au contenu exige toujours
//! une nouvelle allocation propriétaire du cadre.
use super::{Engine, Error, Lease, Memory, PAGE, Result, log};
use crate::hardware::block::io::Frame;
use core::sync::atomic::{Ordering, fence};

const SLOTS: usize = 6;
const WINDOW: u64 = 3 * crate::sched::HZ as u64;

pub(super) struct Probe {
    old: Option<Lease>,
    physical: [u64; SLOTS],
    owners: [Option<Frame>; SLOTS],
    reopened: bool,
}

impl Probe {
    pub(super) fn new() -> Self {
        Self {
            old: None,
            physical: [0; SLOTS],
            owners: core::array::from_fn(|_| None),
            reopened: false,
        }
    }

    pub(super) fn first(&self) -> bool {
        self.old.is_none()
    }

    pub(super) fn pages(&self) -> usize {
        self.owners.iter().flatten().count()
    }

    pub(super) fn deny_cpu(&self, frames: &mut [u64; 24]) -> Option<()> {
        for owner in self.owners.iter().flatten() {
            if frames.contains(&owner.pa) {
                return None;
            }
            *frames.iter_mut().find(|pa| **pa == 0)? = owner.pa;
        }
        Some(())
    }

    pub(super) fn capture(&mut self, engine: &Engine, lease: Lease, mem: &Memory) -> Result<()> {
        if !self.first() || mem.epoch != lease.epoch() {
            return Err(Error::Denied);
        }
        for slot in 0..SLOTS {
            let pa = engine
                .nvme_page_pa(lease, slot as u8)
                .ok_or(Error::Denied)?;
            if pa == 0 || self.physical[..slot].contains(&pa) || mem.cpu[slot] == 0 {
                return Err(Error::Denied);
            }
            self.physical[slot] = pa;
        }
        self.old = Some(lease);
        Ok(())
    }

    /// Appelé sous la garde qui couvrait le retrait et la restitution. Les
    /// six cadres, rendus à l'allocateur physique, sont réalloués et épinglés
    /// au nom du superviseur. Aucune IOVA ne lui est accordée.
    pub(super) fn reallocate(&mut self) -> Result<()> {
        let lease = self.old.ok_or(Error::Denied)?;
        for owner in &mut self.owners {
            *owner = Some(Frame::allocate().map_err(|_| Error::Memory)?);
        }
        // Vérifier les six identités et les contenus initialement nuls avant
        // de marquer la moindre page. Aucune hypothèse sur l'ordre d'allocation.
        for slot in 0..SLOTS {
            let frame = self.owner(slot)?;
            if !matches(frame, 0) {
                return Err(Error::Quarantine);
            }
        }
        for slot in 0..SLOTS {
            let frame = self.owner(slot)?;
            let byte = 0xa0 + slot as u8;
            let word = u64::from_ne_bytes([byte; 8]);
            for offset in (0..PAGE).step_by(8) {
                frame.mem.w64(offset, word);
            }
            log(format_args!(
                "NVME P3 cadre epoque={} slot={slot} iova={:#x} gpa={:#x} motif={byte:#x}",
                lease.epoch(),
                lease.iova(slot as u8),
                self.physical[slot]
            ));
        }
        fence(Ordering::SeqCst);
        self.intact()
    }

    fn owner(&self, slot: usize) -> Result<&Frame> {
        self.owners
            .iter()
            .flatten()
            .find(|owner| owner.pa == self.physical[slot])
            .ok_or(Error::Quarantine)
    }

    fn intact(&self) -> Result<()> {
        fence(Ordering::SeqCst);
        for slot in 0..SLOTS {
            if !matches(self.owner(slot)?, 0xa0 + slot as u8) {
                return Err(Error::Quarantine);
            }
        }
        Ok(())
    }

    pub(super) fn reopen(&mut self, engine: &Engine, new: Lease, mem: &Memory) -> Result<()> {
        let old = self.old.ok_or(Error::Denied)?;
        if self.reopened
            || new.owner() == old.owner()
            || old.epoch().checked_add(1) != Some(new.epoch())
            || mem.cpu[5] != 0
            || mem.iova[5] != 0
            || engine.active(old)
            || (0..SLOTS as u8).any(|s| engine.mapped_iova(new, old.iova(s)) != Some(false))
            || (0..5).any(|slot| {
                engine
                    .nvme_page_pa(new, slot)
                    .is_none_or(|pa| self.physical.contains(&pa))
            })
        {
            return Err(Error::Quarantine);
        }
        self.intact()?;
        self.reopened = true;
        log(format_args!(
            "NVME P3 nouvelle epoque={} ancienne={} ancien_pid={} pid={} anciennes_iova_absentes=6 cadres_distincts=5",
            new.epoch(),
            old.epoch(),
            old.owner(),
            new.owner()
        ));
        Ok(())
    }

    /// Rendez-vous QMP uniquement entre deux sessions, puis après l'arrêt
    /// confirmé du second contrôleur. Aucun acteur ne dépend d'une réponse
    /// du courtier pendant ces fenêtres d'observation.
    pub(super) fn observe(&self, second: bool) -> Result<()> {
        if self.reopened != second {
            return Err(Error::Denied);
        }
        self.intact()?;
        log(format_args!(
            "{}",
            if second {
                "NVME P3 lectures achevees avant retrait"
            } else {
                "NVME P3 entre incarnations temoins=6"
            }
        ));
        crate::sched::sleep(WINDOW);
        self.intact()
    }

    pub(super) fn finish(&mut self) -> Result<()> {
        if !self.reopened {
            return Err(Error::Denied);
        }
        self.intact()?;
        log(format_args!("NVME P3 temoins intacts octets=24576"));
        crate::sched::sleep(WINDOW); // contrelecture QMP avant la libération
        self.intact()?;
        let before = crate::memory::pinned_dma_frames();
        for owner in &mut self.owners {
            let frame = owner.take().ok_or(Error::Quarantine)?;
            // Sûreté : propriété issue de la nouvelle allocation, six GPA
            // distinctes vérifiées, jamais mappées dans le nouveau domaine.
            unsafe { crate::memory::release_dma(frame.pa) };
        }
        if before.checked_sub(SLOTS as u64) != Some(crate::memory::pinned_dma_frames()) {
            return Err(Error::Quarantine);
        }
        log(format_args!("NVME P3 temoins liberes cadres=6"));
        Ok(())
    }
}

fn matches(frame: &Frame, byte: u8) -> bool {
    let word = u64::from_ne_bytes([byte; 8]);
    (0..PAGE)
        .step_by(8)
        .all(|offset| frame.mem.r64(offset) == word)
}
