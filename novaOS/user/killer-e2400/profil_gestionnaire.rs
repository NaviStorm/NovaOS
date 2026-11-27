//! Profil de médiation disponible uniquement avec e2400-sim. Aucune entrée dans la HAL.
use crate::{
    Error, Result,
    catalogue::{Bus, Kind},
    dma::{Direction, Region},
    inventory::{Address, Attachment, Ownership},
    lifecycle::{Lease, Stamp, State},
    manager::Manager,
    rights::Right,
};

#[allow(dead_code)]
#[path = "profil.rs"]
mod geometry;
const PROFILE: [u8; 32] = [
    0x74, 0x07, 0x35, 0x9f, 0x85, 0xac, 0x79, 0x3c, 0x31, 0x8b, 0xcf, 0x1b, 0xde, 0x16, 0xec, 0x0e,
    0x6e, 0x35, 0x22, 0xdd, 0x66, 0xdd, 0xd4, 0x80, 0x16, 0x34, 0x3a, 0x1d, 0x33, 0xd3, 0x8d, 0x42,
];

/// Observations injectées par le fournisseur du banc. Une valeur inconnue vaut false.
#[derive(Clone, Copy, Debug)]
pub struct Topology {
    pub revision: u64,
    pub nic: Attachment,
    pub storage: Attachment,
    pub aliases_resolved: bool,
    pub iommu_scopes_resolved: bool,
    pub resets_and_irq_isolated: bool,
    pub firmware_released: bool,
}
impl Topology {
    fn check(self, m: &Manager<'_, '_>, s: Stamp) -> Result<()> {
        let inv = m.inventory();
        if !inv.ready()
            || inv.revision() != self.revision
            || self.nic != s.attachment
            || inv.attachment(self.storage.slot)? != self.storage
            || inv.attachment(self.nic.slot)? != self.nic
        {
            return Err(Error::Stale);
        }
        if !(self.aliases_resolved
            && self.iommu_scopes_resolved
            && self.resets_and_irq_isolated
            && self.firmware_released)
        {
            return Err(Error::Incomplete);
        }
        let n = inv.node(self.nic.slot)?;
        if !matches!(
            n.address,
            Address::Pci {
                express: true,
                bridge: false,
                ..
            }
        ) || !matches!(
            n.identity,
            Bus::Pci {
                vendor: Some(0x1969),
                device: Some(0xe0a1),
                subsystem_vendor: Some(0x1462),
                subsystem_device: Some(0x115a),
                revision: Some(0x10),
                class_base: Some(2),
                subclass: Some(0)
            }
        ) || !matches!(
            inv.node(self.storage.slot)?.identity,
            Bus::Pci {
                class_base: Some(1),
                ..
            }
        ) {
            return Err(Error::Denied);
        }
        for peer in inv.nodes().iter().filter(|p| p.controller == n.controller) {
            if !peer.provider_trusted
                || peer.ownership == Ownership::Unknown
                || matches!(
                    peer.identity,
                    Bus::Unknown
                        | Bus::Pci {
                            class_base: None,
                            ..
                        }
                )
            {
                return Err(Error::Incomplete);
            }
        }
        let ns = inv.scope(n.slot)?;
        if ns.members != n.slot.bit() || ns.reset_peers != 0 {
            return Err(Error::Denied);
        }
        for disk in inv.nodes().iter().filter(|d| {
            matches!(
                d.identity,
                Bus::Pci {
                    class_base: Some(1),
                    ..
                }
            )
        }) {
            let ds = inv.scope(disk.slot)?;
            if (ns.members | ns.dependencies) & (ds.members | ds.reset_peers) != 0
                || ds.dependencies & ns.members != 0
            {
                return Err(Error::Conflict);
            }
        }
        Ok(())
    }
}

/// Jeton non constructible par le pilote. La méthode reserve valide les données du fournisseur
/// et le profil authentifié avant la transaction de réservation du gestionnaire.
pub struct Instance {
    lease: Lease,
    topology: Topology,
}

/// Accord du fournisseur CPU pour un seul plan et une seule destination de bail.
#[derive(Clone, Copy)]
pub(crate) struct Authorization {
    stamp: Stamp,
    topo: Topology,
    holder: u64,
    incarnation: u64,
    domain: u32,
}
pub struct Permit(Authorization);
impl Permit {
    pub fn install(
        m: &mut Manager<'_, '_>,
        s: Stamp,
        topo: Topology,
        holder: u64,
        incarnation: u64,
        domain: u32,
    ) -> Result<Self> {
        let permit = Authorization::check_plan(m, s, topo, holder, incarnation, domain)?;
        m.install_simulated_permit(s, permit)?;
        Ok(Self(permit))
    }
    pub fn bind(self, m: &Manager<'_, '_>) -> Result<Instance> {
        self.0.bind(m)
    }
}
impl Authorization {
    fn check_plan(
        m: &Manager<'_, '_>,
        s: Stamp,
        topo: Topology,
        holder: u64,
        incarnation: u64,
        domain: u32,
    ) -> Result<Self> {
        topo.check(m, s)?;
        let p = m.plan(s.attachment.slot).ok_or(Error::Transition)?;
        if p.driver() != "org.novaos.lab.killer-e2400-5a"
            || !p.dependencies().iter().any(|d| {
                d.kind == Kind::SystemProfile
                    && d.id == "org.novaos.lab.profil-e2400-5a"
                    && d.sha256 == PROFILE
            })
            || p.hardware().dma_pages as usize != geometry::cadres_dma()
            || !p.hardware().mmio.is_empty()
            || !p.hardware().irq.is_empty()
            || p.hardware().max_irq_routes != 0
            || p.hardware().queues != 2
            || p.hardware().entries_per_queue != 256
        {
            return Err(Error::Denied);
        }
        Ok(Self {
            stamp: s,
            topo,
            holder,
            incarnation,
            domain,
        })
    }
    pub(crate) fn reserve(
        self,
        m: &mut Manager<'_, '_>,
        s: Stamp,
        holder: u64,
        incarnation: u64,
        domain: u32,
    ) -> Result<Stamp> {
        if s != self.stamp
            || (holder, incarnation, domain) != (self.holder, self.incarnation, self.domain)
        {
            return Err(Error::Stale);
        }
        self.topo.check(m, s)?;
        let regions = geometry::REGIONS.map(|r| Region {
            name: r.role,
            frames: r.cadres(),
        });
        m.reserve_inner(s, holder, incarnation, domain, Some((&PROFILE, &regions)))
    }
    fn bind(self, m: &Manager<'_, '_>) -> Result<Instance> {
        let g = m.group(self.stamp.attachment.slot)?;
        let lease = g.lease().ok_or(Error::Transition)?;
        if g.state() != State::Reserved
            || g.stamp().revision != self.stamp.revision.checked_add(1).ok_or(Error::Exhausted)?
            || lease.attachment != self.stamp.attachment
            || lease.profile != PROFILE
            || lease.inventory != self.topo.revision
            || (lease.holder, lease.incarnation, lease.domain)
                != (self.holder, self.incarnation, self.domain)
        {
            return Err(Error::Stale);
        }
        Ok(Instance {
            lease,
            topology: self.topo,
        })
    }
}
impl Instance {
    pub fn lease(&self) -> Lease {
        self.lease
    }
    pub fn check(&self, m: &Manager<'_, '_>, caller: Lease, class: &str) -> Result<()> {
        if caller != self.lease {
            return Err(Error::Stale);
        }
        let g = m.group(caller.attachment.slot)?;
        self.topology.check(m, g.stamp())?;
        m.leaf(caller, Right::Class, class)?;
        Ok(())
    }
    pub fn page(
        &self,
        m: &Manager<'_, '_>,
        lease: Lease,
        index: usize,
        dir: Direction,
    ) -> Result<()> {
        if lease != self.lease {
            return Err(Error::Stale);
        }
        let g = m.group(lease.attachment.slot)?;
        if g.state() != State::Active {
            return Err(Error::Transition);
        }
        self.topology.check(m, g.stamp())?;
        let p = m.dma_page(lease, index)?;
        if !p.exposed
            || p.released
            || p.quarantined
            || p.translation_removed
            || p.source_stopped
            || p.ownership_returned
            || dir != p.direction
        {
            return Err(Error::Denied);
        }
        Ok(())
    }
    pub fn retiring(&self, m: &Manager<'_, '_>, l: Lease) -> Result<()> {
        let g = m.group(self.lease.attachment.slot)?;
        if l != self.lease || g.lease() != Some(l) {
            return Err(Error::Stale);
        }
        if !matches!(
            g.state(),
            State::Stopping | State::Reconciling | State::Quarantined
        ) {
            return Err(Error::Transition);
        }
        Ok(())
    }
}
