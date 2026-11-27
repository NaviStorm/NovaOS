//! Codec DATA et liaisons privées simulées. Le corps est sérialisé champ par champ.
use crate::{Error, Result, inventory::Attachment, next, resources::Use};
use alloc::vec::Vec;
pub const HEADER: usize = 32;
pub const MAX_DATA: usize = 128;
/// Premier octet du détail d'un REFUSE émis avant effet faute de preuve.
pub const PROOF_MISSING: u8 = 0x80;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Discovery,
    Catalog,
    Launcher,
    Supervisor,
    Service,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u16)]
pub enum Op {
    Hello = 1,
    Enumerate = 2,
    Change = 3,
    Reread = 4,
    Resolve = 5,
    ReadPlan = 6,
    Prepare = 7,
    Launch = 8,
    Cancel = 9,
    Receipt = 10,
    Reserve = 11,
    Assign = 12,
    Suspend = 13,
    Revoke = 14,
    Reconcile = 15,
    State = 16,
    PreparePort = 17,
    Publish = 18,
    Close = 19,
    Ack = 20,
}
use Op::*;
const OPS: [Op; 20] = [
    Hello,
    Enumerate,
    Change,
    Reread,
    Resolve,
    ReadPlan,
    Prepare,
    Launch,
    Cancel,
    Receipt,
    Reserve,
    Assign,
    Suspend,
    Revoke,
    Reconcile,
    State,
    PreparePort,
    Publish,
    Close,
    Ack,
];
impl Op {
    pub fn mutation(self) -> bool {
        !matches!(
            self,
            Hello | Enumerate | Reread | ReadPlan | Receipt | State
        )
    }
    fn permitted(self, r: Role) -> bool {
        match self {
            Hello | Receipt | Ack => true,
            Enumerate | Change | Reread => r == Role::Discovery,
            Resolve | ReadPlan => r == Role::Catalog,
            Prepare | Launch | Cancel => r == Role::Launcher,
            Reserve | Assign | Suspend | Revoke | Reconcile | State => r == Role::Supervisor,
            PreparePort | Publish | Close => r == Role::Service,
        }
    }
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Body {
    pub generation: u64,
    pub a: u64,
    pub b: u64,
    pub c: u32,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Request {
    pub op: Op,
    pub epoch: u64,
    pub id: u64,
    pub revision: u64,
    pub body: Body,
}
fn u16_at(b: &[u8], i: usize) -> u16 {
    u16::from_le_bytes([b[i], b[i + 1]])
}
fn u64_at(b: &[u8], i: usize) -> u64 {
    let mut v = [0; 8];
    v.copy_from_slice(&b[i..i + 8]);
    u64::from_le_bytes(v)
}
impl Request {
    pub fn encode(self, out: &mut [u8; MAX_DATA]) -> Result<usize> {
        self.valid()?;
        let len = if self.op == Hello { 32 } else { 64 };
        out[..len].fill(0);
        for (i, v) in [1, self.op as u16, len as u16, 0].into_iter().enumerate() {
            out[2 * i..2 * i + 2].copy_from_slice(&v.to_le_bytes());
        }
        for (i, v) in [self.epoch, self.id, self.revision].into_iter().enumerate() {
            out[8 + i * 8..16 + i * 8].copy_from_slice(&v.to_le_bytes());
        }
        if len > 32 {
            for (i, v) in [self.body.generation, self.body.a, self.body.b]
                .into_iter()
                .enumerate()
            {
                out[32 + 8 * i..40 + 8 * i].copy_from_slice(&v.to_le_bytes());
            }
            out[56..60].copy_from_slice(&self.body.c.to_le_bytes());
        }
        Ok(len)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        if !(HEADER..=MAX_DATA).contains(&bytes.len())
            || u16_at(bytes, 0) != 1
            || usize::from(u16_at(bytes, 4)) != bytes.len()
            || u16_at(bytes, 6) != 0
        {
            return Err(Error::Format);
        }
        let op = *OPS
            .iter()
            .find(|o| **o as u16 == u16_at(bytes, 2))
            .ok_or(Error::Format)?;
        let body = match (op, bytes.len()) {
            (Hello, 32) => Body::default(),
            (Hello, _) => return Err(Error::Format),
            (_, 64) if bytes[60..] == [0; 4] => Body {
                generation: u64_at(bytes, 32),
                a: u64_at(bytes, 40),
                b: u64_at(bytes, 48),
                c: u32::from_le_bytes(bytes[56..60].try_into().map_err(|_| Error::Format)?),
            },
            _ => return Err(Error::Format),
        };
        let r = Self {
            op,
            epoch: u64_at(bytes, 8),
            id: u64_at(bytes, 16),
            revision: u64_at(bytes, 24),
            body,
        };
        r.valid()?;
        Ok(r)
    }
    fn valid(self) -> Result<()> {
        if self.epoch == 0 || self.id == 0 || self.op.mutation() && self.revision == 0 {
            return Err(Error::Format);
        }
        let b = self.body;
        let ok = match self.op {
            Hello => b == Body::default() && self.revision == 0,
            Reserve => b.a != 0 && b.b != 0 && b.c != 0,
            Receipt | Ack => b.a != 0 && b.b != 0 && b.c == 0,
            Change => b.a != 0 && b.b == 0 && b.c == 0 && b.generation == 0,
            ReadPlan => b.a < 32769 && b.b == 0 && b.c == 0 && (b.a == 0 || self.revision != 0),
            Enumerate => b.a <= 128 && b.b == 0 && b.c == 0 && b.generation == 0,
            _ => b.a == 0 && b.b == 0 && b.c == 0,
        };
        if ok { Ok(()) } else { Err(Error::Format) }
    }
}
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Status {
    Confirmed = 1,
    Refused,
    Conflict,
    Stale,
    Full,
    Pending,
    Uncertain,
}
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct Reply {
    pub status: Status,
    pub value: u64,
    pub generation: u64,
    pub digest: [u8; 32],
}
impl Reply {
    pub fn confirmed(value: u64, generation: u64) -> Self {
        Self {
            status: Status::Confirmed,
            value,
            generation,
            digest: [0; 32],
        }
    }
    pub fn encode(self, r: Request) -> [u8; 88] {
        let mut b = [0; 88];
        b[0..2].copy_from_slice(&1u16.to_le_bytes());
        b[2..4].copy_from_slice(&(r.op as u16).to_le_bytes());
        b[4..6].copy_from_slice(&88u16.to_le_bytes());
        b[6] = 1;
        for (i, v) in [
            r.epoch,
            r.id,
            r.revision,
            self.status as u64,
            self.value,
            self.generation,
        ]
        .into_iter()
        .enumerate()
        {
            b[8 + i * 8..16 + i * 8].copy_from_slice(&v.to_le_bytes())
        }
        b[56..].copy_from_slice(&self.digest);
        b
    }
    pub fn decode(r: Request, b: &[u8]) -> Result<Self> {
        if b.len() != 88
            || u16_at(b, 0) != 1
            || u16_at(b, 2) != r.op as u16
            || u16_at(b, 4) != 88
            || u16_at(b, 6) != 1
            || u64_at(b, 8) != r.epoch
            || u64_at(b, 16) != r.id
            || u64_at(b, 24) != r.revision
        {
            return Err(Error::Format);
        }
        let status = match u64_at(b, 32) {
            1 => Status::Confirmed,
            2 => Status::Refused,
            3 => Status::Conflict,
            4 => Status::Stale,
            5 => Status::Full,
            6 => Status::Pending,
            7 => Status::Uncertain,
            _ => return Err(Error::Format),
        };
        Ok(Self {
            status,
            value: u64_at(b, 40),
            generation: u64_at(b, 48),
            digest: b[56..].try_into().map_err(|_| Error::Format)?,
        })
    }
    pub(crate) fn refused(e: Error) -> Self {
        let status = match e {
            Error::Conflict => Status::Conflict,
            Error::Stale | Error::Exhausted => Status::Stale,
            Error::Full | Error::Memory => Status::Full,
            Error::Uncertain => Status::Uncertain,
            _ => Status::Refused,
        };
        Self {
            status,
            ..Self::confirmed(0, 0)
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Binding {
    pub owner: u8,
    pub peer: u8,
    pub role: Role,
    pub group: Option<Attachment>,
}
#[derive(Debug, Clone, Copy)]
pub struct Capability {
    boot: u64,
    epoch: u64,
}
impl Capability {
    pub fn epoch(self) -> u64 {
        self.epoch
    }
}
struct Link {
    last: Option<(Request, Reply)>,
    epoch: u64,
    binding: Binding,
    high: u64,
}
struct Record {
    reserved_receipt: bool,
    reserved_wait: bool,
    request: Request,
    binding: Binding,
    reply: Reply,
}
pub(crate) enum Intent {
    Known(Reply),
    Read(Binding),
    Write,
}
pub struct Fabric {
    boot: u64,
    next_epoch: u64,
    links: Vec<Link>,
    records: Vec<Record>,
    existing: [(u8, u8); 32],
    resident: u32,
}
impl Fabric {
    pub fn new(boot: u64, existing: [(u8, u8); 32]) -> Result<Self> {
        if boot == 0 || existing.iter().any(|(r, h)| *r > 4 || *h > 32 || r > h) {
            return Err(Error::Format);
        }
        let mut records = Vec::new();
        records.try_reserve_exact(64).map_err(|_| Error::Memory)?;
        Ok(Self {
            boot,
            next_epoch: 0,
            links: Vec::new(),
            records,
            existing,
            resident: 0,
        })
    }
    pub(crate) fn declare(&mut self, actor: u8, r: u8, h: u8, baseline: Use) -> Result<()> {
        if !self.links.is_empty() || r > 4 || h > 32 || r > h {
            return Err(Error::Format);
        }
        let mut snapshot = self.existing;
        *snapshot.get_mut(actor as usize).ok_or(Error::Format)? = (r, h);
        let roots: u16 = snapshot.iter().map(|p| u16::from(p.0)).sum();
        let handles: u16 = snapshot.iter().map(|p| u16::from(p.1)).sum();
        if roots > baseline.endpoints || handles > baseline.nodes {
            return Err(Error::Full);
        }
        let resident = self.resident | (1u32 << actor);
        if resident.count_ones() > u32::from(baseline.processes) {
            return Err(Error::Full);
        }
        self.resident = resident;
        self.existing = snapshot;
        Ok(())
    }
    pub(crate) fn restart(&mut self) -> Result<Vec<(Binding, Capability)>> {
        let last = self
            .next_epoch
            .checked_add(self.links.len() as u64)
            .ok_or(Error::Exhausted)?;
        let mut caps = Vec::new();
        caps.try_reserve_exact(self.links.len())
            .map_err(|_| Error::Memory)?;
        for (i, l) in self.links.iter_mut().enumerate() {
            l.high = 0;
            l.last = None;
            l.epoch = self.next_epoch + 1 + i as u64;
            caps.push((
                l.binding,
                Capability {
                    boot: self.boot,
                    epoch: l.epoch,
                },
            ));
        }
        self.next_epoch = last;
        Ok(caps)
    }
    pub(crate) fn actors(&self) -> u32 {
        self.links.iter().fold(0, |m, l| {
            m | (1u32 << l.binding.owner) | (1u32 << l.binding.peer)
        })
    }
    pub(crate) fn resident(&self) -> u32 {
        self.resident
    }
    pub(crate) fn actor_cost(&self, mask: u32) -> Use {
        let count = (mask & !self.resident).count_ones();
        Use {
            processes: count as u16,
            pages: count * 16,
            ..Use::default()
        }
    }
    pub fn counts(&self, actor: u8) -> Result<(u8, u8)> {
        let (mut r, mut h) = *self.existing.get(actor as usize).ok_or(Error::Format)?;
        for l in &self.links {
            if l.binding.owner == actor {
                r += 2;
                h += 2;
            } else if l.binding.peer == actor {
                h += 2;
            }
        }
        Ok((r, h))
    }
    pub fn usage(&self) -> Use {
        Use {
            endpoints: self.links.len() as u16 * 2,
            nodes: self.links.len() as u16 * 4,
            waits: self
                .records
                .iter()
                .filter(|v| v.reply.status == Status::Pending)
                .count() as u16,
            receipts: self.records.len() as u16,
            ..Use::default()
        }
    }
    pub fn bytes(&self) -> (usize, usize) {
        (
            self.links.capacity() * size_of::<Link>(),
            self.records.capacity() * size_of::<Record>(),
        )
    }
    pub(crate) fn open(&mut self, b: Binding, used: Use, cap: Use) -> Result<Capability> {
        if ![b.owner, b.peer].contains(&1)
            || b.owner == b.peer
            || b.role == Role::Discovery && b.group.is_some()
        {
            return Err(Error::Denied);
        }
        if matches!(
            b.role,
            Role::Supervisor | Role::Launcher | Role::Service | Role::Catalog
        ) && b.group.is_none()
        {
            return Err(Error::Denied);
        }
        if self.links.iter().any(|l| {
            let old = l.binding;
            if old.role != b.role {
                return false;
            }
            let authority = |v: Binding| if v.owner == 1 { v.peer } else { v.owner };
            old.group == b.group
                || b.role == Role::Supervisor
                    && old.group.map(|a| a.controller) == b.group.map(|a| a.controller)
                    && authority(old) != authority(b)
        }) {
            return Err(Error::Conflict);
        }
        let (roots, handles) = self.counts(b.owner)?;
        if roots > 2
            || handles > 30
            || self.counts(b.peer)?.1 > 30
            || !used
                .checked_add(Use {
                    endpoints: 2,
                    nodes: 4,
                    ..Use::default()
                })?
                .fits(cap)
        {
            return Err(Error::Full);
        }
        let epoch = next(self.next_epoch)?;
        crate::push(
            &mut self.links,
            Link {
                last: None,
                epoch,
                binding: b,
                high: 0,
            },
            32,
        )?;
        self.next_epoch = epoch;
        Ok(Capability {
            boot: self.boot,
            epoch,
        })
    }
    fn channel(&self, c: Capability) -> Result<usize> {
        if c.boot != self.boot {
            return Err(Error::Denied);
        }
        self.links
            .iter()
            .position(|l| l.epoch == c.epoch)
            .ok_or(Error::Stale)
    }
    pub(crate) fn references(&self, a: Attachment) -> bool {
        self.links.iter().any(|l| l.binding.group == Some(a))
            || self.records.iter().any(|r| r.binding.group == Some(a))
    }
    pub(crate) fn binding(&self, c: Capability) -> Result<Binding> {
        Ok(self.links[self.channel(c)?].binding)
    }
    pub(crate) fn spent(&self, a: Attachment, generation: u64, pending: bool) -> bool {
        self.records.iter().any(|v| {
            (if pending {
                v.reserved_wait && v.reply.status == Status::Pending
            } else {
                v.reserved_receipt
            }) && v.binding.group == Some(a)
                && v.request.body.generation == generation
        })
    }
    pub(crate) fn rotate(&mut self, c: Capability) -> Result<Capability> {
        let i = self.channel(c)?;
        let epoch = next(self.next_epoch)?;
        self.next_epoch = epoch;
        self.links[i].epoch = epoch;
        self.links[i].high = 0;
        self.links[i].last = None;
        Ok(Capability {
            boot: self.boot,
            epoch,
        })
    }
    pub(crate) fn close(&mut self, c: Capability) -> Result<()> {
        let i = self.channel(c)?;
        if self
            .records
            .iter()
            .any(|r| r.binding == self.links[i].binding)
        {
            return Err(Error::Conflict);
        }
        self.links.swap_remove(i);
        Ok(())
    }
    /// Prépare la place du reçu avant de livrer une mutation à l'autorité.
    pub(crate) fn begin(
        &mut self,
        c: Capability,
        r: Request,
        free: usize,
        reserved_receipt: bool,
        reserved_wait: bool,
    ) -> Result<Intent> {
        let i = self.channel(c)?;
        let l = &self.links[i];
        if r.epoch != l.epoch || !r.op.permitted(l.binding.role) || (l.high == 0 && r.op != Hello) {
            return Err(Error::Denied);
        }
        if let Some((old, reply)) = l.last
            && old.id == r.id
        {
            return if old == r {
                Ok(Intent::Known(reply))
            } else {
                Err(Error::Conflict)
            };
        }
        if let Some(v) = self
            .records
            .iter()
            .find(|v| v.request.epoch == r.epoch && v.request.id == r.id)
        {
            return if v.request == r {
                Ok(Intent::Known(v.reply))
            } else {
                Err(Error::Conflict)
            };
        }
        if r.id <= l.high {
            return Err(Error::Stale);
        }
        if matches!(r.op, Receipt | Ack) {
            let index = self
                .records
                .iter()
                .position(|v| v.request.epoch == r.body.a && v.request.id == r.body.b);
            let Some(j) = index else {
                return if r.op == Receipt {
                    let reply = Reply::refused(Error::Uncertain);
                    self.remember(c, r, reply)?;
                    Ok(Intent::Known(reply))
                } else {
                    Err(Error::Stale)
                };
            };
            let record = &self.records[j];
            if record.binding != l.binding
                || record.reply.generation != r.body.generation
                    && !(r.op == Receipt && record.request.body.generation == r.body.generation)
            {
                return Err(Error::Denied);
            }
            if r.op == Receipt {
                let reply = record.reply;
                self.remember(c, r, reply)?;
                return Ok(Intent::Known(reply));
            }
            if r.id <= l.high
                || r.revision != record.reply.value
                || matches!(record.reply.status, Status::Pending | Status::Uncertain)
            {
                return Err(Error::Stale);
            }
            let reply = record.reply;
            self.remember(c, r, reply)?;
            self.records.swap_remove(j);
            return Ok(Intent::Known(reply));
        }
        let binding = l.binding;
        if !r.op.mutation() {
            return Ok(Intent::Read(binding));
        }
        let pending = self
            .records
            .iter()
            .filter(|v| v.binding == binding && v.reply.status == Status::Pending)
            .count();
        if free == 0 || pending >= if reserved_wait { 8 } else { 7 } {
            return Err(Error::Full);
        }
        let mut reply = Reply::confirmed(r.revision, r.body.generation);
        reply.status = Status::Pending;
        crate::push(
            &mut self.records,
            Record {
                reserved_receipt,
                reserved_wait,
                request: r,
                binding,
                reply,
            },
            64,
        )?;
        self.links[i].high = r.id;
        Ok(Intent::Write)
    }
    pub(crate) fn remember(&mut self, c: Capability, r: Request, reply: Reply) -> Result<()> {
        let i = self.channel(c)?;
        self.links[i].high = r.id;
        self.links[i].last = Some((r, reply));
        Ok(())
    }
    pub(crate) fn prepared(&self, c: Capability, r: Request) -> Result<(Binding, Reply)> {
        let channel = &self.links[self.channel(c)?];
        if channel.high == 0 {
            return Err(Error::Denied);
        }
        let b = channel.binding;
        let rec = self
            .records
            .iter()
            .find(|v| v.request == r)
            .ok_or(Error::Stale)?;
        if b != rec.binding {
            return Err(Error::Denied);
        }
        Ok((b, rec.reply))
    }
    pub(crate) fn finish(&mut self, r: Request, reply: Reply) {
        if let Some(v) = self.records.iter_mut().find(|v| v.request == r) {
            v.reply = reply;
        }
    }
}
