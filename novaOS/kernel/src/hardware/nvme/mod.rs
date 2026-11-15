//! Courtier du profil Q35 x86_64 en lecture seule. L'ordonnancement des
//! demandes vient du pilote utilisateur. Aucune adresse ni SQE ne vient d'IPC.
mod identify;
mod queue;

use core::sync::atomic::{Ordering, fence};
use queue::{ACQ, ASQ, DATA, DEPTH, Queue};

const PAGE: usize = 4096;
const CAP: usize = 0;
const VS: usize = 8;
const CC: usize = 0x14;
const CSTS: usize = 0x1c;
const AQA: usize = 0x24;
const ASQ_BASE: usize = 0x28;
const ACQ_BASE: usize = 0x30;
const ENABLED: u32 = (4 << 20) | (6 << 16) | 1;
const SHN: u32 = 1 << 14;
const IO_TIMEOUT_MS: u64 = 5_000;

/// Addresses établies exclusivement par le domaine VT-d. Le type ne donne
/// aucun accès matériel avant la vérification de provenance de prepare().
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct Memory {
    pub(crate) epoch: u64,
    pub(crate) bar: usize,
    pub(crate) len: usize,
    pub(crate) cpu: [usize; 5],
    pub(crate) iova: [u64; 5],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Command {
    IdentifyController,
    ListNamespaces,
    IdentifyNamespace,
    NumberQueues,
    CreateCq,
    CreateSq,
    Read { nsid: u32, lba: u64, blocks: u16 },
    DeleteSq,
    DeleteCq,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Error {
    Memory,
    Unsupported,
    Inherited,
    State,
    Busy,
    Bounds,
    Identity,
    Namespace,
    Geometry,
    Completion,
    Status(u16),
    Fault,
    DmaFault,
    Timeout,
    Exhausted,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Completion {
    pub(crate) bytes: usize,
    pub(crate) sectors: u64,
    pub(crate) block_size: u32,
    pub(crate) nsid: u32,
    pub(crate) vid: u16,
    pub(crate) version: u32,
    pub(crate) serial: [u8; 20],
    pub(crate) model: [u8; 40],
    pub(crate) eui64: [u8; 8],
    pub(crate) mdts: u8,
}

impl Completion {
    const EMPTY: Self = Self {
        bytes: 0,
        sectors: 0,
        block_size: 0,
        nsid: 0,
        vid: 0,
        version: 0,
        serial: [0; 20],
        model: [0; 40],
        eui64: [0; 8],
        mdts: 0,
    };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Prepared,
    Enabling,
    Admin,
    Identified,
    Listed,
    Geometry,
    Negotiated,
    Cq,
    Active,
    SqDeleted,
    Empty,
    ShuttingDown,
    Disabling,
    Stopped,
    Held,
}

#[derive(Clone, Copy)]
struct Flight {
    command: Command,
    cid: u16,
    bytes: usize,
    deadline: u64,
}

pub(crate) struct Controller {
    mem: Memory,
    state: State,
    info: Completion,
    admin: Queue,
    io: Queue,
    flight: Option<Flight>,
    read_submitted: bool,
    next_cid: u16,
    stride: usize,
    timeout_ms: u64,
    deadline: u64,
}

pub(crate) struct Drained<'a>(&'a Controller);

impl Drained<'_> {
    pub(crate) fn confirmed(&self, mem: &Memory) -> bool {
        let c = self.0;
        c.mem == *mem && c.state == State::Stopped && c.flight.is_none() && initial(&c.mem)
    }
}

fn initial(mem: &Memory) -> bool {
    mem.reg32(CC) == 0
        && mem.reg32(CSTS) == 0
        && mem.reg32(AQA) == 0
        && mem.reg64(ASQ_BASE) == 0
        && mem.reg64(ACQ_BASE) == 0
}

impl Controller {
    /// # Safety
    /// L'appelant détient un groupe exclusif admis par le profil Q35, BME
    /// fermé. La BAR UC couvre len octets. Les cinq pages CPU et IOVA sont
    /// appariées, privées, cohérentes sur x86, alignées et valides jusqu'au
    /// retrait confirmé ou retenues en quarantaine. Un seul appelant à la fois
    /// contrôle cette incarnation. Aucun de ces pointeurs ne provient d'IPC.
    pub(crate) unsafe fn prepare(mem: Memory) -> Result<Self, Error> {
        if !mem.valid() {
            return Err(Error::Memory);
        }
        let cap = mem.reg64(CAP);
        let stride = 4usize << ((cap >> 32) & 15);
        let timeout_ms = (cap >> 24 & 0xff) * 500;
        if mem.reg32(VS) != identify::VERSION
            || cap & (1 << 37) == 0
            || (cap >> 48 & 15) != 0
            || (cap as u16) < DEPTH - 1
            || timeout_ms == 0
            || 0x1000 + 3 * stride + 4 > mem.len
        {
            return Err(Error::Unsupported);
        }
        if !initial(&mem) {
            return Err(Error::Inherited);
        }
        for page in 0..mem.cpu.len() {
            mem.zero(page);
        }
        let size = u32::from(DEPTH - 1);
        let attr = size | size << 16;
        mem.put32(AQA, attr);
        mem.put64(ASQ_BASE, mem.iova[ASQ]);
        mem.put64(ACQ_BASE, mem.iova[ACQ]);
        if mem.reg32(AQA) != attr
            || mem.reg64(ASQ_BASE) != mem.iova[ASQ]
            || mem.reg64(ACQ_BASE) != mem.iova[ACQ]
        {
            return Err(Error::Fault);
        }
        Ok(Self {
            mem,
            state: State::Prepared,
            info: Completion::EMPTY,
            admin: Queue::new(0),
            io: Queue::new(1),
            flight: None,
            read_submitted: false,
            next_cid: 1,
            stride,
            timeout_ms,
            deadline: 0,
        })
    }

    fn hold<T>(&mut self, error: Error) -> Result<T, Error> {
        self.state = State::Held;
        self.info.bytes = 0;
        Err(error)
    }

    /// Appeler une fois le domaine publié, BME autorisé et les sources d'IRQ
    /// PCI neutralisées. Les données des files doivent être visibles avant EN.
    pub(crate) fn enable(&mut self, now_ms: u64) -> Result<(), Error> {
        if self.state != State::Prepared {
            return Err(Error::State);
        }
        self.state = State::Enabling;
        self.deadline = now_ms.saturating_add(self.timeout_ms);
        fence(Ordering::SeqCst);
        self.mem.put32(CC, ENABLED);
        if self.mem.reg32(CC) != ENABLED {
            return self.hold(Error::Fault);
        }
        Ok(())
    }

    pub(crate) fn ready(&mut self, now_ms: u64) -> Result<bool, Error> {
        if self.state != State::Enabling {
            return Err(Error::State);
        }
        if now_ms >= self.deadline {
            return self.hold(Error::Timeout);
        }
        match self.mem.reg32(CSTS) {
            0 => Ok(false),
            1 => {
                self.state = State::Admin;
                Ok(true)
            }
            _ => self.hold(Error::Fault),
        }
    }

    fn length(&self, cmd: Command) -> Result<usize, Error> {
        use Command::*;
        match (self.state, cmd) {
            (State::Admin, IdentifyController)
            | (State::Identified, ListNamespaces)
            | (State::Listed, IdentifyNamespace) => Ok(PAGE),
            (State::Geometry, NumberQueues)
            | (State::Negotiated, CreateCq)
            | (State::Cq, CreateSq)
            | (State::Active, DeleteSq)
            | (State::SqDeleted, DeleteCq) => Ok(0),
            (State::Active, Read { nsid, lba, blocks }) => {
                if nsid != self.info.nsid {
                    return Err(Error::Namespace);
                }
                let bytes = usize::from(blocks)
                    .checked_mul(self.info.block_size as usize)
                    .ok_or(Error::Bounds)?;
                if bytes == 0
                    || bytes > PAGE
                    || lba >= self.info.sectors
                    || u64::from(blocks) > self.info.sectors - lba
                {
                    return Err(Error::Bounds);
                }
                Ok(bytes)
            }
            _ => Err(Error::State),
        }
    }

    /// Le service détenteur a déjà autorisé le sous-intervalle de son bail.
    /// Le courtier recoupe ici l'opération avec l'identité et la taille réelles.
    pub(crate) fn submit(&mut self, cmd: Command, now_ms: u64) -> Result<(), Error> {
        if self.flight.is_some() {
            return Err(Error::Busy);
        }
        let bytes = self.length(cmd)?;
        if self.mem.reg32(CSTS) != 1 {
            return self.hold(Error::Fault);
        }
        let cid = self.next_cid;
        // Un CID n'est jamais recyclé dans cette incarnation.
        self.next_cid = cid.checked_add(1).ok_or(Error::Exhausted)?;
        let queue = if matches!(cmd, Command::Read { .. }) {
            &mut self.io
        } else {
            &mut self.admin
        };
        if queue.has_completion(&self.mem) {
            return self.hold(Error::Completion);
        }
        let sqe = queue::entry(cmd, cid, &self.mem);
        self.info.bytes = 0;
        if bytes != 0 {
            self.mem.zero(DATA);
        }
        self.flight = Some(Flight {
            command: cmd,
            cid,
            bytes,
            deadline: now_ms.saturating_add(IO_TIMEOUT_MS),
        });
        self.read_submitted |= matches!(cmd, Command::Read { .. });
        queue.publish(&self.mem, self.stride, &sqe);
        Ok(())
    }

    pub(crate) fn idle_before_read(&self, mem: &Memory) -> bool {
        self.mem == *mem
            && self.state == State::Active
            && self.flight.is_none()
            && !self.read_submitted
            && self.mem.reg32(CSTS) == 1
            && self.mem.reg32(CC) == ENABLED
            && !self.admin.has_completion(mem)
            && !self.io.has_completion(mem)
    }

    #[cfg(test)]
    pub(crate) fn poll(&mut self, now_ms: u64) -> Result<Option<Completion>, Error> {
        self.poll_checked(now_ms, || true)
    }

    pub(crate) fn poll_checked(
        &mut self,
        now_ms: u64,
        dma_clean: impl FnOnce() -> bool,
    ) -> Result<Option<Completion>, Error> {
        use Command::*;
        let pending = self.flight.ok_or(Error::State)?;
        if self.state == State::Held {
            return Err(Error::State);
        }
        if now_ms >= pending.deadline {
            return self.hold(Error::Timeout);
        }
        if self.mem.reg32(CSTS) != 1 {
            return self.hold(Error::Fault);
        }
        let queue = if matches!(pending.command, Read { .. }) {
            &mut self.io
        } else {
            &mut self.admin
        };
        let cqe = queue.consume(&self.mem, self.stride, pending.cid);
        // Un contrôleur émulé peut déposer un CQE de succès malgré un DMA refusé.
        // Échantillonner VT-d après acquisition du CQE et avant lecture du résultat.
        if !dma_clean() {
            return self.hold(Error::DmaFault);
        }
        let result = match cqe {
            Ok(Some(value)) => value,
            Ok(None) => return Ok(None),
            Err(error) => return self.hold(error),
        };
        // La publication de phase du contrôleur suit ses autres écritures DMA.
        // L'acquisition CQ précède cette lecture de la page destination.
        let data = unsafe { &*(self.mem.cpu[DATA] as *const [u8; PAGE]) };
        let next = match pending.command {
            IdentifyController => {
                identify::controller(data, &mut self.info).map(|_| State::Identified)
            }
            ListNamespaces => identify::list(data).map(|_| State::Listed),
            IdentifyNamespace => identify::namespace(data, &mut self.info).map(|_| State::Geometry),
            NumberQueues if result != 0 => Err(Error::Completion), // une et une demandées
            NumberQueues => Ok(State::Negotiated),
            CreateCq => Ok(State::Cq),
            CreateSq | Read { .. } => Ok(State::Active),
            DeleteSq => Ok(State::SqDeleted),
            DeleteCq => Ok(State::Empty),
        };
        match next {
            Err(e) => return self.hold(e),
            Ok(state) => self.state = state,
        }
        self.flight = None;
        if matches!(pending.command, Read { .. }) {
            self.info.bytes = pending.bytes;
        }
        Ok(Some(self.info))
    }

    pub(crate) fn read_data(&self, off: usize, out: &mut [u8]) -> Result<(), Error> {
        if self.state != State::Active
            || self.flight.is_some()
            || out.is_empty()
            || off > self.info.bytes
            || out.len() > self.info.bytes - off
        {
            return Err(Error::Bounds);
        }
        unsafe {
            core::ptr::copy_nonoverlapping(
                (self.mem.cpu[DATA] + off) as *const u8,
                out.as_mut_ptr(),
                out.len(),
            );
        }
        Ok(())
    }

    /// Les CQ des suppressions de SQ puis CQ font partie de la preuve de fin
    /// normale. Un timeout ou une incohérence ne transforme jamais Held en vide.
    pub(crate) fn shutdown(&mut self, now_ms: u64) -> Result<(), Error> {
        if self.state != State::Empty || self.flight.is_some() {
            return Err(Error::State);
        }
        if self.mem.reg32(CSTS) != 1
            || self.admin.has_completion(&self.mem)
            || self.io.has_completion(&self.mem)
        {
            return self.hold(Error::Completion);
        }
        self.info.bytes = 0;
        self.deadline = now_ms.saturating_add(self.timeout_ms);
        self.state = State::ShuttingDown;
        self.mem.put32(CC, ENABLED | SHN);
        Ok(())
    }

    pub(crate) fn stopped(&mut self, now_ms: u64) -> Result<bool, Error> {
        if !matches!(self.state, State::ShuttingDown | State::Disabling) {
            return Err(Error::State);
        }
        if now_ms >= self.deadline {
            return self.hold(Error::Timeout);
        }
        let csts = self.mem.reg32(CSTS);
        if csts & 2 != 0 {
            return self.hold(Error::Fault);
        }
        if self.state == State::ShuttingDown {
            if self.mem.reg32(CC) != (ENABLED | SHN) || csts & 1 == 0 || csts >> 2 & 3 == 3 {
                return self.hold(Error::Fault);
            }
            if csts >> 2 & 3 == 2 {
                self.state = State::Disabling;
                self.deadline = now_ms.saturating_add(self.timeout_ms);
                self.mem.put32(CC, 0);
            }
            return Ok(false);
        }
        if csts & 1 != 0 {
            return Ok(false);
        }
        if self.mem.reg32(CC) != 0 || csts != 0 {
            return self.hold(Error::Fault);
        }
        // Après RDY=0, cette instance connaît et possède les bases écrites.
        // Laisser un état identifiable comme vierge à la prochaine instance.
        self.mem.put32(AQA, 0);
        self.mem.put64(ASQ_BASE, 0);
        self.mem.put64(ACQ_BASE, 0);
        fence(Ordering::SeqCst);
        if !initial(&self.mem) {
            return self.hold(Error::Fault);
        }
        self.state = State::Stopped;
        Ok(true)
    }

    pub(crate) fn drained(&self) -> Option<Drained<'_>> {
        let proof = Drained(self);
        proof.confirmed(&self.mem).then_some(proof)
    }
}
