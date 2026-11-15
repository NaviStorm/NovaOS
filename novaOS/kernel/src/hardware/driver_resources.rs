//! Contrat interne du superviseur pour les futurs pilotes. Les accès restent
//! médiés : le client ne reçoit ni adresse physique, ni PTE utilisateur, ni vecteur.
//! Un seul courtier permanent possède ses ressources. Son intégrateur sérialise les
//! opérations, y compris les notifications et la révocation, et fournit le vrai PID.
//! La phase 1 instancie uniquement le backend synthétique des tests.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Error {
    Denied,
    Invalid,
    Busy,
    Exhausted,
    Pending,
    Quarantined,
}

type Result<T> = core::result::Result<T, Error>;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct Rights(u8);

impl Rights {
    pub(crate) const READ: Self = Self(1);
    pub(crate) const WRITE: Self = Self(2);
    pub(crate) const IRQ: Self = Self(4);

    pub(crate) const fn with(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    fn includes(self, requested: Self) -> bool {
        requested.0 != 0 && requested.0 & !self.0 == 0
    }
}

/// Une sous-région de registres dont chaque accès de cette largeur est autorisable.
/// Une BAR brute, le bus mastering, les bases de file/DMA ou la table MSI-X
/// ne constituent jamais une telle autorisation.
#[derive(Clone, Copy)]
pub(crate) struct Window {
    pub(crate) base: u64,
    pub(crate) bytes: u64,
    pub(crate) width: u8,
    pub(crate) attribute: DeviceMemory,
}

/// Types Device admis par la médiation. Une politique ne peut demander le cache RAM.
#[derive(Clone, Copy)]
pub(crate) enum DeviceMemory {
    X86Uncacheable,
    ArmDevice,
}

impl DeviceMemory {
    const fn appropriate(self) -> bool {
        match self {
            Self::X86Uncacheable => cfg!(target_arch = "x86_64"),
            Self::ArmDevice => cfg!(target_arch = "aarch64"),
        }
    }
}

impl Window {
    fn end(self) -> Option<u64> {
        let width = u64::from(self.width);
        if !self.attribute.appropriate()
            || !matches!(width, 1 | 2 | 4 | 8)
            || self.base == 0
            || self.bytes == 0
            || self.base % width != 0
            || self.bytes % width != 0
        {
            return None;
        }
        self.base.checked_add(self.bytes)
    }

    fn address(self, offset: u64, width: u8) -> Result<u64> {
        if width != self.width
            || offset % u64::from(self.width) != 0
            || offset
                .checked_add(u64::from(width))
                .is_none_or(|end| end > self.bytes)
        {
            return Err(Error::Invalid);
        }
        self.base.checked_add(offset).ok_or(Error::Invalid)
    }
}

/// Décision issue d'une politique superviseur portant sur une incarnation du pilote.
/// La source d'IRQ est un identifiant de route exclusif normalisé par la plateforme.
#[derive(Clone, Copy)]
pub(crate) struct Policy {
    pub(crate) window: Option<Window>,
    pub(crate) irq: Option<u32>,
    pub(crate) rights: Rights,
}

impl Policy {
    fn valid(self) -> bool {
        self.rights.0 != 0
            && self.rights.0 & !7 == 0
            && self.window.is_some() == (self.rights.0 & 3 != 0)
            && self.window.is_none_or(|window| window.end().is_some())
            && self.irq.is_some() == self.rights.includes(Rights::IRQ)
    }

    fn overlaps(self, other: Self) -> bool {
        if self.irq.is_some() && self.irq == other.irq {
            return true;
        }
        match (self.window, other.window) {
            (Some(a), Some(b)) => {
                // Les politiques conservées ont passé valid() avant la réservation.
                a.base < b.end().unwrap() && b.base < a.end().unwrap()
            }
            _ => false,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct Capability {
    slot: usize,
    epoch: u64,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct Notice {
    pub(crate) owner: u64,
    pub(crate) capability: Capability,
    pub(crate) sequence: u64,
}

/// Backend privilégié exclusif. Les opérations sont bornées ; une erreur après
/// admission rend le retrait obligatoire. Une implémentation physique devra établir
/// les ressources depuis une politique indépendante de l'inventaire PCI, exclure RAM
/// et zones réservées, assurer Device/UC, NX et accès superviseur, et refuser tout DMA
/// sans domaine qualifié. La phase 1 ne propose aucun backend physique générique.
pub(crate) trait Backend {
    /// Consultation pure, aucun effet de bord ni activation en cas de refus.
    fn policy(&self, device: u32, owner: u64) -> Option<Policy>;
    /// Réserve sous cette incarnation, origine d'IRQ masquée et état antérieur purgé.
    fn prepare(
        &mut self,
        device: u32,
        cap: Capability,
        policy: Policy,
        rights: Rights,
    ) -> Result<()>;
    fn read(&mut self, device: u32, address: u64, width: u8) -> Result<u64>;
    fn write(&mut self, device: u32, address: u64, width: u8, value: u64) -> Result<()>;
    fn set_mask(&mut self, device: u32, masked: bool) -> Result<()>;
    /// Vérifie la source de l'interruption. Un événement revendiqué est déjà masqué.
    fn claim_and_mask(&mut self, device: u32) -> Result<bool>;
    /// Acquitte la source et la laisse masquée.
    fn acknowledge(&mut self, device: u32) -> Result<()>;
    /// Reprend le retrait de cette incarnation. Succès seulement après confirmation
    /// du drainage DMA, de l'arrêt, du masquage et de la purge IRQ, du retrait des
    /// traductions et des mappings. Conserve ses ressources lors d'un échec, même
    /// si prepare() n'avait réalisé qu'une partie du travail. Contrat du transport
    /// existant : hardware/block/ownership.rs, Retrait::retirer().
    fn retire(&mut self, device: u32, cap: Capability) -> Result<()>;
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    Masked,
    Armed,
    Pending,
    Quarantined,
}

#[derive(Clone, Copy)]
struct Grant {
    device: u32,
    owner: u64,
    epoch: u64,
    policy: Policy,
    rights: Rights,
    sequence: u64,
    state: State,
}

pub(crate) struct Broker<B, const N: usize> {
    backend: B,
    slots: [Option<Grant>; N],
    epoch: u64,
}

impl<B: Backend, const N: usize> Broker<B, N> {
    pub(crate) const fn new(backend: B) -> Self {
        Self {
            backend,
            slots: [None; N],
            epoch: 0,
        }
    }

    pub(crate) fn grant(&mut self, device: u32, owner: u64, rights: Rights) -> Result<Capability> {
        if owner == 0 {
            return Err(Error::Denied);
        }
        let policy = self.backend.policy(device, owner).ok_or(Error::Denied)?;
        if !policy.valid() || !policy.rights.includes(rights) {
            return Err(Error::Denied);
        }
        if self
            .slots
            .iter()
            .flatten()
            .any(|g| g.device == device || policy.overlaps(g.policy))
        {
            return Err(Error::Busy);
        }
        let slot = self
            .slots
            .iter()
            .position(Option::is_none)
            .ok_or(Error::Exhausted)?;
        self.epoch = self.epoch.checked_add(1).ok_or(Error::Exhausted)?;
        let cap = Capability {
            slot,
            epoch: self.epoch,
        };
        self.slots[slot] = Some(Grant {
            device,
            owner,
            epoch: cap.epoch,
            policy,
            rights,
            sequence: 0,
            state: State::Quarantined,
        });
        // L'emplacement est réservé même si la préparation a été en partie appliquée.
        self.backend
            .prepare(device, cap, policy, rights)
            .map_err(|_| Error::Quarantined)?;
        self.slots[slot].as_mut().unwrap().state = State::Masked;
        Ok(cap)
    }

    fn active(&self, owner: u64, cap: Capability, right: Rights) -> Result<Grant> {
        let grant = self
            .slots
            .get(cap.slot)
            .copied()
            .flatten()
            .ok_or(Error::Denied)?;
        if grant.owner != owner || grant.epoch != cap.epoch || !grant.rights.includes(right) {
            return Err(Error::Denied);
        }
        if grant.state == State::Quarantined {
            return Err(Error::Quarantined);
        }
        Ok(grant)
    }

    fn confirmed<T>(&mut self, cap: Capability, result: Result<T>) -> Result<T> {
        result.map_err(|_| {
            self.slots[cap.slot].as_mut().unwrap().state = State::Quarantined;
            Error::Quarantined
        })
    }

    pub(crate) fn read(
        &mut self,
        owner: u64,
        cap: Capability,
        offset: u64,
        width: u8,
    ) -> Result<u64> {
        let g = self.active(owner, cap, Rights::READ)?;
        let address = g
            .policy
            .window
            .ok_or(Error::Denied)?
            .address(offset, width)?;
        let result = self.backend.read(g.device, address, width);
        self.confirmed(cap, result)
    }

    pub(crate) fn write(
        &mut self,
        owner: u64,
        cap: Capability,
        offset: u64,
        width: u8,
        value: u64,
    ) -> Result<()> {
        let g = self.active(owner, cap, Rights::WRITE)?;
        let address = g
            .policy
            .window
            .ok_or(Error::Denied)?
            .address(offset, width)?;
        if width < 8 && value >> (u32::from(width) * 8) != 0 {
            return Err(Error::Invalid);
        }
        let result = self.backend.write(g.device, address, width, value);
        self.confirmed(cap, result)
    }

    pub(crate) fn set_mask(&mut self, owner: u64, cap: Capability, masked: bool) -> Result<()> {
        let g = self.active(owner, cap, Rights::IRQ)?;
        if g.state == State::Pending {
            return if masked { Ok(()) } else { Err(Error::Pending) };
        }
        let result = self.backend.set_mask(g.device, masked);
        self.confirmed(cap, result)?;
        self.slots[cap.slot].as_mut().unwrap().state =
            if masked { State::Masked } else { State::Armed };
        Ok(())
    }

    /// Le routeur fournit la capacité enregistrée lors de l’installation de la route. Une
    /// occurrence tardive d'une ancienne incarnation ne peut acquitter la nouvelle.
    pub(crate) fn interrupt(&mut self, cap: Capability, source: u32) -> Result<Option<Notice>> {
        let Some(g) = self.slots.get(cap.slot).copied().flatten() else {
            return Ok(None);
        };
        if g.epoch != cap.epoch || g.state != State::Armed || g.policy.irq != Some(source) {
            return Ok(None);
        }
        let result = self.backend.claim_and_mask(g.device);
        if !self.confirmed(cap, result)? {
            return Ok(None);
        }
        let grant = self.slots[cap.slot].as_mut().unwrap();
        let Some(sequence) = grant.sequence.checked_add(1) else {
            grant.state = State::Quarantined;
            return Err(Error::Exhausted);
        };
        grant.sequence = sequence;
        grant.state = State::Pending;
        Ok(Some(Notice {
            owner: g.owner,
            capability: cap,
            sequence,
        }))
    }

    pub(crate) fn ack(&mut self, owner: u64, cap: Capability, sequence: u64) -> Result<()> {
        let g = self.active(owner, cap, Rights::IRQ)?;
        if g.state != State::Pending || g.sequence != sequence {
            return Err(Error::Invalid);
        }
        let result = self.backend.acknowledge(g.device);
        self.confirmed(cap, result)?;
        self.slots[cap.slot].as_mut().unwrap().state = State::Masked;
        Ok(())
    }

    /// Appel interne du superviseur. L'admission est fermée avant la première
    /// tentative de retrait, même si le backend devra poursuivre ultérieurement.
    pub(crate) fn revoke(&mut self, cap: Capability) -> Result<()> {
        let g = self
            .slots
            .get_mut(cap.slot)
            .and_then(Option::as_mut)
            .filter(|g| g.epoch == cap.epoch)
            .ok_or(Error::Denied)?;
        g.state = State::Quarantined;
        self.backend
            .retire(g.device, cap)
            .map_err(|_| Error::Quarantined)?;
        self.slots[cap.slot] = None;
        Ok(())
    }

    pub(crate) fn driver_died(&mut self, owner: u64) -> Result<()> {
        // Fermer toutes les admissions de ce processus avant le premier retrait.
        for g in self.slots.iter_mut().flatten().filter(|g| g.owner == owner) {
            g.state = State::Quarantined;
        }
        self.reap(Some(owner))
    }

    pub(crate) fn retry_retirements(&mut self) -> Result<()> {
        self.reap(None)
    }

    fn reap(&mut self, owner: Option<u64>) -> Result<()> {
        let mut result = Ok(());
        for slot in 0..N {
            if let Some(g) = self.slots[slot]
                && g.state == State::Quarantined
                && owner.is_none_or(|pid| g.owner == pid)
                && self
                    .revoke(Capability {
                        slot,
                        epoch: g.epoch,
                    })
                    .is_err()
            {
                result = Err(Error::Quarantined);
            }
        }
        result
    }
}

#[cfg(test)]
#[path = "driver_resources/tests.rs"]
mod tests;
