//! Un crédit lié au service bloc et à son incarnation. Le pilote ne dispose
//! d'aucune opération acceptant une adresse ou le contenu brut d'une SQE.
use super::*;
use crate::abi::{BAD_ARGUMENT, CANCELLED, DENIED, LIMIT, OK, REFUSED, TIMED_OUT, WOULD_BLOCK};
use crate::hardware::nvme::{Command, Completion, Controller, Error as DeviceError};
use wire::*;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Stage {
    Reserved,
    Submitted,
    Completed,
}
struct Ticket {
    id: u64,
    lba: u64,
    blocks: u16,
    stage: Stage,
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
}
type Status<T> = Result<T, u64>;

impl Broker {
    pub(super) fn new(controller: Controller) -> Self {
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
            if e == DeviceError::Timeout {
                TIMED_OUT
            } else {
                REFUSED
            }
        })
    }
    fn block(&mut self, tag: u64, seq: u64, args: [u64; 3], reply: &mut [u8; 128]) -> Status<()> {
        let [a, b, c] = args;
        match tag {
            GEOMETRY if args == [0; 3] => {
                let g = self.geometry.filter(|_| self.active).ok_or(WOULD_BLOCK)?;
                put(reply, 3, u64::from(g.nsid));
                put(reply, 4, g.sectors);
                put(reply, 5, u64::from(g.block_size));
            }
            REGISTER => {
                let g = self.geometry.filter(|_| self.active).ok_or(REFUSED)?;
                if self.ticket.is_some() {
                    return Err(LIMIT);
                }
                // Le bail couvre exactement le namespace unique de l'image RAW de test.
                if a != u64::from(g.nsid) {
                    return Err(DENIED);
                }
                if c == 0
                    || b >= g.sectors
                    || c > g.sectors - b
                    || c > MAX_TRANSFER / u64::from(g.block_size)
                {
                    return Err(BAD_ARGUMENT);
                }
                self.ticket = Some(Ticket {
                    id: seq,
                    lba: b,
                    blocks: c as u16,
                    stage: Stage::Reserved,
                });
                put(reply, 3, seq);
            }
            COLLECT => {
                let t = self
                    .ticket
                    .as_ref()
                    .filter(|t| t.id == a && t.stage == Stage::Completed)
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
        if matches!(tag, IDENTIFY_NAMESPACE | SUBMIT_READ) {
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
                let r = self.controller.poll_checked(now, || domain.dma_clean());
                if let Err(error) = r {
                    self.failure = Some(error);
                }
                let info = self.apply(r)?.ok_or(WOULD_BLOCK)?;
                self.pending = None;
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
                        if info.sectors.checked_mul(u64::from(info.block_size)) != Some(1 << 20) {
                            self.failure = Some(DeviceError::Geometry);
                            return Err(REFUSED);
                        }
                        self.geometry = Some(info);
                    }
                    Command::CreateSq => self.active = true,
                    Command::Read { .. } => {
                        let t = self
                            .ticket
                            .as_mut()
                            .filter(|t| t.stage == Stage::Submitted)
                            .ok_or(DENIED)?;
                        t.stage = Stage::Completed;
                        self.completed += 1;
                        put(reply, 3, info.bytes as u64);
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
                    SUBMIT_READ => {
                        let t = self
                            .ticket
                            .as_ref()
                            .filter(|t| t.id == args[0] && t.stage == Stage::Reserved)
                            .ok_or(DENIED)?;
                        Command::Read {
                            nsid: NSID as u32,
                            lba: t.lba,
                            blocks: t.blocks,
                        }
                    }
                    _ => return Err(DENIED),
                };
                let r = self.controller.submit(cmd, now);
                self.apply(r)?;
                self.pending = Some(cmd);
                if tag == SUBMIT_READ {
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
        let Some((tag, [epoch, seq, a, b, c])) = s.ipc.nvme_request(pid, cfg[BROKER_SEND])? else {
            return Ok(false);
        };
        if seq == 0 || seq <= self.seq[role] {
            return Err(BAD_ARGUMENT);
        }
        self.seq[role] = seq;
        let mut reply = [0u8; RESPONSE_SIZE];
        put(&mut reply, 0, cfg[EPOCH]);
        put(&mut reply, 1, seq);
        let mut result = if epoch != cfg[EPOCH] || self.revoked || self.failure.is_some() {
            Err(DENIED)
        } else if tag != POLL && !domain.dma_clean() {
            self.failure = Some(DeviceError::DmaFault);
            Err(REFUSED)
        } else if role == 0 {
            self.driver(tag, [a, b, c], now, domain, &mut reply)
        } else {
            self.block(tag, seq, [a, b, c], &mut reply)
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
