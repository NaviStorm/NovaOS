use super::*;
use crate::ipc::{Binding, Capability, Intent, Op, Reply, Request, Role, Status};
/// Politique locale du banc consultée pour RESOUDRE. Les octets DATA ne peuvent fournir ces droits.
pub struct Selection<'a, 'b> {
    pub target: &'a Target<'b>,
    pub pin: Option<[u8; 32]>,
    pub approved_members: u128,
}
/// Intention déjà inscrite, déplaçable uniquement dans le banc.
#[derive(Debug, Copy, Clone)]
pub struct Pending(Request);
pub enum Delivery {
    Reply(Reply),
    Pending(Pending),
}
impl Manager<'_, '_> {
    pub fn open_link(&mut self, b: Binding) -> Result<Capability> {
        if let Some(a) = b.group {
            self.present(a)?
        }
        let (roots, handles) = self.actor_counts(b.owner)?;
        if roots > 2 || handles > 30 || self.actor_counts(b.peer)?.1 > 30 {
            return Err(Error::Full);
        }
        let mask = (1u32 << b.owner) | (1u32 << b.peer);
        if self
            .groups
            .iter()
            .filter_map(|g| g.lease)
            .any(|l| mask & (1u32 << l.holder) != 0)
        {
            return Err(Error::Denied);
        }
        let used = self
            .usage()?
            .checked_add(self.fabric.actor_cost(mask & !self.fabric.actors()))?;
        self.fabric.open(b, used, self.cap)
    }
    /// L'autorité survivante remplace le mandat pour un seul pair et la même portée.
    pub fn recover_link(&mut self, old: Capability) -> Result<Capability> {
        self.fabric.rotate(old)
    }
    pub fn close_link(&mut self, c: Capability) -> Result<()> {
        let binding = self.fabric.binding(c)?;
        if binding.role == Role::Supervisor
            && binding.group.is_some_and(|a| {
                self.group(a.slot)
                    .is_ok_and(|g| g.attachment == a && g.lease.is_some())
            })
        {
            return Err(Error::Conflict);
        }
        self.fabric.close(c)
    }
    pub fn actor_counts(&self, actor: u8) -> Result<(u8, u8)> {
        let (mut roots, mut handles) = self.fabric.counts(actor)?;
        for g in &self.groups {
            if g.lease.is_some_and(|l| l.holder == u64::from(actor)) {
                roots += g.roots;
                handles += g.handles;
            }
        }
        Ok((roots, handles))
    }
    pub fn declare_existing_actor(&mut self, actor: u8, roots: u8, handles: u8) -> Result<()> {
        if !self.groups.is_empty() {
            return Err(Error::Conflict);
        }
        self.fabric.declare(actor, roots, handles, self.baseline)
    }
    pub fn management_epoch(&self) -> u64 {
        self.management_epoch
    }
    pub fn restart(&mut self, expected: u64) -> Result<Vec<(Binding, Capability)>> {
        if self.management_epoch != expected {
            return Err(Error::Stale);
        }
        let next_epoch = next(expected)?;
        let caps = self.fabric.restart()?;
        self.management_epoch = next_epoch;
        Ok(caps)
    }
    pub fn ipc_bytes(&self) -> (usize, usize) {
        self.fabric.bytes()
    }
    pub fn serve(
        &mut self,
        c: Capability,
        bytes: &[u8],
        selection: Option<Selection<'_, '_>>,
    ) -> Result<Reply> {
        match self.submit(c, bytes)? {
            Delivery::Reply(reply) => Ok(reply),
            Delivery::Pending(p) => self.complete(c, p, selection),
        }
    }
    pub fn submit(&mut self, c: Capability, bytes: &[u8]) -> Result<Delivery> {
        let r = Request::decode(bytes)?;
        let used = self.usage()?;
        let own = self.fabric.binding(c)?;
        // Le crédit garanti appartient au superviseur survivant. Un autre pair
        // peut clôturer sur ses crédits ordinaires sans immobiliser cette place.
        let closing = own.group.filter(|a| {
            own.role == Role::Supervisor
                && self.group(a.slot).is_ok_and(|g| {
                    g.attachment == *a
                        && g.generation == r.body.generation
                        && g.revision == r.revision
                        && g.lease.is_some()
                        && match r.op {
                            Op::Revoke => {
                                matches!(g.state, Assigned | Initializing | Active | Suspended)
                            }
                            Op::Reconcile => matches!(
                                g.state,
                                Reserved | Stopping | Removed | Reconciling | Quarantined
                            ),
                            _ => false,
                        }
                })
        });
        let receipt = closing.is_some_and(|a| !self.fabric.spent(a, r.body.generation, false));
        let wait = closing.is_some_and(|a| !self.fabric.spent(a, r.body.generation, true));
        let free = (usize::from(self.cap.receipts.saturating_sub(used.receipts))
            + usize::from(receipt))
        .min(usize::from(self.cap.waits.saturating_sub(used.waits)) + usize::from(wait));
        match self.fabric.begin(c, r, free, receipt, wait)? {
            Intent::Known(reply) => Ok(Delivery::Reply(reply)),
            Intent::Read(b) => {
                let reply = self.dispatch(b, r, None).unwrap_or_else(Reply::refused);
                self.fabric.remember(c, r, reply)?;
                Ok(Delivery::Reply(reply))
            }
            Intent::Write => Ok(Delivery::Pending(Pending(r))),
        }
    }
    pub fn complete(
        &mut self,
        c: Capability,
        pending: Pending,
        selection: Option<Selection<'_, '_>>,
    ) -> Result<Reply> {
        let r = pending.0;
        let (binding, known) = self.fabric.prepared(c, r)?;
        if known.status != Status::Pending {
            return Ok(known);
        }
        let reply = self.dispatch(binding, r, selection).unwrap_or_else(|e| {
            let mut reply = Reply::refused(e);
            reply.value = r.revision;
            reply.generation = r.body.generation;
            reply
        });
        self.fabric.finish(r, reply);
        Ok(reply)
    }
    fn dispatch(
        &mut self,
        b: Binding,
        r: Request,
        selection: Option<Selection<'_, '_>>,
    ) -> Result<Reply> {
        if r.op == Op::Hello {
            return Ok(Reply::confirmed(1, 0));
        }
        if b.role == Role::Discovery {
            let rev = self.inventory.revision();
            if r.revision != rev && !(r.revision == 0 && !r.op.mutation() && r.body.a == 0) {
                return Err(Error::Stale);
            }
            return match r.op {
                Op::Change => {
                    let gap = self.notify(r.body.a)?;
                    let mut reply = Reply::confirmed(r.body.a, 0);
                    reply.digest[0] = u8::from(gap);
                    Ok(reply)
                }
                Op::Reread => Ok(Reply::confirmed(rev, 0)),
                Op::Enumerate => {
                    if !self.inventory.ready() {
                        return Err(Error::Incomplete);
                    }
                    let i = r.body.a as usize;
                    if i > self.inventory.nodes().len() {
                        return Err(Error::Stale);
                    }
                    let mut out = Reply::confirmed(rev, 0);
                    if let Some(n) = self.inventory.nodes().get(i) {
                        out.generation = n.appearance;
                        out.digest[0] = n.slot.number();
                        out.digest[1..3].copy_from_slice(&n.controller.to_le_bytes());
                        out.digest[3] = n.parent.map_or(255, Slot::number);
                        out.digest[4] = match n.address {
                            Address::Pci { .. } => 1,
                            Address::Usb { .. } => 2,
                            Address::Acpi(_) => 3,
                            Address::DeviceTree(_) => 4,
                            Address::Virtual { .. } => 5,
                        };
                        out.digest[8..16].copy_from_slice(&((i + 1) as u64).to_le_bytes());
                    }
                    Ok(out)
                }
                _ => Err(Error::Denied),
            };
        }
        let a = b.group.ok_or(Error::Denied)?;
        let group = self.group(a.slot)?;
        let s = group.stamp();
        if a != s.attachment
            || r.body.generation != s.generation
            || r.revision != s.revision && !(r.revision == 0 && !r.op.mutation())
        {
            return Err(Error::Stale);
        }
        let stamp = match r.op {
            Op::Resolve => {
                let policy = selection.ok_or(Error::Denied)?;
                self.select(s, policy.target, policy.pin, policy.approved_members)?
            }
            Op::ReadPlan => {
                let staged = self
                    .plans
                    .iter()
                    .find(|p| p.slot == a.slot)
                    .ok_or(Error::Transition)?;
                if !self.inventory.ready()
                    || (staged.inventory, staged.catalog, staged.policy)
                        != (
                            self.inventory.revision(),
                            self.catalog_revision,
                            self.policy_revision,
                        )
                {
                    return Err(Error::Stale);
                }

                let mut out = Reply::confirmed(s.revision, s.generation);
                out.digest =
                    crate::plan_wire::read(self.plan(a.slot).ok_or(Error::Transition)?, r.body.a)?;
                return Ok(out);
            }
            Op::State => {
                let mut out = Reply::confirmed(s.revision, s.generation);
                out.digest[0] = group.state as u8;
                return Ok(out);
            }
            Op::Reserve => self.reserve(s, r.body.a, r.body.b, r.body.c)?,
            Op::Prepare => {
                let (stamp, ticket) = self.prepare_launch(s)?;
                let mut reply = Reply::confirmed(stamp.revision, stamp.generation);
                reply.digest = ticket.seal();
                return Ok(reply);
            }
            Op::Launch => self.launch(
                s,
                LaunchTicket {
                    lease: group.lease.ok_or(Error::Transition)?,
                    seal: group.seal,
                },
            )?,
            Op::Assign => self.step(s, Event::Assign)?,
            Op::PreparePort => {
                if group.state != Initializing || !group.prepared {
                    return Err(Error::Transition);
                }
                let new = next(s.revision)?;
                let i = self.index(a.slot)?;
                let g = &mut self.groups[i];
                g.backend = true;
                g.revision = new;
                g.stamp()
            }
            Op::Cancel => self.step(s, Event::Cancel)?,
            Op::Suspend => self.step(s, Event::Suspend)?,
            Op::Revoke | Op::Close => self.step(s, Event::Revoke)?,
            Op::Publish => self.step(s, Event::Publish)?,
            Op::Reconcile => {
                let e = match group.state {
                    Reserved => Event::Uncertain,
                    Reconciling
                        if group.lease.is_some() && (!group.stopped || group.pages != 0) =>
                    {
                        Event::Quarantine
                    }
                    Reconciling => Event::Return,
                    _ => Event::Reconcile,
                };
                self.step(s, e)?
            }
            _ => return Err(Error::Denied),
        };
        let mut reply = Reply::confirmed(stamp.revision, stamp.generation);
        reply.digest[0] = self.group(a.slot)?.state as u8;
        Ok(reply)
    }
}
