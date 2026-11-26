#![no_std]
pub mod protocol;
pub mod transport;
pub mod wire;

use wire::{FRAME, LIMIT};
pub const TX_RIGHT: u8 = 1;
pub const RX_RIGHT: u8 = 2;
pub const OBSERVE: u8 = 4;
pub const ADMIN: u8 = 8;
pub const MULTICAST: u64 = 1;
pub use wire::BOITE_GROUPEE;
pub const OPTIONS: u64 = MULTICAST | BOITE_GROUPEE;
const JOURNAL: usize = 2;
pub const EV_TX: u8 = 1;
pub const EV_RX: u8 = 2;
pub const EV_LINK: u8 = 4;
pub const EV_CLOSED: u8 = 8;
const UNBOUND: Binding = Binding {
    boot: [0; 16],
    port: 0,
    owner: 0,
    incarnation: 0,
    channel: 0,
    epoch: 0,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    Denied,
    Stale,
    Version,
    Feature,
    Invalid,
    Full,
    Conflict,
    Closed,
    State,
    LinkDown,
    Exhausted,
    NotAdmitted,
}
type Result<T> = core::result::Result<T, Error>;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Binding {
    pub boot: [u8; 16],
    pub port: u64,
    pub owner: u64,
    pub incarnation: u64,
    pub channel: u64,
    pub epoch: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Version {
    pub major: u16,
    pub minor: u16,
    pub features: u64,
}
pub fn negotiate(
    major: u16,
    min_minor: u16,
    required: u64,
    optional: u64,
    available: u64,
) -> Result<Version> {
    if major != 1 || min_minor != 0 {
        return Err(Error::Version);
    }
    let supported = available & OPTIONS;
    if required & !supported != 0 {
        return Err(Error::Feature);
    }
    Ok(Version {
        major: 1,
        minor: 0,
        features: required | (optional & supported),
    })
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Admitted,
    Publishing,
    Published,
    Complete,
    Cancelled,
    Unknown,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ticket {
    pub binding: Binding,
    pub request: u64,
    pub index: u8,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Slot {
    pub index: u16,
    pub generation: u32,
    pub binding: Binding,
}
#[derive(Clone, Copy)]
pub struct Frame<'a> {
    pub slot: Slot,
    pub bytes: &'a [u8],
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Carrier {
    Unknown,
    Down,
    Up,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Duplex {
    Unknown,
    Half,
    Full,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Link {
    pub carrier: Carrier,
    pub bps: Option<u64>,
    pub duplex: Duplex,
}
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct Stats {
    pub admitted: u64,
    pub published: u64,
    pub complete: u64,
    pub rx: u64,
    pub drops: u64,
    pub returned: u64,
    pub full: u64,
    pub cancelled: u64,
}
#[derive(Clone, Copy)]
struct Record {
    key: Slot,
    len: usize,
    bytes: [u8; FRAME],
    stage: Stage,
}
impl Record {
    const EMPTY: Self = Self {
        key: Slot {
            index: 0,
            generation: 0,
            binding: UNBOUND,
        },
        len: 0,
        bytes: [0; FRAME],
        stage: Stage::Cancelled,
    };
    fn same(&self, f: Frame<'_>) -> bool {
        self.key == f.slot && self.len == f.bytes.len() && self.bytes[..self.len] == *f.bytes
    }
}
#[derive(Clone, Copy)]
struct Batch {
    id: u64,
    n: usize,
    frames: [Record; LIMIT],
}
impl Batch {
    const EMPTY: Self = Self {
        id: 0,
        n: 0,
        frames: [Record::EMPTY; LIMIT],
    };
}
#[derive(Clone, Copy)]
struct Rx {
    generation: u32,
    len: usize,
    bytes: [u8; FRAME],
}
impl Rx {
    const EMPTY: Self = Self {
        generation: 0,
        len: 0,
        bytes: [0; FRAME],
    };
}

pub struct Port {
    binding: Binding,
    rights: u8,
    features: u64,
    closed: bool,
    station: [u8; 6],
    last: u64,
    acked: u64,
    credit: usize,
    tx_generation: [u32; LIMIT],
    journal: [Batch; JOURNAL],
    rx: [Rx; LIMIT],
    groups: [Option<[u8; 6]>; 8],
    revision: u64,
    events: [u64; 4],
    armed: bool,
    wake_bits: u8,
    stats: Stats,
    link: Link,
}
impl Port {
    pub fn open(
        binding: Binding,
        granted: u8,
        requested: u8,
        v: Version,
        station: [u8; 6],
    ) -> Result<Self> {
        if granted & !15 != 0 || requested == 0 || requested & !granted != 0 {
            return Err(Error::Denied);
        }
        if binding.epoch == 0
            || binding.channel == 0
            || binding.incarnation == 0
            || binding.port == 0
            || binding.owner == 0
            || binding.boot == [0; 16]
            || station == [0; 6]
            || station[0] & 1 != 0
        {
            return Err(Error::Invalid);
        }
        negotiate(v.major, v.minor, v.features, 0, OPTIONS)?;
        Ok(Self {
            binding,
            rights: requested,
            features: v.features,
            closed: false,
            station,
            last: 0,
            acked: 0,
            credit: LIMIT,
            tx_generation: [1; LIMIT],
            journal: [Batch::EMPTY; JOURNAL],
            rx: [Rx::EMPTY; LIMIT],
            groups: [None; 8],
            revision: 1,
            events: [0; 4],
            armed: false,
            wake_bits: 0,
            stats: Stats::default(),
            link: Link {
                carrier: Carrier::Unknown,
                bps: None,
                duplex: Duplex::Unknown,
            },
        })
    }
    fn authorize(&self, peer: Binding, rights: u8) -> Result<()> {
        if peer != self.binding {
            return Err(Error::Denied);
        }
        if self.closed {
            return Err(Error::Closed);
        }
        if rights & !self.rights != 0 {
            return Err(Error::Denied);
        }
        Ok(())
    }
    fn changed(&mut self, mask: u8) {
        self.revision = self.revision.saturating_add(1);
        if self.armed || self.wake_bits != 0 {
            self.armed = false;
            self.wake_bits |= mask;
        }
        for (i, v) in self.events.iter_mut().enumerate() {
            if (1 << i) & mask != 0 {
                *v = self.revision;
            }
        }
    }
    pub fn events_since(&self, peer: Binding, seen: u64) -> Result<(u64, u8)> {
        self.authorize(peer, 0)?;
        if seen > self.revision {
            return Err(Error::Stale);
        }
        let mut bits = 0u8;
        for (i, &v) in self.events.iter().enumerate() {
            if v > seen || seen == u64::MAX {
                bits |= 1 << i;
            }
        }
        Ok((self.revision, bits))
    }
    pub fn arm(&mut self, peer: Binding, seen: u64) -> Result<bool> {
        let (now, bits) = self.events_since(peer, seen)?;
        let ready = now != seen || seen == u64::MAX;
        self.armed = !ready;
        // ARM acquitte les notifications déjà observées jusqu'à seen.
        self.wake_bits = bits;
        Ok(ready)
    }
    /// La direction retour ne livre, après fermeture, que le signal terminal déjà dû.
    pub fn take_notification(&mut self, peer: Binding) -> Result<Option<(u64, u8)>> {
        if peer != self.binding {
            return Err(Error::Denied);
        }
        if self.wake_bits == 0 {
            return if self.closed {
                Err(Error::Closed)
            } else {
                Ok(None)
            };
        }
        let bits = core::mem::take(&mut self.wake_bits);
        Ok(Some((self.revision, bits)))
    }
    pub fn revision(&self, peer: Binding) -> Result<u64> {
        self.authorize(peer, 0)?;
        Ok(self.revision)
    }
    pub fn observe(&self, peer: Binding) -> Result<(Link, Stats)> {
        self.authorize(peer, OBSERVE)?;
        Ok((self.link, self.stats))
    }
    pub fn configure_link(&mut self, peer: Binding, link: Link) -> Result<()> {
        self.authorize(peer, ADMIN)?;
        if link.bps == Some(0)
            || (link.carrier != Carrier::Up
                && (link.bps.is_some() || link.duplex != Duplex::Unknown))
        {
            return Err(Error::Invalid);
        }
        if self.link != link {
            self.link = link;
            self.changed(EV_LINK);
        }
        Ok(())
    }
    pub fn credits(&self, peer: Binding) -> Result<usize> {
        self.authorize(peer, TX_RIGHT)?;
        Ok(self.credit)
    }
    pub fn tx_slot(&self, peer: Binding, index: usize) -> Result<Slot> {
        self.authorize(peer, TX_RIGHT)?;
        Ok(Slot {
            binding: peer,
            index: index.try_into().map_err(|_| Error::Invalid)?,
            generation: *self.tx_generation.get(index).ok_or(Error::Invalid)?,
        })
    }
    fn batch(&self, id: u64) -> Result<&Batch> {
        if id > self.last && self.last.checked_add(1) == Some(id) && !self.closed {
            return Err(Error::NotAdmitted);
        }
        if id == 0 || id <= self.acked || id > self.last {
            return Err(Error::Stale);
        }
        let b = &self.journal[(id % JOURNAL as u64) as usize];
        if b.id != id {
            return Err(Error::Stale);
        }
        Ok(b)
    }
    fn ticket(&self, request: u64, index: usize) -> Ticket {
        Ticket {
            binding: self.binding,
            request,
            index: index as u8,
        }
    }
    pub fn submit(&mut self, peer: Binding, id: u64, frames: &[Frame<'_>]) -> Result<Ticket> {
        self.authorize(peer, TX_RIGHT)?;
        if frames.is_empty() || frames.len() > LIMIT {
            return Err(Error::Invalid);
        }
        if id <= self.last {
            let prior = self.batch(id)?;
            if prior.n != frames.len()
                || !prior.frames.iter().zip(frames).all(|(old, f)| old.same(*f))
            {
                return Err(Error::Conflict);
            }
            return Ok(self.ticket(id, 0));
        }
        if self.last.checked_add(1) != Some(id) {
            return Err(Error::Invalid);
        }
        if self.link.carrier == Carrier::Down {
            return Err(Error::LinkDown);
        }
        if self.credit < frames.len() || id - self.acked > JOURNAL as u64 {
            self.stats.full = self.stats.full.saturating_add(1);
            return Err(Error::Full);
        }
        let mut mask = 0u16;
        for f in frames {
            if f.slot.binding != peer {
                return Err(Error::Denied);
            }
            let i = usize::from(f.slot.index);
            if !(14..=FRAME).contains(&f.bytes.len()) || i >= LIMIT || mask & (1 << i) != 0 {
                return Err(Error::Invalid);
            }
            if f.slot.generation != self.tx_generation[i] {
                return Err(Error::Stale);
            }
            if f.slot.generation == u32::MAX {
                return Err(Error::Exhausted);
            }
            if f.bytes[6..12] != self.station {
                return Err(Error::Denied);
            }
            mask |= 1 << i;
        }
        let b = &mut self.journal[(id % JOURNAL as u64) as usize];
        b.id = id;
        b.n = frames.len();
        for (to, f) in b.frames.iter_mut().zip(frames) {
            to.key = f.slot;
            to.len = f.bytes.len();
            to.stage = Stage::Admitted;
            to.bytes[..to.len].copy_from_slice(f.bytes);
            self.tx_generation[usize::from(f.slot.index)] += 1;
        }
        self.credit -= frames.len();
        self.last = id;
        self.stats.admitted = self.stats.admitted.saturating_add(frames.len() as u64);
        self.changed(EV_TX);
        Ok(self.ticket(id, 0))
    }
    fn index(&self, t: Ticket) -> Result<(usize, usize)> {
        if t.binding != self.binding || usize::from(t.index) >= self.batch(t.request)?.n {
            return Err(Error::Stale);
        }
        Ok(((t.request % JOURNAL as u64) as usize, usize::from(t.index)))
    }
    pub fn status(&self, peer: Binding, t: Ticket) -> Result<Stage> {
        self.authorize(peer, TX_RIGHT)?;
        let (b, i) = self.index(t)?;
        Ok(self.journal[b].frames[i].stage)
    }
    /// Unique authorité du backend, jamais une opération de la session cliente.
    pub fn backend(&mut self) -> Backend<'_> {
        Backend(self)
    }
    pub fn ack(&mut self, peer: Binding, through: u64) -> Result<()> {
        self.authorize(peer, TX_RIGHT)?;
        if through <= self.acked {
            return Ok(());
        }
        if through > self.last {
            return Err(Error::Invalid);
        }
        for id in self.acked + 1..=through {
            let b = self.batch(id)?;
            if !b.frames[..b.n]
                .iter()
                .all(|f| matches!(f.stage, Stage::Complete | Stage::Cancelled))
            {
                return Err(Error::State);
            }
        }
        for id in self.acked + 1..=through {
            self.journal[(id % JOURNAL as u64) as usize] = Batch::EMPTY;
        }
        self.acked = through;
        self.changed(EV_TX);
        Ok(())
    }
    pub fn multicast(&mut self, peer: Binding, mac: [u8; 6], join: bool) -> Result<()> {
        self.authorize(peer, RX_RIGHT)?;
        if self.features & MULTICAST == 0 {
            return Err(Error::Feature);
        }
        if mac[0] & 1 == 0 || mac == [255; 6] {
            return Err(Error::Invalid);
        }
        let known = self.groups.iter().position(|m| *m == Some(mac));
        match (join, known) {
            (true, None) => {
                let pos = self
                    .groups
                    .iter()
                    .position(Option::is_none)
                    .ok_or(Error::Full)?;
                self.groups[pos] = Some(mac);
            }
            (false, Some(pos)) => self.groups[pos] = None,
            _ => return Ok(()),
        }
        self.changed(EV_RX);
        Ok(())
    }
    pub fn rx_occupes(&self, peer: Binding) -> Result<usize> {
        self.authorize(peer, RX_RIGHT)?;
        Ok(self.rx.iter().filter(|s| s.len != 0).count())
    }
    pub fn read_rx(&self, peer: Binding, s: Slot) -> Result<&[u8]> {
        self.authorize(peer, RX_RIGHT)?;
        if s.binding != peer {
            return Err(Error::Denied);
        }
        let slot = self.rx.get(usize::from(s.index)).ok_or(Error::Invalid)?;
        if s.generation != slot.generation || slot.len == 0 {
            return Err(Error::Stale);
        }
        Ok(&slot.bytes[..slot.len])
    }
    pub fn release(&mut self, peer: Binding, s: Slot) -> Result<()> {
        self.authorize(peer, RX_RIGHT)?;
        if s.binding != peer {
            return Err(Error::Denied);
        }
        let r = self
            .rx
            .get_mut(usize::from(s.index))
            .ok_or(Error::Invalid)?;
        if s.generation == 0 || r.generation != s.generation {
            return Err(Error::Stale);
        }
        if r.len != 0 {
            r.bytes.fill(0);
            r.len = 0;
            self.stats.returned = self.stats.returned.saturating_add(1);
            self.changed(EV_RX);
        }
        Ok(())
    }
    pub fn close(&mut self, peer: Binding) -> Result<Retirement> {
        self.authorize(peer, 0)?;
        Ok(self.revoke())
    }
    /// Entrée du superviseur. Répéter le retrait ne crédite aucune trame.
    pub fn revoke(&mut self) -> Retirement {
        let first = !self.closed;
        self.closed = true;
        let mut result = Retirement::default();
        for b in &mut self.journal {
            for f in &mut b.frames[..b.n] {
                match f.stage {
                    Stage::Admitted => {
                        f.stage = Stage::Cancelled;
                        self.stats.cancelled = self.stats.cancelled.saturating_add(1);
                    }
                    Stage::Publishing | Stage::Published => f.stage = Stage::Unknown,
                    _ => {}
                }
                if f.stage == Stage::Unknown {
                    result.uncertain += 1;
                }
                if f.stage == Stage::Cancelled {
                    result.cancelled += 1;
                }
                f.bytes.fill(0);
                f.len = 0;
            }
        }
        self.rx.fill(Rx::EMPTY);
        self.groups.fill(None);
        self.credit = 0;
        if first {
            self.changed(EV_CLOSED);
        }
        result
    }
}
#[derive(Default, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Retirement {
    pub uncertain: u8,
    pub cancelled: u8,
}
pub struct Backend<'a>(&'a mut Port);
impl Backend<'_> {
    pub fn transition(&mut self, t: Ticket, from: Stage, to: Stage) -> Result<()> {
        let p = &mut self.0;
        if p.closed {
            return Err(Error::Closed);
        }
        let (b, i) = p.index(t)?;
        let state = &mut p.journal[b].frames[i].stage;
        if *state != from
            || !matches!(
                (from, to),
                (Stage::Admitted, Stage::Publishing)
                    | (Stage::Publishing, Stage::Published)
                    | (Stage::Published, Stage::Complete)
            )
        {
            return Err(Error::State);
        }
        *state = to;
        if to == Stage::Published {
            p.stats.published = p.stats.published.saturating_add(1);
        }
        if to == Stage::Complete {
            p.credit += 1;
            p.stats.complete = p.stats.complete.saturating_add(1);
        }
        p.changed(EV_TX);
        Ok(())
    }
    /// Le périphérique a pu recevoir la trame. Aucun crédit n'est rendu
    /// sur cette notification de perte. QUERY reste disponible avant révocation.
    pub fn incertain(&mut self, ticket: Ticket) -> Result<()> {
        if self.0.closed {
            return Err(Error::Closed);
        }
        let (b, i) = self.0.index(ticket)?;
        let stage = &mut self.0.journal[b].frames[i].stage;
        if !matches!(stage, Stage::Publishing | Stage::Published) {
            return Err(Error::State);
        }
        *stage = Stage::Unknown;
        self.0.changed(EV_TX);
        Ok(())
    }
    /// Le backend atteste que le descripteur n'a jamais été rendu visible.
    pub fn cancel_unpublished(&mut self, t: Ticket) -> Result<()> {
        let p = &mut self.0;
        if p.closed {
            return Err(Error::Closed);
        }
        let (b, i) = p.index(t)?;
        let f = &mut p.journal[b].frames[i];
        if !matches!(f.stage, Stage::Admitted | Stage::Publishing) {
            return Err(Error::State);
        }
        f.stage = Stage::Cancelled;
        p.credit += 1;
        p.stats.cancelled = p.stats.cancelled.saturating_add(1);
        p.changed(EV_TX);
        Ok(())
    }
    pub fn retired_status(&self, t: Ticket) -> Result<Stage> {
        if !self.0.closed {
            return Err(Error::State);
        }
        let (b, i) = self.0.index(t)?;
        Ok(self.0.journal[b].frames[i].stage)
    }
    pub fn image(&self, t: Ticket) -> Result<&[u8]> {
        if self.0.closed {
            return Err(Error::Closed);
        }
        let (b, i) = self.0.index(t)?;
        let f = &self.0.journal[b].frames[i];
        if f.stage != Stage::Publishing {
            return Err(Error::State);
        }
        Ok(&f.bytes[..f.len.max(60)])
    }
    pub fn receive(&mut self, data: &[u8]) -> Result<Slot> {
        let p = &mut self.0;
        if p.closed {
            return Err(Error::Closed);
        }
        if p.rights & RX_RIGHT == 0 {
            return Err(Error::Denied);
        }
        if !(60..=FRAME).contains(&data.len()) {
            return Err(Error::Invalid);
        }
        let mac: [u8; 6] = data[..6].try_into().unwrap();
        if mac != p.station && mac != [255; 6] && !p.groups.contains(&Some(mac)) {
            return Err(Error::Denied);
        }
        let Some(index) =
            p.rx.iter()
                .position(|r| r.len == 0 && r.generation < u32::MAX)
        else {
            p.stats.drops = p.stats.drops.saturating_add(1);
            return Err(Error::Full);
        };
        let r = &mut p.rx[index];
        r.generation += 1;
        r.len = data.len();
        r.bytes[..r.len].copy_from_slice(data);
        let result = Slot {
            binding: p.binding,
            index: index as u16,
            generation: r.generation,
        };
        p.stats.rx = p.stats.rx.saturating_add(1);
        p.changed(EV_RX);
        Ok(result)
    }
}
#[cfg(test)]
mod tests;
