//! Port d'exploitation et port de lecture indépendants. Un numéro de service ne
//! donne jamais le droit noyau correspondant.
use super::*;

pub(super) fn error(e: policy::Error) -> u64 {
    use policy::Error::*;
    match e {
        Unknown | Unregistered | InvalidState => rt::REFUSED,
        Stale => rt::REVOKED,
        InvalidDeclaration | Duplicate | Cycle | MissingDependency => rt::BAD_ARGUMENT,
    }
}
pub(super) fn state(s: State) -> u64 {
    match s {
        State::Registered => p::REGISTERED,
        State::Waiting => p::WAITING,
        State::Starting => p::STARTING,
        State::Ready => p::READY,
        State::Running => p::RUNNING,
        State::Stopping => p::STOPPING,
        State::Stopped => p::STOPPED,
        State::Failed => p::FAILED,
        State::Restarting => p::RESTARTING,
    }
}

impl Manager<'_> {
    pub(super) fn report(&self, id: u64) -> Frame {
        let mut reply = p::request(rt::OK, id, 0);
        if let Ok(i) = self.index(id) {
            let unit = self.units[i];
            reply[3] = self.policy.state(id).map(state).unwrap_or(0);
            reply[4] = self.policy.attempt(id).unwrap_or(0);
            reply[5] = unit.live.pid;
            reply[6..10].copy_from_slice(&unit.live.identity);
            reply[11] = unit.authority;
            reply[12] = unit.stale;
            reply[13] = unit.admitted;
            reply[14] = unit.withdrawals;
            reply[15] = unit.waits;
        }
        reply
    }

    pub(super) fn next_tag(&mut self) -> rt::Result<u64> {
        self.serial = self.serial.checked_add(1).ok_or(rt::LIMIT)?;
        Ok(self.serial)
    }

    fn settings(&self, id: u64) -> rt::Result<Frame> {
        let active = self.manifest[self.index(id)?];
        let stored = self
            .config
            .records
            .iter()
            .find(|d| d.id == id)
            .or_else(|| (id == j::JOURNAL_ID).then_some(&active))
            .ok_or(rt::REFUSED)?;
        let mut reply = self.report(id);
        reply[4] = self.config.applied;
        reply[5] = self.policy.blocking_provider(id).unwrap_or(0);
        reply[8] = u64::from(settings::differs(&active, stored));
        reply[10..].copy_from_slice(&[
            u64::from(stored.auto),
            u64::from(active.auto),
            self.config.stored,
            settings::restart(stored.restart),
            u64::from(stored.retries),
            self.config.status,
        ]);
        if id == j::JOURNAL_ID {
            reply[6] = self.journal.lost;
            reply[7] = self.journal.uncertain;
            reply[9] = self.journal.last_error;
        }
        Ok(reply)
    }

    fn shutdown(&mut self, time: u64) {
        if self.stopping { return; }
        self.stopping = true;
        self.journal.record(0, self.journal.identity, j::INFO, j::SYSTEM, j::SHUTDOWN, time, [0,0]);
        // Drainage borné en fin de session seulement. Un journal arrêté ne
        // ralentit ni la supervision ni l'administration pendant la session.
        self.shutdown_deadline = Some(time.saturating_add(1500));
    }

    fn request(
        &mut self,
        f: Frame,
        port: usize,
        tag: u64,
        dest: u64,
        time: u64,
    ) -> rt::Result<Option<Frame>> {
        let admin = port == 0;
        let [version, op, id, arg, ..] = f;
        if version != p::VERSION {
            return Err(rt::BAD_VERSION);
        }
        if tag == 0 {
            return Err(rt::BAD_ARGUMENT);
        }
        if matches!(op, p::SETTINGS | p::ENABLE | p::DISABLE) {
            if f[4..].iter().any(|&v| v != 0) {
                return Err(rt::BAD_ARGUMENT);
            }
            if self.cache_lab || self.qualification {
                return Err(rt::REFUSED);
            }
            if op == p::SETTINGS {
                if arg != 0 {
                    return Err(rt::BAD_ARGUMENT);
                }
                // Une réponse peut disparaître du seul côté appelant. Le gestionnaire
                // ne peut inférer ce doute de son propre dernier acquittement.
                self.config.refresh();
            } else {
                if port != 2 {
                    return Err(rt::DENIED);
                }
                if self.stopping {
                    return Err(rt::CLOSED);
                }
                // Profil fermé du banc. Après admission de ENABLE CAS 1, le
                // noyau révoque les deux délégations clientes. Le gestionnaire
                // termine la publication déjà remise, sans pouvoir répondre au client.
                if rt::config(9) == 88 && op == p::ENABLE && id == 9 && arg == 1
                    && self.config.stored == 1 && self.config.status == rt::OK
                {
                    for slot in [7, 8] {
                        let removed = rt::value(rt::call(rt::IPC_REVOKE, rt::config(slot), 0, 0))?;
                        if removed != 1 { return Err(rt::REFUSED); }
                    }
                }
                let subject = self.units[self.index(id)?].live.identity;
                if let Err(e) = self.config.autostart(id, op == p::ENABLE, arg) {
                    self.journal.record(id, subject, j::ERROR, j::CONFIG, j::CONFIG_ERROR,
                        time, [e, arg]);
                    return Err(e);
                }
                self.journal.record(id, subject, j::INFO, j::CONFIG,
                    if op == p::ENABLE { j::ENABLED } else { j::DISABLED },
                    time, [self.config.stored, self.config.applied]);
            }
            return self.settings(id).map(Some);
        }
        if port == 2 {
            return Err(rt::DENIED);
        }
        if matches!(op, p::CACHE_TEST | p::CACHE_ASYNC_WRITE) {
            if !admin || !self.cache_lab || self.stopping || !(11..=15).contains(&id) {
                return Err(rt::DENIED);
            }
            if (op == p::CACHE_ASYNC_WRITE && (id != 12 || arg != cache_steps::RACE_WRITE))
                || f[5..].iter().any(|w| *w != 0)
            {
                return Err(rt::BAD_ARGUMENT);
            }
            let i = self.index(id)?;
            if !matches!(self.policy.state(id), Ok(State::Ready | State::Running)) {
                return Err(rt::REFUSED);
            }
            let live = self.units[i].live;
            let serial = self.next_tag()?;
            let mut cmd = p::request(p::CACHE_STEP, id, live.attempt);
            cmd[4..8].copy_from_slice(&live.identity);
            cmd[8..10].copy_from_slice(&[arg, f[4]]);
            rt::send_frame(live.tx, serial, &p::encode(&cmd), 0, false)?;
            if op == p::CACHE_ASYNC_WRITE {
                // L'acteur 12 effectue le véritable WRITE et en conserve le résultat.
                // L'acteur 11 reste disponible pour confirmer la remise puis purger.
                return Ok(Some(self.report(id)));
            }
            self.pending = Some(Pending {
                tag: serial,
                incoming: tag,
                destination: dest,
                instance: live,
                deadline: time.saturating_add(15_000),
            });
            return Ok(None);
        }
        if f[4..].iter().any(|&w| w != 0) {
            return Err(rt::BAD_ARGUMENT);
        }
        let read = matches!(op, p::LIST | p::STATUS | p::DEP | p::QUERY);
        if !admin && !read {
            return Err(rt::DENIED);
        }
        if self.stopping && !read && op != p::LOG {
            return Err(rt::CLOSED);
        }
        if op == p::LIST && id == 0 {
            let d = self
                .manifest
                .get(usize::try_from(arg).map_err(|_| rt::BAD_ARGUMENT)?)
                .ok_or(rt::REFUSED)?;
            let mut answer = self.report(d.id);
            answer[10] = d.id;
            return Ok(Some(answer));
        }
        if op == p::SHUTDOWN && id == 0 && arg == 0 {
            self.shutdown(time);
            return Ok(Some(self.report(0)));
        }
        if op == p::INJECT && self.qualification && id == 0 && arg == 5 {
            #[cfg(target_arch = "x86_64")]
            unsafe {
                core::arch::asm!("ud2", options(noreturn));
            }
            #[cfg(target_arch = "aarch64")]
            unsafe {
                core::arch::asm!("brk #0", options(noreturn));
            }
        }
        if op == p::LOG && id == 0 {
            let first = self.last_log.saturating_sub(LOG_COUNT as u64 - 1).max(1);
            let seq = if arg == 0 { first } else { arg };
            if seq < first {
                return Err(rt::REVOKED);
            }
            if seq > self.last_log {
                return Err(rt::WOULD_BLOCK);
            }
            return Ok(Some(self.logs[(seq as usize - 1) % LOG_COUNT]));
        }
        if self.qualification && op == p::REGISTER && arg == 0 && matches!(id, 100 | 101) {
            let (invalid, expected) = if id == 100 {
                (catalog::CYCLE, policy::Error::Cycle)
            } else {
                (catalog::MISSING, policy::Error::MissingDependency)
            };
            return match Policy::new(invalid) {
                Err(e) if e == expected => Err(rt::BAD_ARGUMENT),
                // Un succès ou une autre cause serait une régression d'admission.
                _ => Err(rt::REFUSED),
            };
        }
        let i = self.index(id)?;
        match op {
            p::STATUS if arg == 0 => {
                self.policy.state(id).map_err(error)?;
            }
            p::REGISTER if arg == 0 => self.policy.register(id).map_err(error)?,
            p::START if arg == 0 => {
                let active = self.units[i].live.right != 0;
                self.policy.start(id).map_err(error)?;
                if !active {
                    self.units[i].authority = 2;
                }
            }
            p::STOP if arg == 0 => self.policy.stop(id).map_err(error)?,
            p::RESTART if arg == 0 => {
                self.policy.restart(id).map_err(error)?;
                self.units[i].authority = 2;
            }
            p::DEP => {
                let d = self.manifest[i];
                let ordinal = usize::try_from(arg).map_err(|_| rt::BAD_ARGUMENT)?;
                let mut answer = self.report(id);
                if let Some(required) = d.required.get(ordinal) {
                    answer[10] = *required;
                    answer[11] = 1;
                } else {
                    answer[10] = *d
                        .optional
                        .get(ordinal - d.required.len())
                        .ok_or(rt::REFUSED)?;
                    answer[11] = 0;
                }
                return Ok(Some(answer));
            }
            p::QUERY | p::CACHE_MAINTAIN if arg == 0 && (op == p::QUERY || id == 10) => {
                if !matches!(self.policy.state(id), Ok(State::Ready | State::Running)) {
                    return Err(rt::REFUSED);
                }
                let live = self.units[i].live;
                let serial = self.next_tag()?;
                if id == 9 {
                    let packet = [rt::WIRE_VERSION, rt::INFO, 0, 0, 0, 0, 0, 0];
                    rt::send_frame(live.tx, serial, &packet, 0, false)?;
                } else {
                    let command = if op == p::CACHE_MAINTAIN {
                        p::MAINTAIN
                    } else {
                        p::SAMPLE
                    };
                    live.command(command, 0, serial)?;
                }
                self.pending = Some(Pending {
                    tag: serial,
                    incoming: tag,
                    destination: dest,
                    instance: live,
                    deadline: time.saturating_add(if op == p::CACHE_MAINTAIN {
                        15_000
                    } else {
                        1_000
                    }),
                });
                return Ok(None);
            }
            p::INJECT => {
                if !self.qualification || !(3..=8).contains(&id) {
                    return Err(rt::DENIED);
                }
                if id == 8 && arg == 6 {
                    let (channel, frame) = self.held.take().ok_or(rt::REFUSED)?;
                    self.release = true;
                    self.accept_ready(i, channel, &frame, time);
                } else if (1..=4).contains(&arg) && self.units[i].live.right != 0 {
                    self.units[i].live.command(p::DISTURB, arg, 1)?;
                } else {
                    return Err(rt::BAD_ARGUMENT);
                }
            }
            _ => return Err(rt::BAD_ARGUMENT),
        }
        Ok(Some(self.report(id)))
    }

    fn control_message(&mut self, port: usize, tx: u64, msg: &rt::ReceiveV1, time: u64) {
        if msg.kind == 2 && port == 0 && msg.aux & proc::SHUTDOWN_NOTICE != 0 {
            self.shutdown(time);
            return;
        }
        if msg.kind != 1 {
            return;
        }
        let frame = p::decode(&msg.payload[..msg.len as usize]);
        let reply = match frame {
            Some(frame) => match self.request(frame, port, msg.tag, tx, time) {
                Ok(Some(reply)) => reply,
                Ok(None) => return,
                Err(e) => p::request(e, frame[2], 0),
            },
            None => p::request(rt::BAD_ARGUMENT, 0, 0),
        };
        self.response = Some((tx, msg.tag, reply));
    }

    pub(super) fn run(&mut self, ports: [(u64, u64); 3]) -> rt::Result<()> {
        let mut first = 0;
        let mut blocked_since = None;
        loop {
            let time = now()?;
            self.services(time)?;
            self.actions(time)?;
            let logger = self.index(j::JOURNAL_ID).ok().and_then(|i| {
                matches!(self.policy.state(j::JOURNAL_ID), Ok(State::Ready | State::Running))
                    .then_some(self.units[i].live)
            });
            if let Some(live) = logger { self.journal.submit(live, time); }
            if self.shutdown_deadline.is_some_and(|end|
                self.journal.idle() || logger.is_none() || time >= end) {
                self.shutdown_deadline = None;
                self.policy.shutdown();
            }
            if self.pending.is_some_and(|q| time >= q.deadline) {
                self.queued_response(rt::TIMED_OUT, 0);
            }
            if let Some((tx, tag, reply)) = self.response {
                match rt::send_frame(tx, tag, &p::encode(&reply), 0, false) {
                    Err(rt::WOULD_BLOCK) => {
                        // La saturation d'un canal de retour ne doit pas bloquer l'autre autorité.
                        let start = *blocked_since.get_or_insert(time);
                        // Le service utilise uniquement ses capacités IPC. Il ne reçoit
                        // pas le droit THREAD_WAIT du lanceur d'applications.
                        match rt::send_for(tx, tag, &p::encode(&reply), 2) {
                            Ok(()) | Err(rt::CLOSED | rt::REVOKED | rt::DENIED) => {
                                self.response = None
                            }
                            Err(rt::TIMED_OUT) if time.saturating_sub(start) >= 250 => {
                                self.response = None
                            }
                            Err(rt::TIMED_OUT) => (),
                            Err(e) => return Err(e),
                        }
                    }
                    Ok(()) | Err(rt::DENIED | rt::CLOSED | rt::REVOKED) => {
                        self.response = None;
                    }
                    Err(e) => return Err(e),
                }
            }
            if self.response.is_none() {
                blocked_since = None;
            }
            if self.stopping
                && self.units.iter().all(|u| u.live.right == 0)
                && self.response.is_none()
            {
                return Ok(());
            }
            if self.response.is_some() {
                continue;
            }
            if let Some(q) = self.pending {
                let i = self.index(q.instance.id)?;
                self.units[i].waits = self.units[i].waits.saturating_add(1);
                match rt::receive_for(q.instance.rx, 2) {
                    Ok(message) => self.event(i, &message, now()?),
                    Err(rt::TIMED_OUT) => (),
                    Err(rt::CLOSED | rt::REVOKED | rt::DENIED) => {
                        self.queued_response(rt::CLOSED, 0)
                    }
                    Err(e) => return Err(e),
                }
                continue;
            }
            let mut served = false;
            for offset in 0..ports.len() {
                let n = (first + offset) % ports.len();
                if ports[n].0 == 0 {
                    continue;
                }
                match rt::receive_for(ports[n].0, 0) {
                    Ok(msg) => {
                        self.control_message(n, ports[n].1, &msg, time);
                        first = (n + 1) % ports.len();
                        served = true;
                        break;
                    }
                    Err(rt::WOULD_BLOCK | rt::DENIED | rt::REVOKED | rt::CLOSED) => (),
                    Err(e) => return Err(e),
                }
            }
            if !served {
                // Temps borné pour les événements sur les autres ports et les échéances du graphe.
                match rt::receive_for(ports[0].0, 2) {
                    Ok(msg) => self.control_message(0, ports[0].1, &msg, now()?),
                    Err(rt::TIMED_OUT) => (),
                    Err(rt::REVOKED | rt::CLOSED | rt::DENIED) => {
                        // La racine d'exploitation appartient au gestionnaire.
                        // Sa perte rend le contrôle impossible : retirer la famille.
                        return Err(rt::CLOSED);
                    }
                    Err(e) => return Err(e),
                }
            }
        }
    }
}
