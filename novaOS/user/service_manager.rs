//! Gestionnaire natif lancé après admission P9. La politique de services est
//! locale à cet ELF ; le noyau possède les processus, identités et dérivations IPC.
#![no_std]
#![no_main]
#[path = "services/catalog.rs"]
mod catalog;
#[path = "services/control.rs"]
mod control;
#[allow(dead_code)]
#[path = "../abi/instances.rs"]
mod instance;
#[path = "services/policy.rs"]
mod policy;
#[path = "../abi/service_process.rs"]
mod proc;
#[path = "services/protocol.rs"]
mod protocol;
#[path = "storage/runtime.rs"]
mod runtime;
use policy::{Action, Cause, Declaration, Policy, State};
use protocol::{self as p, Frame};
use runtime as rt;

const MAX: usize = policy::MAX_SERVICES;
const LOG_COUNT: usize = 32;

#[derive(Clone, Copy, Default)]
struct Binding {
    id: u64,
    attempt: u64,
    pid: u64,
    identity: [u64; 4],
    right: u64,
    rx: u64,
    tx: u64,
}
impl Binding {
    fn accepts(self, channel: u64, frame: &Frame) -> bool {
        self.right != 0
            && self.rx == channel
            && frame[0] == p::VERSION
            && frame[2] == self.id
            && frame[3] == self.attempt
            && frame[4..8] == self.identity
    }
    fn command(self, op: u64, arg: u64, tag: u64) -> rt::Result<()> {
        let mut f = p::request(op, self.id, self.attempt);
        f[4..8].copy_from_slice(&self.identity);
        f[8] = arg;
        rt::send_frame(self.tx, tag, &p::encode(&f), 0, false)
    }
}

#[derive(Clone, Copy, Default)]
struct Unit {
    live: Binding,
    previous: Binding,
    stale: u64,
    admitted: u64,
    withdrawals: u64,
    waits: u64,
    authority: u64,
    probe: u64,
}

#[derive(Clone, Copy)]
struct Pending {
    tag: u64,
    incoming: u64,
    destination: u64,
    instance: Binding,
    deadline: u64,
}
struct Manager<'a> {
    manifest: &'a [Declaration],
    policy: Policy<'a>,
    units: [Unit; MAX],
    qualification: bool,
    stopping: bool,
    delayed: Option<(u64, Frame)>,
    held: Option<(u64, Frame)>,
    release: bool,
    pending: Option<Pending>,
    serial: u64,
    response: Option<(u64, u64, Frame)>,
    logs: [Frame; LOG_COUNT],
    last_log: u64,
    storage_seen: bool,
}

fn syscall(operation: u64, frame: &mut Frame) -> rt::Result<u64> {
    rt::value(rt::call(
        instance::SERVICE_PROCESS,
        operation,
        frame.as_mut_ptr() as u64,
        proc::FRAME as u64,
    ))
}
fn now() -> rt::Result<u64> {
    rt::value(rt::call(instance::SERVICE_PROCESS, proc::NOW, 0, 0))
}

impl Manager<'_> {
    fn index(&self, id: u64) -> rt::Result<usize> {
        self.manifest
            .iter()
            .position(|d| d.id == id)
            .ok_or(rt::REFUSED)
    }

    fn spawn(&mut self, i: usize, d: Declaration, attempt: u64) -> rt::Result<()> {
        if self.units[i].live.right != 0
            || d.caps != catalog::PRIVATE_TMP
            || d.args.len() != 1
            || (d.image == catalog::STORAGE_IMAGE) != (d.id == 9)
            || !matches!(d.image, catalog::STORAGE_IMAGE | catalog::SYSTEM_IMAGE)
        {
            return Err(rt::DENIED);
        }
        let mut words = [0; 16];
        words[0] = d.id;
        words[3] = d.args[0];
        words[4] = attempt;
        let right = syscall(proc::START, &mut words)?;
        let binding = Binding {
            id: d.id,
            attempt,
            pid: words[4],
            right,
            rx: words[1],
            tx: words[2],
            identity: words[5..9].try_into().unwrap(),
        };
        let old = self.units[i].previous;
        if binding.pid == 0
            || binding.right == 0
            || binding.rx == 0
            || binding.tx == 0
            || binding.identity[..2] == [0, 0]
            || binding.identity[2..].contains(&0)
            || (old.right != 0
                && (binding.rx == old.rx
                    || binding.tx == old.tx
                    || binding.right == old.right
                    || binding.identity == old.identity))
        {
            // Un incohérent retour du lanceur entraîne le retrait du gestionnaire
            // et de toutes ses familles par son mécanisme de vie noyau.
            rt::exit(rt::REFUSED);
        }
        // La politique embarquée autorise l'auto-démarrage. L'endpoint
        // d'exploitation authentifie les décisions de lancement manuel.
        if self.units[i].authority == 0 {
            self.units[i].authority = u64::from(!d.auto) + 1;
        }
        self.units[i].live = binding;
        self.units[i].probe = 0;
        Ok(())
    }

    fn actions(&mut self, time: u64) -> rt::Result<()> {
        while let Some(action) = self.policy.next_action(time) {
            match action {
                Action::Launch { id, attempt } => {
                    let i = self.index(id)?;
                    if let Err(e) = self.spawn(i, self.manifest[i], attempt) {
                        self.policy
                            .finished(id, attempt, Cause::LaunchError, e, time)
                            .map_err(control::error)?;
                    }
                }
                Action::Stop { id, attempt } | Action::Kill { id, attempt } => {
                    let live = self.units[self.index(id)?].live;
                    if live.right == 0 || live.attempt != attempt {
                        return Err(rt::REVOKED);
                    }
                    if matches!(action, Action::Stop { .. }) {
                        // Si le destinataire a perdu son canal ou que la file est pleine,
                        // l'échéance de STOPPING déclenchera l'arrêt contraint.
                        let _ = live.command(p::HALT, 0, 1);
                    } else {
                        let mut words = [0; 16];
                        words[0] = live.right;
                        match syscall(proc::KILL, &mut words) {
                            Ok(_) | Err(rt::CLOSED) => (),
                            Err(e) => return Err(e),
                        }
                    }
                }
            }
            self.collect_logs();
        }
        self.collect_logs();
        Ok(())
    }

    fn collect_logs(&mut self) {
        while let Some(event) = self.policy.pop_event() {
            let Ok(i) = self.index(event.id) else {
                continue;
            };
            let unit = self.units[i];
            let binding = if unit.live.attempt == event.attempt {
                unit.live
            } else if unit.previous.attempt == event.attempt {
                unit.previous
            } else {
                Binding::default()
            };
            let mut line = [0; 16];
            line[0] = p::VERSION;
            line[2] = event.id;
            line[3] = control::state(event.to);
            line[4] = event.attempt;
            line[5] = binding.pid;
            line[6..10].copy_from_slice(&binding.identity);
            line[10] = control::state(event.from);
            line[11] = event.cause as u64;
            line[12] = event.exit;
            line[13] = event.dependency;
            line[14] = self.policy.dropped_events();
            let Some(n) = self.last_log.checked_add(1) else {
                return;
            };
            line[15] = n;
            self.logs[(n as usize - 1) % LOG_COUNT] = line;
            self.last_log = n;
        }
    }

    fn accept_ready(&mut self, i: usize, channel: u64, frame: &Frame, time: u64) {
        let live = self.units[i].live;
        if live.accepts(channel, frame) && self.policy.ready(live.id, live.attempt, time).is_ok() {
            self.units[i].admitted = self.units[i].admitted.saturating_add(1);
        } else {
            self.units[i].stale = self.units[i].stale.saturating_add(1);
        }
    }

    fn queued_response(&mut self, status: u64, value: u64) {
        if let Some(q) = self.pending.take() {
            let mut f = self.report(q.instance.id);
            f[1] = status;
            f[10] = value;
            self.response = Some((q.destination, q.incoming, f));
        }
    }

    fn event(&mut self, i: usize, msg: &rt::ReceiveV1, time: u64) {
        let live = self.units[i].live;
        if msg.kind != 1 {
            return;
        }
        if live.id == 9 && msg.len == 12 {
            let bytes = &msg.payload[..12];
            let valid = bytes[..8] == [rt::WIRE_VERSION, 0, 0, 0, 0, 0, 0, 0]
                && &bytes[8..] == rt::IDENTITY;
            if self.units[i].probe != 0 && self.units[i].probe == msg.tag {
                self.units[i].probe = 0;
                if valid && self.policy.ready(live.id, live.attempt, time).is_ok() {
                    self.units[i].admitted = self.units[i].admitted.saturating_add(1);
                    if !self.storage_seen {
                        self.storage_seen = true;
                        let text = b"SVC stockage INFO disponible";
                        rt::call(rt::DEBUG_WRITE, text.as_ptr() as u64, text.len() as u64, 0);
                    }
                } else {
                    let _ = self
                        .policy
                        .fault(live.id, live.attempt, Cause::Communication, time);
                }
            } else if self
                .pending
                .is_some_and(|q| q.tag == msg.tag && q.instance.right == live.right)
            {
                if valid {
                    // Sous une sollicitation continue, les vraies réponses
                    // attestent la vivacité au même titre que les battements.
                    let _ = self.policy.heartbeat(live.id, live.attempt, time);
                    self.queued_response(
                        rt::OK,
                        u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as u64,
                    );
                } else {
                    self.queued_response(rt::REFUSED, 0);
                }
            }
            return;
        }
        let Some(f) = p::decode(&msg.payload[..msg.len as usize]) else {
            return;
        };
        if !live.accepts(live.rx, &f) {
            return;
        }
        match f[1] {
            p::ANNOUNCE => {
                if live.id == 9 {
                    if self.units[i].probe == 0 && self.policy.state(9) == Ok(State::Starting) {
                        let issued = self.next_tag().and_then(|tag| {
                            rt::send_frame(
                                live.tx,
                                tag,
                                &[rt::WIRE_VERSION, rt::INFO, 0, 0, 0, 0, 0, 0],
                                0,
                                false,
                            )?;
                            Ok(tag)
                        });
                        match issued {
                            Ok(tag) => self.units[i].probe = tag,
                            Err(_) => {
                                let _ =
                                    self.policy
                                        .fault(9, live.attempt, Cause::Communication, time);
                            }
                        }
                    }
                } else if self.qualification
                    && live.id == 8
                    && live.attempt == 1
                    && self.delayed.is_none()
                {
                    // Point d'injection du retard : le message a réellement été reçu de l'ancien processus.
                    self.delayed = Some((live.rx, f));
                    let _ = live.command(p::DISTURB, 2, 1);
                } else if self.qualification && live.id == 8 && !self.release && live.attempt == 2 {
                    if let Some((old, message)) = self.delayed.take() {
                        self.accept_ready(i, old, &message, time);
                        // Éprouver séparément l'origine du message et l'identité
                        // complète, même si un numéro de tentative courant est présenté.
                        self.accept_ready(i, old, &f, time);
                        let mut old_identity = message;
                        old_identity[3] = live.attempt;
                        self.accept_ready(i, live.rx, &old_identity, time);
                    }
                    self.held = Some((live.rx, f));
                } else {
                    self.accept_ready(i, live.rx, &f, time);
                }
            }
            p::PULSE
                if !matches!(
                    self.policy.state(live.id),
                    Ok(State::Starting | State::Stopping)
                ) =>
            {
                let _ = self.policy.heartbeat(live.id, live.attempt, time);
            }
            p::LOST => {
                let _ = self
                    .policy
                    .fault(live.id, live.attempt, Cause::Capability, time);
            }
            p::VALUE
                if self
                    .pending
                    .is_some_and(|q| q.tag == msg.tag && q.instance.right == live.right) =>
            {
                let _ = self.policy.heartbeat(live.id, live.attempt, time);
                self.queued_response(f[9], f[8]);
            }
            _ => (),
        }
    }

    fn services(&mut self, time: u64) -> rt::Result<()> {
        for i in 0..self.manifest.len() {
            let live = self.units[i].live;
            if live.right == 0 {
                continue;
            }
            // La limite de 8 correspond à la capacité complète de l'endpoint ;
            // une source active ne peut monopoliser le gestionnaire.
            for _ in 0..8 {
                match rt::receive_for(live.rx, 0) {
                    Ok(msg) => self.event(i, &msg, time),
                    Err(rt::WOULD_BLOCK | rt::DENIED | rt::REVOKED | rt::CLOSED) => break,
                    Err(e) => return Err(e),
                }
            }
            let mut words = [0; 16];
            words[0] = live.right;
            let kind = match syscall(proc::POLL, &mut words) {
                Ok(kind) => kind,
                Err(rt::WOULD_BLOCK) => continue,
                Err(e) => return Err(e),
            };
            // Les deux opérations ci-dessous utilisent les anciennes capacités effectives.
            // Une entrée précédemment déposée ne doit plus être observable.
            let dead = |s| matches!(s, rt::CLOSED | rt::REVOKED | rt::DENIED);
            let rx = rt::receive_for(live.rx, 0).err().unwrap_or(rt::OK);
            let tx = rt::call(rt::IPC_NOTIFY, live.tx, 1, 0).status;
            if !dead(rx) || !dead(tx) {
                return Err(rt::REFUSED);
            }
            if self.pending.is_some_and(|q| q.instance.right == live.right) {
                self.queued_response(rt::CLOSED, 0);
            }
            self.units[i].previous = live;
            self.units[i].live = Binding::default();
            self.units[i].withdrawals = self.units[i].withdrawals.saturating_add(1);
            let cause = match kind {
                0 => Cause::Exited,
                1 => Cause::Crash,
                2 => Cause::Killed,
                _ => return Err(rt::REFUSED),
            };
            self.policy
                .finished(live.id, live.attempt, cause, words[1], time)
                .map_err(control::error)?;
            self.collect_logs();
        }
        Ok(())
    }
}

fn supervise() -> rt::Result<()> {
    let qualification = rt::config(2) == 1;
    let manifest = if qualification {
        catalog::QUALIFICATION
    } else {
        catalog::INSTALLED
    };
    let mut manager = Manager {
        manifest,
        policy: Policy::new(manifest).map_err(control::error)?,
        units: [Unit::default(); MAX],
        qualification,
        stopping: false,
        delayed: None,
        held: None,
        release: false,
        pending: None,
        serial: 2,
        response: None,
        logs: [[0; 16]; LOG_COUNT],
        last_log: 0,
        storage_seen: false,
    };
    let ports = [
        (rt::config(0), rt::config(1)),
        (rt::config(3), rt::config(4)),
    ];
    if ports.iter().any(|&(rx, tx)| rx == 0 || tx == 0) {
        return Err(rt::DENIED);
    }
    manager.run(ports)
}

#[unsafe(no_mangle)]
extern "C" fn run() -> ! {
    rt::exit(supervise().err().unwrap_or(0))
}
