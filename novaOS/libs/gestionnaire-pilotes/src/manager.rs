use crate::{
    Error, Result,
    catalogue::{Catalog, Plan, Target},
    inventory::*,
    lifecycle::*,
    next,
    resources::Use,
};
use State::*;
use alloc::vec::Vec;
#[path = "control.rs"]
mod control;
pub use control::{Delivery, Pending, Selection};
pub const MAX_GROUPS: usize = 32;
pub const MAX_PLANS: usize = 2;
struct SelectionPlan {
    parent: Option<Lease>,
    slot: Slot,
    profile: [u8; 32],
    plan: Plan,
    inventory: u64,
    catalog: u64,
    policy: u64,
    scope: Scope,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LaunchTicket {
    lease: Lease,
    seal: [u8; 32],
}
impl LaunchTicket {
    pub fn seal(&self) -> [u8; 32] {
        self.seal
    }
}
pub struct Group {
    seal: [u8; 32],
    attachment: Attachment,
    state: State,
    revision: u64,
    generation: u64,
    lease: Option<Lease>,
    leaf_epoch: u64,
    grants: crate::rights::Grants,
    roots: u8,
    handles: u8,
    held: Use,
    scope: Scope,
    pages: u128,
    initial_pages: u128,
    unmapped: u128,
    returned: u128,
    stopped: bool,
    last_proof: u64,
    fresh_proof: bool,
    exposed: bool,
    ticket: bool,
    launched: bool,
    prepared: bool,
    backend: bool,
    quiescent: bool,
    removed: bool,
    source_revision: u64,
    policy: u64,
    catalog: u64,
}
impl Group {
    pub fn state(&self) -> State {
        self.state
    }
    pub fn stamp(&self) -> Stamp {
        Stamp {
            attachment: self.attachment,
            revision: self.revision,
            generation: self.generation,
        }
    }
    pub fn lease(&self) -> Option<Lease> {
        self.lease
    }
    pub fn held(&self) -> Use {
        self.held
    }
    fn change(&mut self, state: State, revision: u64) {
        self.state = state;
        self.revision = revision;
    }
}
pub struct Manager<'c, 'p> {
    inventory: Inventory,
    catalog: &'c Catalog<'p>,
    catalog_revision: u64,
    policy_revision: u64,
    groups: Vec<Group>,
    plans: Vec<SelectionPlan>,
    management_epoch: u64,
    pending_changes: u128,
    fabric: crate::ipc::Fabric,
    last_incarnation: u64,
    cap: Use,
    baseline: Use,
    q35: Option<u16>,
    nvme: Option<u64>,
    last_generation: u64,
}
impl<'c, 'p> Manager<'c, 'p> {
    /// Le dernier numéro et les réservations existantes sont injectés par le registre du banc.
    pub fn new(
        boot: u64,
        catalog: &'c Catalog<'p>,
        policy: u64,
        cap: Use,
        baseline: Use,
        q35: Option<u16>,
        last_generation: u64,
    ) -> Result<Self> {
        if policy == 0 || !cap.fits(Use::LAB) || !baseline.fits(cap) {
            return Err(Error::Format);
        }
        Ok(Self {
            inventory: Inventory::new(boot)?,
            catalog,
            catalog_revision: 1,
            policy_revision: policy,
            groups: Vec::new(),
            plans: Vec::new(),
            management_epoch: 1,
            pending_changes: 0,
            fabric: crate::ipc::Fabric::new(boot, [(0, 0); 32])?,
            last_incarnation: 0,
            cap,
            baseline,
            q35,
            nvme: None,
            last_generation,
        })
    }
    pub fn inventory(&self) -> &Inventory {
        &self.inventory
    }
    pub fn notify(&mut self, revision: u64) -> Result<bool> {
        self.inventory.notify(revision)
    }
    pub fn refresh(
        &mut self,
        revision: u64,
        complete: bool,
        input: impl IntoIterator<Item = Node>,
    ) -> Result<()> {
        if self.groups.iter().any(|g| g.revision == u64::MAX) {
            return Err(Error::Exhausted);
        }
        let changed = self.inventory.refresh(revision, complete, input)?;
        self.pending_changes |= changed;
        if complete {
            let mut affected = self.pending_changes;
            loop {
                let before = affected;
                for node in self.inventory.nodes() {
                    if node.requires() & affected != 0 {
                        affected |= node.slot.bit();
                    }
                }
                if before == affected {
                    break;
                }
            }
            for g in &mut self.groups {
                if self.inventory.attachment(g.attachment.slot) != Ok(g.attachment)
                    || (g.scope.members | g.attachment.slot.bit()) & affected != 0
                    || g.lease.is_some() && self.inventory.scope(g.attachment.slot) != Ok(g.scope)
                {
                    g.removed = true;
                    if permits(Some(g.state), Removed) {
                        g.change(Removed, g.revision + 1);
                    }
                }
            }
            self.plans.retain(|p| {
                self.groups
                    .iter()
                    .any(|g| g.attachment.slot == p.slot && g.state == Verified)
            });
            self.pending_changes = 0;
        }
        Ok(())
    }
    pub fn change_catalog(
        &mut self,
        catalog: &'c Catalog<'p>,
        revision: u64,
        policy: u64,
    ) -> Result<()> {
        if revision <= self.catalog_revision || policy < self.policy_revision {
            return Err(Error::Stale);
        }
        self.catalog = catalog;
        self.catalog_revision = revision;
        self.policy_revision = policy;
        Ok(())
    }
    pub fn nvme_lease(&mut self, lease: Option<u64>) -> Result<()> {
        if lease == Some(0) {
            return Err(Error::Format);
        }
        self.nvme = lease;
        Ok(())
    }
    pub fn groups(&self) -> &[Group] {
        &self.groups
    }
    fn index(&self, slot: Slot) -> Result<usize> {
        self.groups
            .iter()
            .position(|g| g.attachment.slot == slot)
            .ok_or(Error::Stale)
    }
    pub fn group(&self, slot: Slot) -> Result<&Group> {
        Ok(&self.groups[self.index(slot)?])
    }
    pub fn discover(&mut self, slot: Slot) -> Result<Stamp> {
        if !self.inventory.ready() {
            return Err(Error::Incomplete);
        }
        let a = self.inventory.attachment(slot)?;
        if self.index(slot).is_ok() {
            return Err(Error::Conflict);
        }
        let g = Group {
            seal: [0; 32],
            attachment: a,
            revision: 1,
            state: Discovered,
            generation: 0,
            lease: None,
            leaf_epoch: 0,
            grants: Default::default(),
            roots: 0,
            handles: 0,
            held: Use::default(),
            scope: Scope {
                members: 0,
                reset_peers: 0,
                dependencies: 0,
            },
            pages: 0,
            initial_pages: 0,
            unmapped: 0,
            returned: 0,
            stopped: false,
            last_proof: 0,
            fresh_proof: false,
            exposed: false,
            ticket: false,
            launched: false,
            prepared: false,
            backend: false,
            quiescent: false,
            removed: false,
            source_revision: 0,
            policy: 0,
            catalog: 0,
        };
        let s = g.stamp();
        crate::push(&mut self.groups, g, MAX_GROUPS)?;
        Ok(s)
    }
    /// Recycle une place définitivement clôturée. Une apparition encore présente
    /// passe par Réadmission et les anciens canaux doivent être collectés.
    pub fn forget(&mut self, s: Stamp) -> Result<()> {
        let i = self.checked(s)?;
        let g = &self.groups[i];
        if !self.inventory.ready()
            || self.inventory.attachment(s.attachment.slot) == Ok(s.attachment)
            || g.state != Returned
            || g.lease.is_some()
            || self.fabric.references(s.attachment)
        {
            return Err(Error::Conflict);
        }
        self.groups.swap_remove(i);
        Ok(())
    }
    fn checked(&self, s: Stamp) -> Result<usize> {
        let i = self.index(s.attachment.slot)?;
        if self.groups[i].stamp() != s {
            return Err(Error::Stale);
        }
        Ok(i)
    }
    fn present(&self, a: Attachment) -> Result<()> {
        if !self.inventory.ready() {
            return Err(Error::Incomplete);
        }
        if self.inventory.attachment(a.slot)? != a {
            return Err(Error::Stale);
        }
        Ok(())
    }
    pub fn select(
        &mut self,
        s: Stamp,
        target: &Target<'_>,
        pin: Option<[u8; 32]>,
        approved_members: u128,
    ) -> Result<Stamp> {
        let i = self.checked(s)?;
        self.present(s.attachment)?;
        if self.groups[i].state != Discovered {
            return Err(Error::Transition);
        }
        if self.plans.len() >= MAX_PLANS {
            return Err(Error::Full);
        }
        let node = self.inventory.node(s.attachment.slot)?;
        if target.revision != self.inventory.revision()
            || target.bus != node.identity
            || target.attachment.as_bytes() != node.slot.label()
        {
            return Err(Error::Stale);
        }
        let (scope, parent) = if node.ownership == Ownership::Parent {
            let mut owner = node;
            while owner.ownership == Ownership::Parent {
                owner = self.inventory.node(owner.parent.ok_or(Error::Denied)?)?;
            }
            let parent = self.group(owner.slot)?.lease.ok_or(Error::Denied)?;
            self.mediated(node.slot, parent)?;
            (
                Scope {
                    members: node.slot.bit(),
                    reset_peers: 0,
                    dependencies: owner.slot.bit(),
                },
                Some(parent),
            )
        } else {
            (self.inventory.scope(node.slot)?, None)
        };
        if scope.members != approved_members {
            return Err(Error::Denied);
        }
        let rev = next(s.revision)?;
        let plan = self.catalog.select(target, pin).map_err(Error::Catalogue)?;
        if let Some(l) = parent {
            let h = plan.hardware();
            if target.profile != Some(l.profile)
                || !plan.dependencies().iter().any(|d| {
                    d.kind == crate::catalogue::Kind::SystemProfile && d.sha256 == l.profile
                })
                || !h.dma.is_empty()
                || !h.irq.is_empty()
                || !h.mmio.is_empty()
                || h.queues != 0
            {
                return Err(Error::Denied);
            }
        }
        if plan.revisions() != (self.policy_revision, self.inventory.revision()) {
            return Err(Error::Stale);
        }
        crate::push(
            &mut self.plans,
            SelectionPlan {
                parent,
                slot: node.slot,
                profile: target.profile.unwrap_or([0; 32]),
                plan,
                scope,
                inventory: self.inventory.revision(),
                catalog: self.catalog_revision,
                policy: self.policy_revision,
            },
            MAX_PLANS,
        )?;
        let g = &mut self.groups[i];
        g.change(State::Verified, rev);
        Ok(g.stamp())
    }
    fn group_usage(&self) -> Result<Use> {
        self.groups
            .iter()
            .try_fold(self.baseline, |u, g| u.checked_add(g.held))
    }
    pub fn usage(&self) -> Result<Use> {
        let mut total = self
            .group_usage()?
            .checked_add(self.fabric.usage())?
            .checked_add(self.fabric.actor_cost(self.fabric.actors()))?;
        for g in &self.groups {
            if g.held.receipts != 0 && self.fabric.spent(g.attachment, g.generation, false) {
                total.receipts -= 1;
            }
            if g.held.waits != 0 && self.fabric.spent(g.attachment, g.generation, true) {
                total.waits -= 1;
            }
        }
        Ok(total)
    }
    fn guard_q35(&self, a: Attachment) -> Result<()> {
        if self.q35 == Some(a.controller) && self.nvme.is_some() {
            Err(Error::Denied)
        } else {
            Ok(())
        }
    }
    pub fn reserve(
        &mut self,
        s: Stamp,
        holder: u64,
        incarnation: u64,
        domain: u32,
    ) -> Result<Stamp> {
        let i = self.checked(s)?;
        self.present(s.attachment)?;
        self.guard_q35(s.attachment)?;
        let g = &self.groups[i];
        if g.state != Verified {
            return Err(Error::Transition);
        }
        let vi = self
            .plans
            .iter()
            .position(|v| v.slot == s.attachment.slot)
            .ok_or(Error::Transition)?;
        let v = &self.plans[vi];
        if v.parent.is_some() {
            return Err(Error::Denied);
        }
        if (v.inventory, v.catalog, v.policy)
            != (
                self.inventory.revision(),
                self.catalog_revision,
                self.policy_revision,
            )
        {
            return Err(Error::Stale);
        }
        if incarnation <= self.last_incarnation {
            return Err(Error::Stale);
        }
        if holder <= 1 || domain == 0 {
            return Err(Error::Format);
        }
        let scope = v.scope;
        let mut active = 0;
        let mut controllers = [0u16; 8];
        for other in &self.groups {
            if let Some(l) = other.lease {
                if !controllers.contains(&l.attachment.controller) {
                    *controllers
                        .iter_mut()
                        .find(|n| **n == 0)
                        .ok_or(Error::Full)? = l.attachment.controller;
                }
                if (other.scope.members & (scope.members | scope.reset_peers)) != 0
                    || other.scope.reset_peers & scope.members != 0
                    || l.domain == domain
                    || l.holder == holder
                {
                    return Err(Error::Conflict);
                }
                if other.state == Active {
                    active |= other.scope.members
                }
            }
        }
        if !controllers.contains(&0) && !controllers.contains(&s.attachment.controller) {
            return Err(Error::Full);
        }
        if scope.dependencies & !active != 0 {
            return Err(Error::Denied);
        }
        let actor = u8::try_from(holder).map_err(|_| Error::Format)?;
        let (roots, handles) = self.actor_counts(actor)?;
        if (self.fabric.actors() | self.fabric.resident()) & (1u32 << actor) != 0 {
            return Err(Error::Denied);
        }
        let q = v.plan.quotas();
        if roots + q.ipc_roots_created > 4 || handles + q.ipc_handles > 32 {
            return Err(Error::Full);
        }
        let (new_roots, new_handles) = (q.ipc_roots_created, q.ipc_handles);
        let held = Use::plan(&v.plan)?;
        if !self.usage()?.checked_add(held)?.fits(self.cap) {
            return Err(Error::Full);
        }
        let rev = next(s.revision)?;
        let generation = next(self.last_generation)?;
        let seal = crate::plan_wire::seal(&v.plan);
        let grants = crate::rights::Grants::from_plan(&v.plan)?;
        let profile = v.profile;
        let (inventory, catalog, policy) = (v.inventory, v.catalog, v.policy);
        let g = &mut self.groups[i];
        g.source_revision = inventory;
        g.catalog = catalog;
        g.policy = policy;
        self.plans.swap_remove(vi);
        g.grants = grants;
        g.seal = seal;
        g.roots = new_roots;
        g.handles = new_handles;
        g.held = held;
        g.scope = scope;
        g.generation = generation;
        g.pages = mask(held.dma);
        g.initial_pages = g.pages;
        g.unmapped = 0;
        g.returned = 0;
        g.lease = Some(Lease {
            attachment: s.attachment,
            inventory: g.source_revision,
            holder,
            incarnation,
            generation,
            domain,
            profile,
        });
        g.ticket = false;
        g.launched = false;
        g.stopped = false;
        g.fresh_proof = false;
        g.last_proof = 0;
        g.exposed = false;
        g.prepared = false;
        g.backend = false;
        g.quiescent = false;
        g.change(Reserved, rev);
        self.last_generation = generation;
        self.last_incarnation = incarnation;
        Ok(g.stamp())
    }
    fn current_reservation(&self, s: Stamp) -> Result<usize> {
        let i = self.checked(s)?;
        let g = &self.groups[i];
        if g.state != Reserved {
            return Err(Error::Transition);
        }
        self.present(s.attachment)?;
        self.guard_q35(s.attachment)?;
        if (g.source_revision, g.catalog, g.policy)
            != (
                self.inventory.revision(),
                self.catalog_revision,
                self.policy_revision,
            )
        {
            return Err(Error::Stale);
        }
        Ok(i)
    }
    pub fn prepare_launch(&mut self, s: Stamp) -> Result<(Stamp, LaunchTicket)> {
        let i = self.current_reservation(s)?;
        let g = &mut self.groups[i];
        if g.ticket {
            return Err(Error::Conflict);
        }
        let token = LaunchTicket {
            lease: g.lease.ok_or(Error::Transition)?,
            seal: g.seal,
        };
        g.revision = next(g.revision)?;
        g.ticket = true;
        Ok((g.stamp(), token))
    }
    pub fn launch(&mut self, s: Stamp, t: LaunchTicket) -> Result<Stamp> {
        let i = self.current_reservation(s)?;
        let g = &mut self.groups[i];
        if !g.ticket || g.launched || g.lease != Some(t.lease) || g.seal != t.seal {
            return Err(Error::Denied);
        }
        g.revision = next(g.revision)?;
        g.launched = true;
        g.exposed = true;
        Ok(g.stamp())
    }
    pub fn grant_bytes(&self) -> usize {
        self.groups.iter().map(|g| g.grants.bytes()).sum()
    }
    pub fn leaf(
        &self,
        lease: Lease,
        right: crate::rights::Right,
        name: &str,
    ) -> Result<crate::rights::Leaf> {
        let g = self.group(lease.attachment.slot)?;
        if g.lease != Some(lease) || g.state != Active {
            return Err(Error::Stale);
        }
        self.present(lease.attachment)?;
        if (g.catalog, g.policy) != (self.catalog_revision, self.policy_revision) {
            return Err(Error::Stale);
        }
        Ok(crate::rights::Leaf {
            lease,
            epoch: g.leaf_epoch,
            offset: g.grants.offset(right, name)?,
            right,
        })
    }
    /// Le banc fournit le titulaire authentifié. Aucun effet matériel n'est exécuté.
    pub fn authorize(
        &self,
        leaf: crate::rights::Leaf,
        actor: u64,
        incarnation: u64,
        right: crate::rights::Right,
        name: &str,
    ) -> Result<()> {
        if leaf.lease.holder != actor
            || leaf.lease.incarnation != incarnation
            || leaf.right != right
        {
            return Err(Error::Denied);
        }
        let current = self.leaf(leaf.lease, right, name)?;
        if current.offset != leaf.offset || current.epoch != leaf.epoch {
            return Err(Error::Denied);
        }
        Ok(())
    }
    pub fn driver_death(&mut self, lease: Lease) -> Result<Stamp> {
        let g = self.group(lease.attachment.slot)?;
        if g.lease != Some(lease) {
            return Err(Error::Stale);
        }
        self.step(
            g.stamp(),
            if g.state == Reserved && g.launched {
                Event::Uncertain
            } else {
                Event::Revoke
            },
        )
    }
    /// Entrées distinctes d'un profil de laboratoire. Les feuilles du pilote ne peuvent les fabriquer via DATA.
    pub fn readiness(
        &mut self,
        lease: Lease,
        init: bool,
        backend: bool,
        quiescent: bool,
    ) -> Result<()> {
        let i = self.index(lease.attachment.slot)?;
        let g = &mut self.groups[i];
        if g.lease != Some(lease)
            || !matches!(g.state, Assigned | Initializing | Active | Suspended)
        {
            return Err(Error::Stale);
        }
        g.prepared = init;
        g.backend = backend;
        g.quiescent = quiescent;
        Ok(())
    }
    pub fn evidence(&mut self, p: Observation) -> Result<()> {
        let i = self.index(p.lease.attachment.slot)?;
        let g = &mut self.groups[i];
        if g.lease != Some(p.lease) || p.serial <= g.last_proof {
            return Err(Error::Stale);
        }
        if !matches!(g.state, Reconciling | Quarantined) {
            return Err(Error::Transition);
        }
        if (p.translations_removed | p.ownership_returned) & !g.initial_pages != 0 {
            return Err(Error::Format);
        }
        let stopped = g.stopped || p.source_stopped;
        let unmapped = g.unmapped | p.translations_removed;
        let returned = g.returned | p.ownership_returned;
        if (stopped, unmapped, returned) == (g.stopped, g.unmapped, g.returned) {
            return Err(Error::Stale);
        }
        g.stopped = stopped;
        g.unmapped = unmapped;
        g.returned = returned;
        if stopped {
            g.pages &= !(unmapped & returned);
        }
        g.held.dma = g.pages.count_ones();
        g.last_proof = p.serial;
        g.fresh_proof = true;
        Ok(())
    }
    pub fn step(&mut self, s: Stamp, e: Event) -> Result<Stamp> {
        let i = self.checked(s)?;
        let g = &self.groups[i];
        let to = e.destination();
        if !permits(Some(g.state), to) {
            return Err(Error::Transition);
        }
        let revision = next(s.revision)?;
        match e {
            Event::Assign | Event::Initialize | Event::Publish | Event::Resume => {
                self.present(s.attachment)?;
                if matches!(e, Event::Assign) {
                    self.guard_q35(s.attachment)?;
                    if !g.launched {
                        return Err(Error::Denied);
                    }
                }
                if g.source_revision != self.inventory.revision()
                    || g.catalog != self.catalog_revision
                    || g.policy != self.policy_revision
                {
                    return Err(Error::Stale);
                }
                if matches!(e, Event::Initialize) && g.state != Assigned
                    || matches!(e, Event::Resume) && g.state != Suspended
                {
                    return Err(Error::Transition);
                }
                if matches!(e, Event::Initialize | Event::Publish | Event::Resume) && !g.prepared
                    || matches!(e, Event::Publish) && !g.backend
                {
                    return Err(Error::Denied);
                }
            }
            Event::Suspend if !g.quiescent => return Err(Error::Uncertain),
            Event::Uncertain if g.state != Reserved => return Err(Error::Transition),
            Event::Cancel if g.state != Reserved || g.exposed => return Err(Error::Uncertain),
            Event::Reconcile if g.state == Reserved => return Err(Error::Transition),
            Event::Reconcile
                if g.state == Quarantined
                    && !g.fresh_proof
                    && !(g.lease.is_none() && self.inventory.ready()) =>
            {
                return Err(Error::Uncertain);
            }
            Event::Quarantine
                if g.lease.is_none() && self.inventory.ready() || g.stopped && g.pages == 0 =>
            {
                return Err(Error::Transition);
            }
            Event::Return
                if g.state != Reconciling
                    || g.lease.is_none() && !self.inventory.ready()
                    || g.lease.is_some() && (!g.stopped || g.pages != 0) =>
            {
                return Err(Error::Uncertain);
            }
            Event::Readmit => {
                self.guard_q35(s.attachment)?;
                if !self.inventory.ready() {
                    return Err(Error::Incomplete);
                }
                self.inventory.node(s.attachment.slot)?;
                if g.removed && self.inventory.attachment(s.attachment.slot)? == g.attachment {
                    return Err(Error::Stale);
                }
            }
            Event::Rediscover if g.state != Readmitting => return Err(Error::Transition),
            Event::Invalidate if g.state != State::Verified => return Err(Error::Transition),
            _ => {}
        }
        let mut dependants = 0;
        if matches!(e, Event::Suspend | Event::Revoke | Event::Remove) {
            let mut closed = g.scope.members | s.attachment.slot.bit();
            loop {
                let before = closed;
                for child in &self.groups {
                    if child.scope.dependencies & closed != 0 {
                        closed |= child.scope.members;
                        dependants |= child.attachment.slot.bit();
                    }
                }
                if before == closed {
                    break;
                }
            }
            if dependants != 0 && matches!(e, Event::Suspend) {
                return Err(Error::Conflict);
            }
            if self
                .groups
                .iter()
                .any(|c| dependants & c.attachment.slot.bit() != 0 && c.revision == u64::MAX)
            {
                return Err(Error::Exhausted);
            }
        }
        let new_attachment = if matches!(e, Event::Rediscover) {
            self.guard_q35(self.inventory.attachment(s.attachment.slot)?)?;
            if !self.inventory.ready() {
                return Err(Error::Incomplete);
            }
            Some(self.inventory.attachment(s.attachment.slot)?)
        } else {
            None
        };
        let g = &mut self.groups[i];
        if let Some(a) = new_attachment {
            g.attachment = a;
            g.removed = false
        }
        match e {
            Event::Cancel | Event::Return => {
                g.grants = Default::default();
                g.held = Use::default();
                g.lease = None;
                g.scope = Scope {
                    members: 0,
                    reset_peers: 0,
                    dependencies: 0,
                };
                g.pages = 0;
            }
            Event::Publish => g.leaf_epoch = revision,
            Event::Remove => g.removed = true,
            Event::Uncertain | Event::Assign => g.exposed = true,
            Event::Suspend => {
                g.backend = false;
                g.prepared = false;
                g.quiescent = false;
            }
            Event::Quarantine | Event::Reconcile => g.fresh_proof = false,
            _ => {}
        }
        self.plans.retain(|p| p.slot != s.attachment.slot);
        g.change(to, revision);
        let stamp = g.stamp();
        for child in &mut self.groups {
            if dependants & child.attachment.slot.bit() == 0 {
                continue;
            }
            let destination = if child.state == Reserved {
                Reconciling
            } else {
                Revoking
            };
            if permits(Some(child.state), destination) {
                child.change(destination, child.revision + 1);
            }
        }
        Ok(stamp)
    }
    pub fn bind_child(
        &self,
        s: Stamp,
        parent: Lease,
        subject: u64,
        incarnation: u64,
    ) -> Result<crate::rights::ClassLeaf> {
        self.checked(s)?;
        if subject == 0 || incarnation == 0 {
            return Err(Error::Denied);
        }
        self.use_mediated(s.attachment, parent)?;
        let v = self
            .plans
            .iter()
            .find(|v| v.slot == s.attachment.slot)
            .ok_or(Error::Stale)?;
        if v.parent != Some(parent)
            || (v.policy, v.catalog, v.inventory)
                != (
                    self.policy_revision,
                    self.catalog_revision,
                    self.inventory.revision(),
                )
        {
            return Err(Error::Stale);
        }
        Ok(crate::rights::ClassLeaf {
            stamp: s,
            parent,
            epoch: self.group(parent.attachment.slot)?.leaf_epoch,
            subject,
            incarnation,
        })
    }
    pub fn authorize_child(
        &self,
        leaf: crate::rights::ClassLeaf,
        subject: u64,
        incarnation: u64,
        right: &str,
    ) -> Result<()> {
        if leaf.subject != subject || leaf.incarnation != incarnation {
            return Err(Error::Denied);
        }
        let current = self.bind_child(leaf.stamp, leaf.parent, subject, incarnation)?;
        if current.epoch != leaf.epoch {
            return Err(Error::Stale);
        }
        if self
            .plan(leaf.stamp.attachment.slot)
            .is_some_and(|p| p.capabilities().iter().any(|s| s == right))
        {
            Ok(())
        } else {
            Err(Error::Denied)
        }
    }
    pub fn use_mediated(&self, endpoint: Attachment, parent: Lease) -> Result<()> {
        if self.mediated(endpoint.slot, parent)? != endpoint {
            return Err(Error::Stale);
        }
        Ok(())
    }
    pub fn mediated(&self, child: Slot, parent: Lease) -> Result<Attachment> {
        self.present(parent.attachment)?;
        let mut slot = child;
        if slot == parent.attachment.slot {
            return Err(Error::Denied);
        }
        while slot != parent.attachment.slot {
            let node = self.inventory.node(slot)?;
            if node.ownership != Ownership::Parent
                || !node.provider_trusted
                || node.controller != parent.attachment.controller
            {
                return Err(Error::Denied);
            }
            slot = node.parent.ok_or(Error::Denied)?;
        }
        let g = self.group(parent.attachment.slot)?;
        if g.lease != Some(parent)
            || g.state != Active
            || (g.catalog, g.policy) != (self.catalog_revision, self.policy_revision)
            || self.inventory.scope(parent.attachment.slot) != Ok(g.scope)
        {
            return Err(Error::Stale);
        }
        self.inventory.attachment(child)
    }
    pub fn policy_revision(&self) -> u64 {
        self.policy_revision
    }
    /// Empreinte de la sérialisation canonique utilisée par les tickets de lancement.
    pub fn plan_seal(&self, slot: Slot) -> Option<[u8; 32]> {
        self.plan(slot).map(crate::plan_wire::seal)
    }

    pub fn plan(&self, slot: Slot) -> Option<&Plan> {
        self.plans.iter().find(|p| p.slot == slot).map(|p| &p.plan)
    }
    pub fn plan_slots_bytes(&self) -> usize {
        self.plans.capacity() * size_of::<SelectionPlan>()
    }
    pub fn registry_bytes(&self) -> usize {
        self.groups.capacity() * size_of::<Group>()
    }
}

fn mask(count: u32) -> u128 {
    if count == 128 {
        u128::MAX
    } else {
        (1u128 << count) - 1
    }
}
