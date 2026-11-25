//! Un crédit lié au service bloc et à son incarnation. Le pilote ne dispose
//! d'aucune opération acceptant une adresse ou le contenu brut d'une SQE.
use super::*;
use crate::abi::{BAD_ARGUMENT, CANCELLED, DENIED, LIMIT, OK, REFUSED, TIMED_OUT, WOULD_BLOCK};
use crate::hardware::nvme::{Command, Completion, Controller, Error as DeviceError};
use alloc::vec::Vec;
use wire::*;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Stage {
    Reserved,
    Submitted,
    Completed,
}
struct Ticket {
    id: u64,
    blocks: u16,
    stage: Stage,
    command: Command,
    source: Vec<u8>,
}
pub(super) struct Broker {
    pub(super) controller: Controller,
    seq: [u64; 2],
    pending: Option<Command>,
    geometry: Option<Completion>,
    pub(super) active: bool,
    ticket: Option<Ticket>,
    pub(super) stopped: bool,
    pub(super) failure: Option<DeviceError>,
    pub(super) submitted: usize,
    pub(super) completed: usize,
    pub(super) revoked: bool,
    mode: u64,
    pub(super) writes: usize,
    pub(super) flushes: usize,
    #[cfg(feature="net-q35-duo")]
    samples: Samples,
}
type Status<T> = Result<T, u64>;

#[cfg(feature="net-q35-duo")]
#[derive(Default)]
struct Samples {
    hz:u64, begin:u64, command:u64, bytes:[u64;2], cycles:[u64;3], flush:Vec<u64>,
}

#[cfg(feature="net-q35-duo")]
impl Broker {
    pub(super) fn begin_samples(&mut self) {
        self.samples.hz=hal::timer::counter_hz();
        self.samples.begin=hal::timer::counter();
    }
    pub(super) fn report_samples(&self)->Result<(),&'static str> {
        let s=&self.samples;
        if s.hz==0 || s.bytes.contains(&0) || s.flush.len()!=self.flushes || !(256..=1024).contains(&s.flush.len()) {
            return Err("échantillons NVMe incomplets");
        }
        let elapsed=hal::timer::counter()-s.begin;
        for group in s.flush.chunks(32) {hal::log(format_args!("DUO FLUSH cycles={group:?}"));}
        hal::log(format_args!("DUO mesures hz={} duree={} octets={:?} service={:?} flush={}",s.hz,elapsed,s.bytes,s.cycles,s.flush.len()));
        Ok(())
    }
}

impl Broker {
    pub(super) fn new(controller: Controller, mode: u64) -> Self {
        Self {
            controller,
            seq: [0; 2],
            pending: None,
            geometry: None,
            active: false,
            ticket: None,
            stopped: false,
            failure: None,
            submitted: 0,
            completed: 0,
            revoked: false,
            mode,
            writes: 0,
            flushes: 0,
            #[cfg(feature="net-q35-duo")]
            samples:Samples::default(),
        }
    }
    fn apply<T>(&mut self, r: Result<T, DeviceError>) -> Status<T> {
        r.map_err(|e| {
            if matches!(
                e,
                DeviceError::State
                    | DeviceError::Busy
                    | DeviceError::Bounds
                    | DeviceError::Namespace
            ) {
                return BAD_ARGUMENT;
            }
            self.failure = Some(e);
            match e {
                DeviceError::Timeout => TIMED_OUT,
                DeviceError::Status(_) if self.mode >= profile::INIT => MEDIA_IO,
                _ => REFUSED,
            }
        })
    }
    fn limit(&self, g: Completion) -> u64 {
        // La dernière page physique reste hors de tout bail du profil inscriptible.
        if self.mode >= profile::INIT {
            g.sectors - MAX_TRANSFER / u64::from(g.block_size)
        } else {
            g.sectors
        }
    }
    pub(super) fn flight(&self) -> Option<Command> {
        self.pending
    }
    /// Point d'observation du premier transfert de la transaction P9. La
    /// FLUSH d'ouverture a déjà fini et les auto-contrôles de droits sont passés.
    pub(super) fn withdrawal_point(&self, mode: u64) -> Option<u64> {
        let t = self.ticket.as_ref()?;
        if self.flushes == 0
            || self.pending.is_some()
            || !matches!(t.command, Command::Write { .. })
        {
            return None;
        }
        match (mode, t.stage) {
            (profile::REVOKE_BEFORE, Stage::Reserved)
                if !t.source.is_empty() && self.writes == 0 =>
            {
                Some(t.id)
            }
            (profile::REVOKE_AFTER, Stage::Completed) if self.writes == 1 => Some(t.id),
            _ => None,
        }
    }

    pub(super) fn frozen_source(&self) -> Option<&[u8]> {
        self.ticket
            .as_ref()
            .map(|t| t.source.as_slice())
            .filter(|s| !s.is_empty())
    }
    fn block(
        &mut self,
        s: &mut Core,
        pid: Pid,
        domain: &Domain,
        tag: u64,
        seq: u64,
        args: [u64; 3],
        reply: &mut [u8; 128],
    ) -> Status<()> {
        let [a, b, c] = args;
        match tag {
            GEOMETRY if args == [0; 3] => {
                let g = self.geometry.filter(|_| self.active).ok_or(WOULD_BLOCK)?;
                put(reply, 3, u64::from(g.nsid));
                put(reply, 4, self.limit(g));
                put(reply, 5, u64::from(g.block_size));
            }
            REGISTER | REGISTER_WRITE | REGISTER_FLUSH => {
                let g = self.geometry.filter(|_| self.active).ok_or(REFUSED)?;
                if self.ticket.is_some() {
                    return Err(LIMIT);
                }
                if a != u64::from(g.nsid) {
                    return Err(DENIED);
                }
                let command = if tag == REGISTER_FLUSH {
                    if b != 0 || c != 0 {
                        return Err(BAD_ARGUMENT);
                    }
                    Command::Flush { nsid: g.nsid }
                } else {
                    let limit = self.limit(g);
                    if c == 0
                        || b >= limit
                        || c > limit - b
                        || c > MAX_TRANSFER / u64::from(g.block_size)
                    {
                        return Err(BAD_ARGUMENT);
                    }
                    if tag == REGISTER_WRITE {
                        Command::Write {
                            nsid: g.nsid,
                            lba: b,
                            blocks: c as u16,
                        }
                    } else {
                        Command::Read {
                            nsid: g.nsid,
                            lba: b,
                            blocks: c as u16,
                        }
                    }
                };
                self.ticket = Some(Ticket {
                    id: seq,
                    blocks: c as u16,
                    command,
                    stage: Stage::Reserved,
                    source: Vec::new(),
                });
                put(reply, 3, seq);
            }
            STAGE | COLLECT_PAGE => {
                let t = self.ticket.as_mut().filter(|t| t.id == a).ok_or(DENIED)?;
                let n = t.blocks as usize * self.geometry.ok_or(REFUSED)?.block_size as usize;
                if n == 0 || n > MAX_TRANSFER as usize || c != n as u64 {
                    return Err(BAD_ARGUMENT);
                }
                let space = s.processes.get(pid).space.as_ref().ok_or(DENIED)?;
                if tag == STAGE {
                    if t.stage != Stage::Reserved
                        || !matches!(t.command, Command::Write { .. })
                        || !t.source.is_empty()
                    {
                        return Err(DENIED);
                    }
                    let mut frozen = Vec::new();
                    frozen.try_reserve_exact(n).map_err(|_| abi::NO_MEMORY)?;
                    frozen.resize(n, 0);
                    if !heap::with_pages(|m| Ok(space.copy_from_user(m, b, &mut frozen))).unwrap() {
                        return Err(abi::BAD_ADDRESS);
                    }
                    t.source = frozen;
                } else {
                    if t.stage != Stage::Completed || !matches!(t.command, Command::Read { .. }) {
                        return Err(DENIED);
                    }
                    let mut bytes = Vec::new();
                    bytes.try_reserve_exact(n).map_err(|_| abi::NO_MEMORY)?;
                    bytes.resize(n, 0);
                    self.controller
                        .read_data(0, &mut bytes)
                        .map_err(|_| REFUSED)?;
                    if !domain.dma_clean() {
                        self.failure = Some(DeviceError::DmaFault);
                        return Err(REFUSED);
                    }
                    if !heap::with_pages(|m| Ok(space.copy_to_user(m, b, &bytes))).unwrap() {
                        return Err(abi::BAD_ADDRESS);
                    }
                }
                put(reply, 3, n as u64);
            }
            COLLECT => {
                let t = self
                    .ticket
                    .as_ref()
                    .filter(|t| {
                        t.id == a
                            && t.stage == Stage::Completed
                            && matches!(t.command, Command::Read { .. })
                    })
                    .ok_or(DENIED)?;
                let bytes =
                    usize::from(t.blocks) * self.geometry.ok_or(REFUSED)?.block_size as usize;
                let offset = usize::try_from(b).map_err(|_| BAD_ARGUMENT)?;
                let len = usize::try_from(c).map_err(|_| BAD_ARGUMENT)?;
                if len == 0 || len > CHUNK || offset > bytes || len > bytes - offset {
                    return Err(BAD_ARGUMENT);
                }
                self.controller
                    .read_data(offset, &mut reply[HEADER..HEADER + len])
                    .map_err(|_| REFUSED)?;
                put(reply, 3, c);
            }
            RELEASE | CANCEL if b == 0 && c == 0 => {
                let t = self.ticket.as_ref().filter(|t| t.id == a).ok_or(DENIED)?;
                match (tag, t.stage) {
                    (RELEASE, Stage::Completed) | (CANCEL, Stage::Reserved | Stage::Completed) => {
                        self.ticket = None
                    }
                    (CANCEL, Stage::Submitted) => {
                        self.revoked = true;
                        return Err(CANCELLED);
                    }
                    _ => return Err(DENIED),
                }
            }
            _ => return Err(DENIED),
        }
        Ok(())
    }
    fn driver(
        &mut self,
        tag: u64,
        args: [u64; 3],
        now: u64,
        domain: &Domain,
        reply: &mut [u8; 128],
    ) -> Status<()> {
        if matches!(
            tag,
            IDENTIFY_NAMESPACE | SUBMIT_READ | SUBMIT_WRITE | SUBMIT_FLUSH
        ) {
            if args[1..] != [0, 0] {
                return Err(BAD_ARGUMENT);
            }
        } else if args != [0; 3] {
            return Err(BAD_ARGUMENT);
        }
        match tag {
            DISCOVER => {
                // Le profil PCI ne crée ce rôle qu'après validation de ces trois champs.
                put(reply, 3, 0x1b36);
                put(reply, 4, 0x0010);
                put(reply, 5, 0x30);
            }
            ENABLE => {
                let r = self.controller.enable(now);
                self.apply(r)?;
            }
            READY => {
                let r = self.controller.ready(now);
                if !self.apply(r)? {
                    return Err(WOULD_BLOCK);
                }
            }
            POLL => {
                let cmd = self.pending.ok_or(DENIED)?;
                let cid = self.controller.inflight_cid().ok_or(REFUSED)?;
                let r = self.controller.poll_checked(now, || domain.dma_clean());
                if let Err(error) = r {
                    self.failure = Some(error);
                    if self.mode >= profile::INIT {
                        let raw = match error {
                            DeviceError::Status(status) => Some(status),
                            _ => None,
                        };
                        put(reply, 4, cid.into());
                        put(reply, 5, raw.map_or(u64::MAX, u64::from));
                        hal::log(format_args!(
                            "NVME RW erreur commande={cmd:?} cid={cid} statut_nvme={raw:?} erreur={error:?}"
                        ));
                    }
                }
                let info = self.apply(r)?.ok_or(WOULD_BLOCK)?;
                self.pending = None;
                if self.mode >= profile::INIT && cmd == Command::IdentifyController {
                    hal::log(format_args!("NVME RW Identify VWC={:#x}", info.vwc));
                }
                match cmd {
                    Command::IdentifyController => {
                        put(reply, 3, info.vid.into());
                        put(reply, 4, info.version.into());
                        reply[HEADER..HEADER + 20].copy_from_slice(&info.serial);
                        reply[HEADER + 20..HEADER + 60].copy_from_slice(&info.model);
                        hal::log(format_args!(
                            "NVME Identify Controller vid={:#x} vs={:#x} serial={} modele={} mdts={}",
                            info.vid,
                            info.version,
                            core::str::from_utf8(&info.serial).unwrap_or("").trim(),
                            core::str::from_utf8(&info.model).unwrap_or("").trim(),
                            info.mdts
                        ));
                    }
                    Command::ListNamespaces => put(reply, 3, NSID),
                    Command::IdentifyNamespace => {
                        put(reply, 3, info.nsid.into());
                        put(reply, 4, info.sectors);
                        put(reply, 5, info.block_size.into());
                        reply[HEADER..HEADER + 8].copy_from_slice(&info.eui64);
                        hal::log(format_args!(
                            "NVME Identify Namespace nsid={} secteurs={} lba={} eui64={:016x}",
                            info.nsid,
                            info.sectors,
                            info.block_size,
                            u64::from_be_bytes(info.eui64)
                        ));
                        if info.sectors.checked_mul(u64::from(info.block_size))
                            != Some(if self.mode >= profile::INIT {
                                4 << 20
                            } else {
                                1 << 20
                            })
                        {
                            self.failure = Some(DeviceError::Geometry);
                            return Err(REFUSED);
                        }
                        self.geometry = Some(info);
                    }
                    Command::CreateSq => self.active = true,
                    Command::Read { .. } | Command::Write { .. } | Command::Flush { .. } => {
                        let t = self
                            .ticket
                            .as_mut()
                            .filter(|t| t.stage == Stage::Submitted)
                            .ok_or(DENIED)?;
                        t.stage = Stage::Completed;
                        self.completed += 1;
                        match cmd {
                            Command::Write { .. } => self.writes += 1,
                            Command::Flush { .. } => {
                                self.flushes += 1;
                                hal::log(format_args!(
                                    "NVME RW FLUSH CQE cid={} ecritures={} numero={}",
                                    info.cid, self.writes, self.flushes
                                ));
                            }
                            _ => {}
                        }
                        let bytes = if matches!(cmd, Command::Write { .. }) {
                            t.source.len()
                        } else {
                            info.bytes
                        };
                        #[cfg(feature="net-q35-duo")]
                        if self.mode==profile::MEASURE {
                            let elapsed=hal::timer::counter()-self.samples.command;
                            let k=match cmd {Command::Read{..}=>0,Command::Write{..}=>1,_=>2};
                            self.samples.cycles[k]+=elapsed;
                            if k<2 {self.samples.bytes[k]+=bytes as u64;}
                            else if self.samples.flush.len()<1024 {self.samples.flush.push(elapsed);}
                        }
                        put(reply, 3, bytes as u64);
                    }
                    _ => {}
                }
            }
            SHUTDOWN => {
                let r = self.controller.shutdown(now);
                self.apply(r)?;
            }
            STOPPED => {
                let r = self.controller.stopped(now);
                if !self.apply(r)? {
                    return Err(WOULD_BLOCK);
                }
                self.stopped = true;
            }
            _ => {
                if self.pending.is_some() {
                    return Err(LIMIT);
                }
                let cmd = match tag {
                    IDENTIFY_CONTROLLER => Command::IdentifyController,
                    LIST_NAMESPACES => Command::ListNamespaces,
                    IDENTIFY_NAMESPACE if args[0] == NSID => Command::IdentifyNamespace,
                    NUMBER_QUEUES => Command::NumberQueues,
                    CREATE_CQ => Command::CreateCq,
                    CREATE_SQ => Command::CreateSq,
                    DELETE_SQ if self.ticket.is_none() => Command::DeleteSq,
                    DELETE_CQ => Command::DeleteCq,
                    SUBMIT_READ | SUBMIT_WRITE | SUBMIT_FLUSH => {
                        let t = self
                            .ticket
                            .as_ref()
                            .filter(|t| t.id == args[0] && t.stage == Stage::Reserved)
                            .ok_or(DENIED)?;
                        if !matches!(
                            (tag, t.command),
                            (SUBMIT_READ, Command::Read { .. })
                                | (SUBMIT_WRITE, Command::Write { .. })
                                | (SUBMIT_FLUSH, Command::Flush { .. })
                        ) {
                            return Err(DENIED);
                        }
                        t.command
                    }
                    _ => return Err(DENIED),
                };
                #[cfg(feature="net-q35-duo")]
                if self.mode==profile::MEASURE {self.samples.command=hal::timer::counter();}
                let r = if matches!(cmd, Command::Write { .. }) {
                    let src = self
                        .ticket
                        .as_ref()
                        .filter(|t| !t.source.is_empty())
                        .ok_or(DENIED)?;
                    self.controller.submit_write(cmd, &src.source, now)
                } else {
                    self.controller.submit(cmd, now)
                };
                self.apply(r)?;
                self.pending = Some(cmd);
                if matches!(tag, SUBMIT_READ | SUBMIT_WRITE | SUBMIT_FLUSH) {
                    self.ticket.as_mut().unwrap().stage = Stage::Submitted;
                    self.submitted += 1;
                }
                if tag == DELETE_SQ {
                    self.active = false;
                }
            }
        }
        Ok(())
    }
    /// Le seul redémarrage supervisé admis sans pilote concerne une file
    /// connue, vide, dont toutes les commandes ont déjà été comptabilisées.
    pub(super) fn close_idle(&mut self, domain: &Domain) -> bool {
        if !self.active
            || !domain.dma_clean()
            || self.pending.is_some()
            || self.failure.is_some()
            || self
                .ticket
                .as_ref()
                .is_some_and(|t| t.stage == Stage::Submitted)
        {
            return false;
        }
        for cmd in [Command::DeleteSq, Command::DeleteCq] {
            if self.controller.submit(cmd, now_ms()).is_err() {
                return false;
            }
            loop {
                match self
                    .controller
                    .poll_checked(now_ms(), || domain.dma_clean())
                {
                    Ok(Some(_)) => break,
                    Ok(None) => sleep(1),
                    Err(_) => return false,
                }
            }
        }
        if self.controller.shutdown(now_ms()).is_err() {
            return false;
        }
        loop {
            match self.controller.stopped(now_ms()) {
                Ok(true) => return true,
                Ok(false) => sleep(1),
                Err(_) => return false,
            }
        }
    }
    pub(super) fn step(
        &mut self,
        s: &mut Core,
        peers: &Peers,
        domain: &Domain,
        role: usize,
        now: u64,
    ) -> Status<bool> {
        let pid = peers.pids[role];
        let cfg = &peers.cfg[role];
        // L'autorité dépend de l'endpoint d'arrivée identifié par le noyau.
        // Une trame qui nomme WRITE sur un canal READ ne confère aucun droit.
        let mut received = None;
        for slot in [BROKER_SEND, WRITE_CAP, FLUSH_CAP] {
            let port = cfg[slot];
            if port != 0
                && let Some(frame) = s.ipc.nvme_request(pid, port)?
            {
                received = Some((slot, frame));
                break;
            }
        }
        let Some((slot, (tag, [epoch, seq, a, b, c]))) = received else {
            return Ok(false);
        };
        if seq == 0 || seq <= self.seq[role] {
            return Err(BAD_ARGUMENT);
        }
        self.seq[role] = seq;
        let mut reply = [0u8; RESPONSE_SIZE];
        put(&mut reply, 0, cfg[EPOCH]);
        put(&mut reply, 1, seq);
        let expected_port = if role == 1 {
            match tag {
                REGISTER_WRITE | STAGE => WRITE_CAP,
                REGISTER_FLUSH => FLUSH_CAP,
                _ => BROKER_SEND,
            }
        } else {
            BROKER_SEND
        };
        let permission = match tag {
            REGISTER_WRITE | STAGE | SUBMIT_WRITE => Some(WRITE_CAP),
            REGISTER_FLUSH | SUBMIT_FLUSH => Some(FLUSH_CAP),
            POLL | RELEASE => self.ticket.as_ref().and_then(|t| match t.command {
                Command::Write { .. } => Some(WRITE_CAP),
                Command::Flush { .. } => Some(FLUSH_CAP),
                _ => None,
            }),
            _ => None,
        };
        let file_io = matches!(
            tag,
            REGISTER
                | REGISTER_WRITE
                | REGISTER_FLUSH
                | STAGE
                | SUBMIT_READ
                | SUBMIT_WRITE
                | SUBMIT_FLUSH
                | COLLECT_PAGE
                | COLLECT
                | RELEASE
        ) || (tag == POLL && self.ticket.is_some());
        let allowed = (!file_io || peers.file_authorized(s))
            && permission.is_none_or(|k| {
                let handle = peers.cfg[1][k];
                profile::writable(self.mode)
                    && handle != 0
                    && s.ipc.nvme_live(peers.pids[1], handle, abi::SEND)
            });
        if file_io
            && !allowed
            && self.ticket.is_some()
            && expected_port == slot
            && epoch == cfg[EPOCH]
            && (matches!(tag, POLL | RELEASE) || !peers.file_authorized(s))
        {
            self.revoked = true;
        }
        let mut result = if slot != expected_port
            || !allowed
            || epoch != cfg[EPOCH]
            || self.revoked
            || self.failure.is_some()
        {
            Err(DENIED)
        } else if tag != POLL && !domain.dma_clean() {
            self.failure = Some(DeviceError::DmaFault);
            Err(REFUSED)
        } else if role == 0 {
            self.driver(tag, [a, b, c], now, domain, &mut reply)
        } else {
            self.block(s, pid, domain, tag, seq, [a, b, c], &mut reply)
        };
        if role == 1 && tag == COLLECT && result.is_ok() && !domain.dma_clean() {
            // Les octets ne quittent le courtier qu'après cette dernière vérification.
            reply[3 * 8..].fill(0);
            self.failure = Some(DeviceError::DmaFault);
            result = Err(REFUSED);
        }
        put(&mut reply, 2, result.err().unwrap_or(OK));
        let space = s.processes.get(pid).space.as_ref().ok_or(DENIED)?;
        if !heap::with_pages(|m| Ok(space.copy_to_user(m, RESPONSE_VA, &reply))).unwrap()
            || !s.ipc.lab_notify(pid, cfg[BROKER_REPLY], seq)
        {
            return Err(DENIED);
        }
        s.flush_ipc();
        Ok(true)
    }
}
