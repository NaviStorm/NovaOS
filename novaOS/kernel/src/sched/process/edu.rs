//! Supervision du profil matériel EDU. Tous les changements du bail et les
//! files d'IRQ sont sérialisés par la garde interprocesseurs de l'ordonnanceur.
use super::*;
use crate::hardware::{discovery::EduBar, lab_irq as irq};
#[allow(dead_code)]
#[path = "../../../../user/edu_protocol.rs"]
mod wire;
use wire::*;

const DRIVER: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/edu.bin"));
const MANAGER: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/devices.bin"));
const DEADLINE: u64 = 4 * HZ as u64;

#[derive(Clone, Copy)]
pub(super) struct Lease {
    pid: Pid,
    manager: Pid,
    epoch: u64,
    command: u64,
    response: u64,
    interrupt: u64,
    closing: bool,
    bound: bool,
    mapped: bool,
}

// Le propriétaire est un PID de génération unique. La notification contient
// le numéro de l'occurrence et exige un acquittement de la même incarnation.
pub(crate) fn lab_irq_notice(epoch: u64, sequence: u64) -> bool {
    with(|s| {
        let Some(g) = s.processes.edu else {
            return false;
        };
        if g.epoch != epoch || g.closing {
            return false;
        }
        let result = s.ipc.lab_notify(g.pid, g.interrupt, sequence);
        s.flush_ipc();
        result
    })
}

fn retire(s: &mut Core) -> bool {
    let Some(mut g) = s.processes.edu else {
        return true;
    };
    g.closing = true;
    s.processes.edu = Some(g);
    s.ipc.lab_close_sender(g.pid, g.command);
    s.ipc.lab_close(g.pid, g.response);
    s.ipc.lab_close(g.pid, g.interrupt);
    s.flush_ipc();
    if g.mapped {
        let Some(space) = s
            .processes
            .slots
            .get(g.pid.slot)
            .and_then(Option::as_ref)
            .filter(|p| p.pid == g.pid)
            .and_then(|p| p.space.as_ref())
        else {
            return false;
        };
        if heap::with_pages(|m| space.revoke_lab_mmio(m, g.epoch)).is_err() {
            return false;
        }
        g.mapped = false;
        s.processes.edu = Some(g);
    }
    if g.bound && !irq::retire(g.epoch) {
        return false;
    }
    s.processes.edu = None;
    true
}

static LAST_DEATH_PENDING: AtomicU64 = AtomicU64::new(0);
pub(super) fn died(s: &mut Core, pid: Pid) {
    if s.processes
        .edu
        .is_some_and(|g| g.pid == pid && !g.closing && irq::sequence(g.epoch).is_some())
    {
        LAST_DEATH_PENDING.store(pid.number, Ordering::Release);
    }
    if s.processes
        .edu
        .is_some_and(|g| g.pid == pid || g.manager == pid)
    {
        retire(s);
    }
}

/// Le parent ne peut jamais libérer la pile d'un thread exécuté ailleurs.
/// La cible d'une échéance de laboratoire est arrêtée sur son propre CPU au
/// retour d'IRQ utilisateur. Le PID ne peut désigner une incarnation future.
static STOP: AtomicU64 = AtomicU64::new(0);
pub(crate) fn stop_if_requested() {
    let n = STOP.load(Ordering::Acquire);
    if n != 0
        && with(|s| {
            s.thread(s.current())
                .owner
                .is_some_and(|pid| pid.number == n)
        })
    {
        STOP.store(0, Ordering::Release);
        let level = if cfg!(target_arch = "x86_64") {
            "Ring3"
        } else {
            "EL0"
        };
        finish(
            Outcome::Fault(Fault {
                trap: "SUPERVISION",
                access: "calcul",
                cause: "delai",
                address: 0,
                syndrome: 0,
            }),
            0,
            level,
        );
    }
}
fn data(s: &Core, pid: Pid, i: usize) -> u64 {
    heap::with_pages(|m| Ok(s.processes.get(pid).space.as_ref().unwrap().read_data(m, i))).unwrap()
}
fn set(s: &Core, pid: Pid, i: usize, v: u64) {
    heap::with_pages(|m| {
        s.processes
            .get(pid)
            .space
            .as_ref()
            .unwrap()
            .write_data(m, i, v);
        Ok(())
    })
    .unwrap()
}
fn wait_for(mut f: impl FnMut() -> bool) -> bool {
    let start = ticks();
    while !f() {
        if ticks().wrapping_sub(start) >= DEADLINE {
            return false;
        }
        sleep(1);
    }
    true
}
fn bind(s: &mut Core, pid: Pid, manager: Pid, bar: EduBar, epoch: u64) -> bool {
    if s.processes.edu.is_some() {
        return false;
    }
    s.processes.edu = Some(Lease {
        pid,
        manager,
        epoch,
        command: 0,
        response: 0,
        interrupt: 0,
        closing: false,
        bound: false,
        mapped: false,
    });
    let result = (|| {
        let cmd = s.ipc.lab_sender(pid)?;
        s.processes.edu.as_mut().unwrap().command = cmd;
        let reply = s.ipc.lab_receiver(pid)?;
        s.processes.edu.as_mut().unwrap().response = reply;
        let event = s.ipc.lab_receiver(pid)?;
        s.processes.edu.as_mut().unwrap().interrupt = event;
        if !irq::bind(epoch) {
            return Err(crate::abi::REFUSED);
        }
        s.processes.edu.as_mut().unwrap().bound = true;
        let space = s.processes.get(pid).space.as_ref().unwrap();
        let mapping = heap::with_pages(|m| space.map_lab_mmio(m, bar.base, bar.size, epoch));
        if mapping.is_ok() || mapping == Err(crate::memory::Error::Verification) {
            s.processes.edu.as_mut().unwrap().mapped = true;
        }
        mapping.map_err(|_| crate::abi::REFUSED)?;
        for (i, v) in [(2, epoch), (3, cmd), (4, reply), (5, event)] {
            set(s, pid, i, v);
        }
        Ok(())
    })();
    if result.is_err() {
        retire(s);
    }
    result.is_ok()
}
fn dispatch_command(s: &mut Core) -> bool {
    let Some(g) = s.processes.edu else {
        return true;
    };
    if g.closing {
        return true;
    }
    let request = match s.ipc.lab_pop(g.pid, g.command) {
        Ok(Some(r)) => r,
        Ok(None) => return true,
        Err(_) => return false,
    };
    let (op, epoch, value) = request;
    let ok = epoch == g.epoch
        && match (op, value) {
            (LIVE, n) if n <= u32::MAX as u64 => irq::live(epoch, n as u32) == Some(!(n as u32)),
            (MASK, 0) => irq::mask(epoch),
            (UNMASK, 0) => irq::unmask(epoch),
            (RAISE, 0) => irq::raise(epoch),
            (ACK, seq) => irq::sequence(epoch) == Some(seq) && irq::acknowledge(epoch, seq),
            _ => false,
        };
    let sent = s
        .ipc
        .lab_notify(g.pid, g.response, if ok { ACCEPTED } else { REJECTED });
    s.flush_ipc();
    sent
}
fn notify(pid: Pid, key: u64, bits: u64) -> bool {
    with(|s| {
        let ok = s.ipc.lab_notify(pid, key, bits);
        s.flush_ipc();
        ok
    })
}
fn await_manager(pid: Pid, key: u64, expected: u64) -> bool {
    let mut good = false;
    wait_for(|| {
        with(|s| match s.ipc.lab_pop(pid, key) {
            Ok(Some((tag, 0, 0))) => {
                good = tag == expected;
                true
            }
            Ok(None) => false,
            _ => true,
        })
    }) && good
}
struct Session {
    driver: Child,
    manager: Child,
    cmd: u64,
    reply: u64,
}
fn stage(mode: u64, previous: u64) -> Option<Session> {
    let _gate = Guard::new();
    let driver = create_image(DRIVER, &mode.to_le_bytes(), None, None).ok()?;
    let p = driver.0;
    let offer = [OFFER_MAGIC, u64::from(BDF), 0x11e8_1234, p.number];
    let mut bytes = [0; 32];
    for (word, x) in bytes.chunks_exact_mut(8).zip(offer) {
        word.copy_from_slice(&x.to_le_bytes());
    }
    let manager = match create_image(MANAGER, &[], Some(&bytes), None) {
        Ok(c) => c,
        Err(_) => {
            ipc::undo_unpublished(driver);
            return None;
        }
    };
    let m = manager.0;
    let handles = with(|s| {
        let cmd = s.ipc.lab_sender(m)?;
        let reply = s.ipc.lab_receiver(m)?;
        set(s, m, 0, cmd);
        set(s, m, 1, reply);
        set(s, p, 7, previous);
        let thread = s
            .slots
            .iter_mut()
            .flatten()
            .find(|t| t.owner == Some(p))
            .unwrap();
        thread.cpu_mask = 1 << (crate::smp::count() - 1);
        // Le TCB attend la décision du gestionnaire. Aucun retour utilisateur
        // avant attribution ou témoin explicite de démarrage refusé.
        let id = thread.id.slot;
        thread.state = State::Blocked(Wait::EduGrant(p.number));
        for _ in 0..s.ready.len {
            let queued = s.ready.pop().unwrap();
            if queued != id {
                s.ready.push(queued);
            }
        }
        Ok::<_, u64>((cmd, reply))
    });
    match handles {
        Ok((cmd, reply)) => Some(Session {
            driver,
            manager,
            cmd,
            reply,
        }),
        Err(_) => {
            with(|s| {
                s.ipc.death(m, ticks());
                s.ipc.collect();
            });
            ipc::undo_unpublished(manager);
            ipc::undo_unpublished(driver);
            None
        }
    }
}
fn name(mode: u64) -> &'static str {
    match mode {
        NORMAL => "normal",
        REFUSE => "refus",
        WRITE_RO => "ecriture",
        OUTSIDE => "hors-page",
        REVOKE => "retrait",
        PENDING => "pendante",
        SILENT => "muet",
        ACK_FIRST => "acquit-avant",
        REVOKE_FIRST => "retrait-avant",
        IRQ_FAULT => "mort-irq",
        IRQ_DURING_WAIT => "irq-attente",
        _ => unreachable!(),
    }
}
fn expected(mode: u64, out: Outcome) -> bool {
    match (mode, out) {
        (NORMAL | IRQ_DURING_WAIT, Outcome::Exit(0)) => true,
        (REFUSE, Outcome::Exit(code)) => code == 100 + crate::abi::REFUSED,
        (WRITE_RO, Outcome::Fault(f)) => {
            f.access == "ecriture" && f.cause == "protection" && f.address == PAGE as u64 + 4
        }
        (OUTSIDE | IRQ_FAULT | REVOKE | PENDING | ACK_FIRST | REVOKE_FIRST, Outcome::Fault(f)) => {
            f.access == "lecture"
                && f.cause == "absente"
                && f.address
                    == PAGE as u64
                        + if matches!(mode, OUTSIDE | IRQ_FAULT) {
                            4096
                        } else {
                            0
                        }
        }
        (SILENT, Outcome::Fault(f)) => f.trap == "SUPERVISION" && f.cause == "delai",
        _ => false,
    }
}
fn release_start(s: &mut Core, pid: Pid) -> bool {
    let wait = Wait::EduGrant(pid.number);
    if s.slots
        .iter()
        .flatten()
        .any(|t| t.owner == Some(pid) && t.state == State::Blocked(wait))
    {
        set(s, pid, 1, 1);
        return s.signal(wait).is_some();
    }
    false
}
fn abort_actor(child: &Child) -> bool {
    if child.try_result().is_none() {
        STOP.store(child.0.number, Ordering::Release);
        with(|s| release_start(s, child.0));
        if !wait_for(|| child.try_result().is_some()) {
            return false;
        }
        let _ = STOP.compare_exchange(child.0.number, 0, Ordering::AcqRel, Ordering::Acquire);
    }
    true
}
fn sample(bar: EduBar, mode: u64, previous: &mut u64) -> bool {
    let Some(session) = stage(mode, *previous) else {
        return false;
    };
    let ok = exercise(&session, bar, mode, previous);
    if !ok {
        let retired = wait_for(|| with(retire));
        let driver_stopped = abort_actor(&session.driver);
        let manager_stopped = abort_actor(&session.manager);
        let clean = retired && driver_stopped && manager_stopped;
        hal::log(format_args!("PILOTE abandon acteurs_collectes={clean}"));
    }
    ok
}
fn exercise(session: &Session, bar: EduBar, mode: u64, previous: &mut u64) -> bool {
    let Session {
        driver,
        manager,
        cmd,
        reply,
    } = session;
    let (cmd, reply) = (*cmd, *reply);
    let p = driver.0;
    let m = manager.0;
    if !await_manager(m, cmd, MANAGER_READY) {
        return false;
    }
    // La sélection utilisateur est une demande. Le catalogue de ce profil fixe
    // seul le BDF, l'image embarquée, la BAR bornée et l'unique propriétaire.
    if mode == REFUSE {
        if !with(|s| release_start(s, p)) {
            return false;
        }
        return wait_for(|| driver.try_result().is_some())
            && driver.try_result().is_some_and(|r| expected(mode, r))
            && notify(m, reply, FAILED)
            && wait_for(|| manager.try_result().is_some())
            && manager.try_result() == Some(Outcome::Exit(0));
    }
    let ready = with(|s| {
        if !bind(s, p, m, bar, p.number) {
            return false;
        }
        let other = s.processes.get(m).space.as_ref().unwrap();
        let a = heap::with_pages(|mem| other.map_lab_mmio(mem, bar.base, bar.size, p.number + 1));
        let b = heap::with_pages(|mem| other.revoke_lab_mmio(mem, p.number));
        let owner = s.processes.get(p).space.as_ref().unwrap();
        let stale = heap::with_pages(|mem| owner.revoke_lab_mmio(mem, p.number - 1));
        if a != Err(crate::memory::Error::Occupied)
            || b != Err(crate::memory::Error::Reserved)
            || stale != Err(crate::memory::Error::Reserved)
        {
            return false;
        }
        *previous = s.processes.edu.unwrap().command;
        true
    });
    if mode == NORMAL && ready {
        let probe = crate::hardware::physical_dma::Group {
            unit: 0,
            group: BDF,
        };
        if crate::hardware::physical_dma::physical_domain(probe, p.number).is_ok() {
            return false;
        }
        hal::log(format_args!("PILOTE DMA physique refuse avant demarrage"));
    }
    if !ready || !notify(m, reply, STARTED) || !await_manager(m, cmd, MANAGER_RUNNING) {
        return false;
    }
    if !with(|s| release_start(s, p)) {
        return false;
    }
    let race = matches!(mode, ACK_FIRST | REVOKE_FIRST);
    let mut request_ok = true;

    let mut external_action_done = false;
    let mut silent_since = None;
    let deadline = ticks();
    loop {
        if driver.try_result().is_some() {
            break;
        }
        if ticks().wrapping_sub(deadline) > DEADLINE {
            return false;
        }
        request_ok = with(|s| {
            if s.processes.get(p).outcome.is_some() {
                return true;
            }
            // Quand l'IRQ est pendante, retenir la requête ACK jusqu'à ce que le
            // client ait confirmé son dépôt. Exercer les deux ordres sous la garde réelle.
            if mode == IRQ_DURING_WAIT
                && !external_action_done
                && data(s, p, 6) == 1
                && s.slots
                    .iter()
                    .flatten()
                    .any(|t| t.owner == Some(p) && matches!(t.state, State::Blocked(Wait::Ipc(_))))
            {
                if !irq::raise(p.number) {
                    return false;
                }
                hal::log(format_args!(
                    "PILOTE IRQ levee receveur bloque pid={}",
                    p.number
                ));
                external_action_done = true;
            }
            let hold_ack = race && irq::sequence(p.number) == Some(1);
            if !hold_ack && !dispatch_command(s) {
                return false;
            }
            if matches!(mode, REVOKE | PENDING | SILENT | ACK_FIRST | REVOKE_FIRST)
                && !external_action_done
                && data(s, p, 6) == 1
            {
                if mode == SILENT
                    && ticks().wrapping_sub(*silent_since.get_or_insert(ticks())) < HZ as u64 / 5
                {
                    return true;
                }
                if race && !s.processes.edu.is_some_and(|g| g.closing) {
                    if !s.ipc.lab_ack_queued(p, *previous, ACK, p.number, 1)
                        || irq::sequence(p.number) != Some(1)
                    {
                        return false;
                    }
                    if mode == ACK_FIRST
                        && (!dispatch_command(s) || irq::sequence(p.number).is_some())
                    {
                        return false;
                    }
                    hal::log(format_args!(
                        "PILOTE course {} acquit_en_file=true acquit_execute={}",
                        name(mode),
                        mode == ACK_FIRST
                    ));
                }
                if mode == PENDING
                    && !s.processes.edu.is_some_and(|g| g.closing)
                    && irq::sequence(p.number) != Some(1)
                {
                    return false;
                }
                let cpu = s
                    .slots
                    .iter()
                    .flatten()
                    .find(|t| t.owner == Some(p))
                    .unwrap()
                    .last_cpu;
                if mode == REVOKE {
                    hal::log(format_args!(
                        "PILOTE retrait cpu={} pilote_cpu={cpu}",
                        crate::smp::index()
                    ));
                }
                if retire(s) {
                    if irq::acknowledge(p.number, 1) || irq::unmask(p.number) {
                        return false;
                    }
                    if mode == PENDING || race {
                        set(s, p, 8, 1);
                    }
                    if mode == SILENT {
                        STOP.store(p.number, Ordering::Release);
                    }
                    external_action_done = true;
                }
            }
            true
        });
        if !request_ok {
            break;
        }
        sleep(1);
    }
    let outcome = driver.try_result();
    if !request_ok || !outcome.is_some_and(|r| expected(mode, r)) {
        hal::log(format_args!(
            "PILOTE issue {} {outcome:?} commandes={request_ok}",
            name(mode)
        ));
        return false;
    }
    if mode == IRQ_FAULT {
        if LAST_DEATH_PENDING.load(Ordering::Acquire) != p.number {
            return false;
        }
        hal::log(format_args!(
            "PILOTE mort avec IRQ non acquittee pid={}",
            p.number
        ));
    }
    if !wait_for(|| with(retire))
        || !notify(m, reply, STOPPED)
        || !await_manager(m, cmd, MANAGER_RETIRED)
        || !notify(m, reply, CONFIRMED)
        || !wait_for(|| manager.try_result().is_some())
        || manager.try_result() != Some(Outcome::Exit(0))
    {
        return false;
    }

    true
}

pub(crate) fn run(bar: EduBar) -> bool {
    if set_affinity(1).is_err() {
        let closed = irq::shutdown();
        hal::log(format_args!("PILOTE affinite refusee fermeture={closed}"));
        return false;
    }
    let before = heap::audit(true);
    if !irq::prepare(bar.base) {
        let closed = irq::shutdown();
        hal::log(format_args!(
            "PILOTE preparation IRQ refusee fermeture={closed}"
        ));
        return false;
    }
    let mut last = 0;
    for case in [
        NORMAL,
        REFUSE,
        WRITE_RO,
        OUTSIDE,
        REVOKE,
        PENDING,
        SILENT,
        ACK_FIRST,
        REVOKE_FIRST,
        IRQ_FAULT,
        IRQ_DURING_WAIT,
    ] {
        let mut good = sample(bar, case, &mut last);
        if good && case == NORMAL {
            good = irq::probe_unbound();
            if good {
                hal::log(format_args!(
                    "PILOTE IRQ tardive sans destinataire confirmee"
                ));
            }
        }
        hal::log(format_args!("PILOTE cas={} ok={good}", name(case)));
        if !good {
            let closed = wait_for(|| with(retire)) && irq::shutdown();
            hal::log(format_args!("PILOTE abandon fermeture={closed}"));
            return false;
        }
    }
    if !irq::shutdown() {
        return false;
    }
    reap();
    let pages = with(|s| s.ipc.pages());
    let returned = heap::audit(true) == before;
    hal::log(format_args!(
        "PILOTE ressources ipc={pages} cadres_restitues={returned}"
    ));
    returned && pages == 0
}
