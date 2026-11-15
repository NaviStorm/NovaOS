//! Contrat interne préparatoire pour un backend physique Intel VT-d, AMD-Vi ou Arm SMMU.
//! Aucun backend physique n'est enregistré. La table persistante appartient au superviseur.
//! Les futurs appels devront être sérialisés, y compris retrait et mort du propriétaire.
//! Les domaines VirtIO et Q35 qualifiés ont leur propre chemin dans block/ownership.rs.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Group {
    pub unit: u32,
    /// Identifie tout le groupe d'isolation, alias et fonctions inséparables inclus.
    pub group: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Direction {
    ReadByDevice,
    WrittenByDevice,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Token {
    group: Group,
    owner: u64,
    epoch: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Error {
    Denied,
    Busy,
    Geometry,
    Exhausted,
    Quarantined,
}

type Result<T> = core::result::Result<T, Error>;

/// Chemin de production de la phase 2. Une identité PCI, DMAR, IVRS ou IORT ne
/// fournit pas l'attestation nécessaire à la création d'un domaine physique.
pub(crate) fn physical_domain(_group: Group, _owner: u64) -> Result<Token> {
    Err(Error::Denied)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Page {
    /// Identité d'une allocation détenue par le noyau. Jamais une adresse client.
    pub frame: u64,
    pub iova: u64,
    pub direction: Direction,
}

/// Contrat d'une unité qualifiée. Un échec peut survenir après un effet partiel,
/// sauf pour pin() dont l'échec doit garantir l'absence de nouvel épinglage.
pub(crate) trait Unit {
    /// Décision pure, incluant couverture de tous les alias, IRQ/DMA bypass exclus.
    fn permits_group(&self, group: Group, owner: u64) -> bool;
    /// Associer l'ensemble du groupe fermé, DMA arrêté, sans traduction héritée.
    fn attach_closed(&mut self, token: Token) -> Result<()>;
    /// Établir le propriétaire, les droits, la RAM ordinaire de ce cadre puis
    /// l'épingler atomiquement pour la durée de l'instance.
    fn pin(&mut self, token: Token, page: Page) -> Result<()>;
    /// La traduction ne peut conférer que le sens demandé. Le groupe garde son
    /// admission fermée. Réaliser la maintenance cache avant accès du périphérique.
    fn configure_page(&mut self, token: Token, page: Page) -> Result<()>;
    /// Publier tables et invalidations, achever leur synchronisation, ensuite
    /// seulement autoriser la source. Aucun appel ultérieur n'ajoute de page.
    fn publish_and_open(&mut self, token: Token) -> Result<()>;
    /// Idempotent par époque, même après un attach/configure/publish partiel.
    /// Fermer les soumissions, arrêter la source, drainer les transactions, retirer
    /// mappings et domaines, invalider IOTLB et ATC si présents, attendre leur
    /// acquittement et assurer la cohérence CPU. L'absence de preuve rend Err.
    fn retire_and_confirm(&mut self, token: Token) -> Result<()>;
    /// Appelé exclusivement après confirmation. L'épinglage est unique par cadre.
    fn unpin(&mut self, token: Token, page: Page);
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum State {
    Preparing,
    Running,
    Quarantined,
}

/// Un emplacement par groupe servi ; le constructeur du futur superviseur devra
/// conserver un singleton par groupe d'isolation et inclure ses alias de requester.
pub(crate) struct Domain<B: Unit, const N: usize> {
    unit: B,
    group: Group,
    epoch: u64,
    active: Option<(Token, State)>,
    pinned: [Option<Page>; N],
    used: usize,
}

impl<B: Unit, const N: usize> Domain<B, N> {
    pub(crate) fn new(group: Group, unit: B) -> Self {
        Self {
            unit,
            group,
            epoch: 0,
            active: None,
            pinned: [None; N],
            used: 0,
        }
    }

    pub(crate) fn attach(&mut self, owner: u64) -> Result<Token> {
        if self.active.is_some() {
            return Err(Error::Busy);
        }
        if owner == 0 || !self.unit.permits_group(self.group, owner) {
            return Err(Error::Denied);
        }
        self.epoch = self.epoch.checked_add(1).ok_or(Error::Exhausted)?;
        let token = Token {
            group: self.group,
            owner,
            epoch: self.epoch,
        };
        self.active = Some((token, State::Preparing));
        if self.unit.attach_closed(token).is_err() {
            self.active = Some((token, State::Quarantined));
            return Err(Error::Quarantined);
        }
        Ok(token)
    }

    fn authorize(&self, owner: u64, token: Token) -> Result<State> {
        match self.active {
            Some((current, state)) if current == token && token.owner == owner => Ok(state),
            _ => Err(Error::Denied),
        }
    }

    pub(crate) fn map(&mut self, owner: u64, token: Token, page: Page) -> Result<()> {
        if self.authorize(owner, token)? != State::Preparing {
            return Err(Error::Denied);
        }
        if (page.iova | page.frame) & 4095 != 0
            || page.iova == 0
            || page.frame == 0
            || page.iova.checked_add(4095).is_none()
            || page.frame.checked_add(4095).is_none()
        {
            return Err(Error::Geometry);
        }
        if self.pinned[..self.used]
            .iter()
            .flatten()
            .any(|p| p.frame == page.frame || p.iova == page.iova)
        {
            return Err(Error::Busy);
        }
        let slot = self.pinned.get_mut(self.used).ok_or(Error::Exhausted)?;
        self.unit.pin(token, page)?;
        *slot = Some(page);
        self.used += 1;
        if self.unit.configure_page(token, page).is_err() {
            self.active = Some((token, State::Quarantined));
            return Err(Error::Quarantined);
        }
        Ok(())
    }

    pub(crate) fn publish(&mut self, owner: u64, token: Token) -> Result<()> {
        if self.authorize(owner, token)? != State::Preparing || self.used == 0 {
            return Err(Error::Denied);
        }
        if self.unit.publish_and_open(token).is_err() {
            self.active = Some((token, State::Quarantined));
            return Err(Error::Quarantined);
        }
        self.active = Some((token, State::Running));
        Ok(())
    }

    pub(crate) fn revoke(&mut self, owner: u64, token: Token) -> Result<()> {
        self.authorize(owner, token)?;
        self.retire()
    }

    /// Privé au superviseur. Permet la reprise même si attach_closed a échoué
    /// avant remise du token ou si le pilote n'existe plus.
    pub(crate) fn retire(&mut self) -> Result<()> {
        let (token, _) = self.active.ok_or(Error::Denied)?;
        self.active = Some((token, State::Quarantined));
        self.unit
            .retire_and_confirm(token)
            .map_err(|_| Error::Quarantined)?;
        for page in &mut self.pinned[..self.used] {
            self.unit.unpin(token, page.take().expect("cadre épinglé"));
        }
        self.used = 0;
        self.active = None;
        Ok(())
    }
}

#[cfg(test)]
#[path = "physical_dma/tests.rs"]
mod tests;
