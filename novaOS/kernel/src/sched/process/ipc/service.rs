//! Autorité de lancement accordée au gestionnaire natif de cette session.
//! Le noyau possède les références de processus. Les politiques et le graphe
//! restent dans l’espace utilisateur. Chaque capacité IPC désigne une seule paire.
use super::*;
#[allow(dead_code)]
#[path = "../../../../../abi/service_process.rs"]
mod wire;

const MANAGER: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/service_manager.bin"));
const UTILITY: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/system_service.bin"));
const STORAGE: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/storage-managed.bin"));
const JOURNAL: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/journal_service.bin"));
const JOURNAL_ID: u64 = 16;
fn journal_profile(profile: u64) -> bool {
    matches!(profile, 41 | 42 | 77 | 87 | 89)
}
const INSTALLATION: u64 = 0x5345_5256_0000_0000;
const STOP_LIMIT: u64 = 15_000;

#[derive(Clone, Copy)]
pub(in crate::sched::process) struct Session {
    pub(in crate::sched::process) manager: Pid,
    client: Pid,
    fs: Pid,
    relay: Pid,
    administration: u64,
    lab: bool,
    pub(in crate::sched::process) profile: u64,
    stopping: bool,
    pub(in crate::sched::process) purge_error: Option<u64>,
}

pub(in crate::sched::process) fn owns_family(s: &Core, pid: Pid) -> bool {
    s.processes
        .manager
        .is_some_and(|m| m.manager == pid || s.processes.get(pid).service_owner == Some(m.manager))
}

// En exploitation, le journal du gestionnaire reçoit les sorties des services.
// L'identité d'instance couvre aussi leurs travailleurs sans leur donner de droits.
pub(in crate::sched::process) fn uses_private_journal(s: &Core, instance: Instance) -> bool {
    s.processes.manager.is_some_and(|m| !m.lab)
        && s.processes.instances.app(instance)
            .is_some_and(|app| (INSTALLATION..=INSTALLATION + JOURNAL_ID).contains(&app))
}

fn image(elf: &'static [u8], stable: u64) -> Res<Child> {
    let space = heap::with_pages(|m| UserSpace::new(m, elf, &[], None))
        .map_err(|e| launch_error(Error::Memory(e)))?;
    publish_image(space, None, INSTALLATION + stable, None).map_err(launch_error)
}

fn config(s: &Core, pid: Pid, offset: usize, words: &[u64]) {
    let space = s.processes.get(pid).space.as_ref().unwrap();
    heap::with_pages(|m| {
        for (i, v) in words.iter().enumerate() {
            space.write_data(m, offset + i, *v)
        }
        Ok(())
    })
    .unwrap()
}

// Une racine par direction. Les droits de dérivation ne sont nécessaires que
// pendant le raccordement, sous la garde et avant l'entrée des deux acteurs.
fn channel(s: &mut Core, owner: Pid, peer: Option<Pid>, direction: u32) -> Res<(u64, u64)> {
    let root = s.ipc.create(owner)?;
    let opposite = if direction == SEND { RECEIVE } else { SEND };
    let limited = if let Some(peer) = peer {
        match s.ipc.derive(owner, root, peer, opposite.into()) {
            Ok(child) => child,
            Err(e) => {
                let node = s.ipc.lookup(owner, root, MANAGE)?;
                s.ipc.close(node);
                return Err(e);
            }
        }
    } else {
        0
    };
    let node = s.ipc.lookup(owner, root, MANAGE)?;
    s.ipc.nodes.get_mut(node.id.slot()).rights = direction;
    Ok((root, limited))
}

pub(in crate::sched::process) fn launch_session(
    images: [(&'static [u8], u32); 4],
    key: u64,
    profile: u64,
    volume: Option<[u8; 16]>,
    early_ms: [u64; 2],
) -> Res<([Child; 4], Child)> {
    let private = crate::hardware::block::private_frames().map_err(|_| DENIED)?;
    let guard = Guard::new();
    if with(|s| s.processes.manager.is_some()) {
        return Err(REFUSED);
    }
    let manager = image(MANAGER, 0)?;
    let group = match publish_block_group(images, key, profile, volume, &private) {
        Ok(group) => group,
        Err(code) => {
            retire_unpublished(manager);
            return Err(code);
        }
    };
    let owner = manager.0;
    let client = group[3].0;
    let lab = matches!(profile, 75 | 76);
    let setup = with(|s| {
        if matches!(profile, 78..=86) {
            config(s, group[1].0, 13, &[profile]);
        }
        let admin = (lab || matches!(profile, 77..=89)).then_some(client);
        let (rx, admin_tx) = channel(s, owner, admin, RECEIVE)?;
        let (tx, admin_rx) = channel(s, owner, admin, SEND)?;
        let ((read_rx, query_tx), (read_tx, query_rx)) = if profile == 88 {
            // La sonde conserve la consultation après révocation des mutations.
            // Répartir les racines sans relever le quota de quatre par processus.
            let (query_tx, read_rx) = channel(s, client, Some(owner), SEND)?;
            let (query_rx, read_tx) = channel(s, client, Some(owner), RECEIVE)?;
            ((read_rx, query_tx), (read_tx, query_rx))
        } else {
            (channel(s, owner, Some(client), RECEIVE)?, channel(s, owner, Some(client), SEND)?)
        };
        config(s, owner, 0, &[rx, tx, if matches!(profile, 78..=86) { 2 } else { u64::from(lab) }, read_rx, read_tx]);
        config(
            s,
            client,
            11,
            &[
                query_tx,
                query_rx,
                if matches!(profile, 78..=86 | 88) { profile } else { u64::from(profile == 76) },
                admin_tx,
                admin_rx,
            ],
        );
        // Les profils historiques de sonde conservent leur câblage. Le droit
        // de configuration est distinct du port d'exploitation du profil 77.
        if matches!(profile, 41 | 42 | 77 | 87 | 88 | 89) {
            let fs = group[1].0;
            // Les racines suivent la durée de vie du fournisseur P9.
            let (read, requests) = channel(s, fs, Some(owner), RECEIVE)?;
            let (write, replies) = channel(s, fs, Some(owner), SEND)?;
            config(s, owner, 5, &[requests, replies]);
            config(s, fs, 14, &[read, write]);
        }
        if matches!(profile, 87 | 89) {
            // La fin du client de maintenance retire ses deux racines.
            let (send, rx) = channel(s, client, Some(owner), SEND)?;
            let (receive, tx) = channel(s, client, Some(owner), RECEIVE)?;
            config(s, owner, 7, &[rx, tx]);
            config(s, client, 16, &[send, receive]);
        } else if profile == 88 {
            let (rx, send) = channel(s, owner, Some(client), RECEIVE)?;
            let (tx, receive) = channel(s, owner, Some(client), SEND)?;
            // Le gestionnaire de la sonde peut révoquer les enfants de ces
            // deux seules racines. La sonde cliente conserve les droits minimaux.
            for (root, direction) in [(rx, RECEIVE), (tx, SEND)] {
                let node = s.ipc.lookup(owner, root, direction)?;
                s.ipc.nodes.get_mut(node.id.slot()).rights |= MANAGE;
            }
            config(s, owner, 7, &[rx, tx, profile]);
            config(s, client, 16, &[send, receive]);
        }
        config(s, owner, 10, &[u64::from(journal_profile(profile)), early_ms[0], early_ms[1]]);
        s.processes.manager = Some(Session {
            fs: group[1].0,
            relay: group[0].0,
            manager: owner,
            client,
            administration: rx,
            lab,
            profile,
            stopping: false,
            purge_error: None,
        });
        tmp::new_lease(s, owner)?;
        Ok(())
    });
    if let Err(e) = setup {
        with(|s| s.processes.manager = None);
        retire_unpublished(manager);
        for c in group {
            retire_unpublished(c);
        }
        return Err(e);
    }
    drop(guard);
    measure("services_admis");
    Ok((group, manager))
}

fn owner(s: &Core) -> Pid {
    s.thread(s.current()).owner.unwrap()
}

fn authority(s: &Core, pid: Pid) -> Res<Session> {
    s.processes
        .manager
        .filter(|m| m.manager == pid && s.processes.get(pid).outcome.is_none())
        .ok_or(DENIED)
}

fn frame(s: &Core, pid: Pid, address: u64) -> Res<[u64; 16]> {
    let space = s.processes.get(pid).space.as_ref().ok_or(CLOSED)?;
    let mut data = [0; wire::FRAME];
    let valid = heap::with_pages(|m| {
        Ok(space.writable(m, address, data.len()) && space.copy_from_user(m, address, &mut data))
    })
    .unwrap();
    if !valid {
        return Err(BAD_ADDRESS);
    }
    Ok(core::array::from_fn(|i| {
        u64::from_le_bytes(data[i * 8..i * 8 + 8].try_into().unwrap())
    }))
}

fn reply(s: &Core, pid: Pid, address: u64, words: &[u64; 16]) -> Res<()> {
    let mut data = [0; wire::FRAME];
    for (chunk, value) in data.chunks_exact_mut(8).zip(words) {
        chunk.copy_from_slice(&value.to_le_bytes())
    }
    let space = s.processes.get(pid).space.as_ref().ok_or(CLOSED)?;
    heap::with_pages(|m| Ok(space.copy_to_user(m, address, &data)))
        .unwrap()
        .then_some(())
        .ok_or(BAD_ADDRESS)
}

fn start(pid: Pid, m: Session, address: u64, mut words: [u64; 16]) -> Res<u64> {
    let id = words[0];
    let argument = words[3];
    let attempt = words[4];
    if words[1..3] != [0, 0] || words[5..].iter().any(|v| *v != 0) || attempt == 0 {
        return Err(BAD_ARGUMENT);
    }
    let elf = match id {
        1 | 2 if argument == 0 => UTILITY,
        9 if argument == 0 => STORAGE,
        JOURNAL_ID if journal_profile(m.profile) && argument == 0 => JOURNAL,
        10 if argument == 0 => UTILITY,
        11..=15 if matches!(m.profile, 78..=86) && argument == 0 => UTILITY,
        3..=8 if m.lab && argument <= 6 => UTILITY,
        _ => return Err(DENIED),
    };
    with(|s| {
        if m.stopping || !s.processes.temporary.online() {
            return Err(CLOSED);
        }
        if s.processes
            .slots
            .iter()
            .flatten()
            .any(|p| p.service_owner == Some(pid) && p.service_id == id)
        {
            return Err(REFUSED);
        }
        Ok(())
    })?;
    let child = image(elf, id)?;
    let target = child.0;
    let result = with(|s| {
        let (events, receive) = channel(s, target, Some(pid), SEND)?;
        let (commands, send) = channel(s, target, Some(pid), RECEIVE)?;
        let identity = s
            .processes
            .get(target)
            .instance
            .identity(crate::boot_id::current())
            .ok_or(REFUSED)?;
        s.ipc.locals.get_mut(target.slot).bootstrap = commands;
        s.processes.get_mut(target).service_owner = Some(pid);
        s.processes.get_mut(target).service_id = id;
        // L'image de vérification du tmp peut déléguer à un travailleur de sa
        // propre incarnation. Aucune autorité du terminal ni du volume public.
        if id == 2 {
            s.processes.get_mut(target).worker_image = Some(elf);
        }
        config(s, target, 0, &[events, commands, id, argument, attempt]);
        if !matches!(id, 10 | JOURNAL_ID) {
            tmp::new_lease(s, target)?;
        }
        words[1] = receive;
        words[2] = send;
        words[4] = target.number;
        for (i, b) in identity.encode().chunks_exact(8).enumerate() {
            words[5 + i] = u64::from_le_bytes(b.try_into().unwrap());
        }
        reply(s, pid, address, &words)?;
        if id == JOURNAL_ID {
            connect_journal(s, m, target)?;
            #[cfg(feature = "journal-pannes-lab")]
            journal_memory(s, target, "lancement");
        }
        Ok(WaitRight { target }.key())
    });
    match result {
        Ok(handle) => {
            core::mem::forget(child);
            Ok(handle)
        }
        Err(status) => {
            retire_unpublished(child);
            Err(status)
        }
    }
}

#[cfg(feature = "journal-pannes-lab")]
fn journal_memory(s: &Core, pid: Pid, phase: &str) {
    if let Some(space) = s.processes.get(pid).space.as_ref() {
        let (data, stack) = space.page_counts();
        hal::log(format_args!("JOURNAL RAM {phase} pid={} privees={data} pile={stack} octets_page=4096", pid.number));
    }
}

// Une racine privée sans droit d'usage est imputée au relais minimal, qui a
// déjà démarré. Une dérivation par direction est remise à chaque extrémité.
// Le terminal conserve ses deux places pour les commandes ELF déléguées.
fn journal_channel(s: &mut Core, owner: Pid, sender: Pid, receiver: Pid) -> Res<(Node,u64,u64)> {
    let key = s.ipc.create(owner)?;
    let root = s.ipc.lookup(owner, key, MANAGE)?;
    let grants = s.ipc.derive(owner, key, sender, SEND.into()).and_then(|tx|
        s.ipc.derive(owner, key, receiver, RECEIVE.into()).map(|rx| (tx, rx)));
    match grants {
        Ok((tx,rx)) => {
            s.ipc.nodes.get_mut(root.id.slot()).rights = 0;
            Ok((root,tx,rx))
        }
        Err(e) => { s.ipc.close(root); Err(e) }
    }
}

fn connect_journal(s: &mut Core, m: Session, target: Pid) -> Res<()> {
    let mut ids = [0; 8];
    for (i, pid) in [m.client, m.manager].into_iter().enumerate() {
        let proc = s.processes.slots[pid.slot].as_ref()
            .filter(|p| p.pid == pid && p.outcome.is_none() && p.space.is_some()).ok_or(CLOSED)?;
        let id = proc.instance.identity(crate::boot_id::current()).ok_or(REFUSED)?.encode();
        for (j, field) in id.chunks_exact(8).enumerate() {
            ids[4*i+j] = u64::from_le_bytes(field.try_into().unwrap());
        }
    }
    for peer in [m.fs, m.relay] {
        if !s.processes.slots[peer.slot].as_ref().is_some_and(|p|
            p.pid == peer && p.outcome.is_none() && p.space.is_some()) {
            return Err(CLOSED);
        }
    }
    let (store_tx, fs_rx) = channel(s, target, Some(m.fs), SEND)?;
    let (store_rx, fs_tx) = channel(s, target, Some(m.fs), RECEIVE)?;
    let (root_tx, cli_tx, journal_rx) = journal_channel(s, m.relay, m.client, target)?;
    let (root_rx, journal_tx, cli_rx) = match journal_channel(s, m.relay, target, m.client) {
        Ok(pair) => pair,
        Err(e) => { s.ipc.close(root_tx); return Err(e); }
    };
    s.processes.get_mut(target).launch_grants = LaunchGrants([root_tx.id, root_rx.id]);
    let rights = if m.profile == 89 { 63 } else { 1 };
    config(s, target, 5, &[store_tx, store_rx, journal_rx, journal_tx, rights]);
    config(s, target, 10, &ids);
    config(s, target, 18, &[ticks().saturating_mul(1000) / u64::from(HZ)]);
    config(s, m.fs, 16, &[fs_rx, fs_tx]);
    config(s, m.client, 18, &[cli_tx, cli_rx]);
    Ok(())
}

fn collect_or_stop(op: u64, pid: Pid, address: u64, mut words: [u64; 16]) -> Res<u64> {
    if words[1..].iter().any(|v| *v != 0) {
        return Err(BAD_ARGUMENT);
    }
    reap();
    with(|s| {
        let child = s
            .processes
            .slots
            .iter()
            .flatten()
            .find(|p| p.service_owner == Some(pid) && WaitRight { target: p.pid }.key() == words[0])
            .ok_or(REVOKED)?;
        let target = child.pid;
        if op == wire::KILL {
            if child.outcome.is_some() {
                return Err(CLOSED);
            }
            s.processes.get_mut(target).service_forced = true;
            stop_process(s, target, Outcome::Exit(125), hal::timer::counter());
            return Ok(0);
        }
        if child.space.is_some()
            || s.processes.instances.app(child.instance).is_some()
            || s.processes
                .temporary
                .has_service_lease(Some(child.instance))
        {
            return Err(WOULD_BLOCK);
        }
        let (kind, code) = match child.outcome.ok_or(WOULD_BLOCK)? {
            Outcome::Exit(code) => (if child.service_forced { 2 } else { 0 }, code),
            Outcome::Fault(_) => (1, 0),
        };
        words[1] = code;
        reply(s, pid, address, &words)?;
        s.processes.get_mut(target).service_owner = None;
        s.processes.release_observer(target);
        Ok(kind)
    })
}

fn begin_drain(s: &mut Core, session: Session) -> Res<()> {
    if session.stopping {
        return Ok(());
    }
    if let Ok(n) = s
        .ipc
        .lookup(session.manager, session.administration, RECEIVE)
    {
        s.ipc.add_notification(n.endpoint, wire::SHUTDOWN_NOTICE)?;
        s.ipc.signal_heads();
        s.flush_ipc();
    }
    s.processes.manager.as_mut().unwrap().stopping = true;
    Ok(())
}

fn drain(pid: Pid) -> Res<u64> {
    let deadline = ticks().saturating_add(STOP_LIMIT);
    with(|s| {
        let m = s
            .processes
            .manager
            .filter(|m| m.client == pid)
            .ok_or(DENIED)?;
        begin_drain(s, m)
    })?;
    loop {
        reap();
        if with(|s| -> Res<bool> {
            let m = s
                .processes
                .manager
                .filter(|m| m.client == pid)
                .ok_or(CLOSED)?;
            if let Some(code) = m.purge_error {
                return Err(code);
            }
            if !s.processes.temporary.online() {
                return Err(CLOSED);
            }
            Ok(s.processes.slots[m.manager.slot]
                .as_ref()
                .is_none_or(|p| p.pid != m.manager || p.space.is_none())
                && !s
                    .processes
                    .instances
                    .contains_app(INSTALLATION..=INSTALLATION + JOURNAL_ID)
                && !s.processes.temporary.has_service_lease(None)
                && s.processes.cache.drained())
        })? {
            with(|s| {
                if s.processes.manager.is_some_and(|m| matches!(m.profile, 78..=86)) {
                    hal::log(format_args!("CACHE memoire courtier pic={} final=0 octets", s.processes.cache.peak()));
                }
            });
            measure("services_draines");
            return Ok(0);
        }
        if ticks() >= deadline {
            return Err(TIMED_OUT);
        }
        sleep(1);
    }
}

pub(in crate::sched::process) fn process(op: u64, address: u64, size: u64) -> Res<u64> {
    let pid = with(|s| owner(s));
    if op == wire::DRAIN && address == 0 && size == 0 {
        return drain(pid);
    }
    let _guard = Guard::new();
    let manager = with(|s| authority(s, pid))?;
    if op == wire::NOW && (address, size) == (0, 0) {
        return Ok(ticks().saturating_mul(1000) / u64::from(HZ));
    }
    if size != wire::FRAME as u64 || address % 8 != 0 {
        return Err(BAD_ARGUMENT);
    }
    let words = with(|s| frame(s, pid, address))?;
    match op {
        wire::START => start(pid, manager, address, words),
        wire::POLL | wire::KILL => collect_or_stop(op, pid, address, words),
        _ => Err(BAD_ARGUMENT),
    }
}

// Un gestionnaire disparu ne laisse aucune autorité de suivi ou famille.
// L’arrêt du client initial demande le même drainage que la fin volontaire.
pub(in crate::sched::process) fn retired(s: &mut Core, pid: Pid, now: u64) {
    let Some(m) = s.processes.manager else {
        return;
    };
    if s.processes.get(pid).service_owner == Some(m.manager)
        && s.processes.get(pid).service_id == JOURNAL_ID {
        #[cfg(feature = "journal-pannes-lab")]
        journal_memory(s, pid, "retrait");
        for (peer, slot) in [(m.fs, 16), (m.client, 18)] {
            if s.processes.slots[peer.slot].as_ref().is_some_and(|p|
                p.pid == peer && p.space.is_some() && p.outcome.is_none()) {
                config(s, peer, slot, &[0, 0]);
            }
        }
    }
    if m.client == pid {
        if let Some(target) = s.processes.slots.iter().flatten().find(|p|
            p.service_owner == Some(m.manager) && p.service_id == JOURNAL_ID).map(|p| p.pid) {
            withdraw_grants(s, target);
        }
        let _ = begin_drain(s, m);
    }
    if m.manager != pid {
        return;
    }
    for i in 0..MAX {
        let Some(child) = s.processes.slots[i].as_ref() else {
            continue;
        };
        if child.service_owner != Some(pid) {
            continue;
        }
        let target = child.pid;
        let living = child.outcome.is_none();
        if living {
            // Le retrait du journal reconnaît son propriétaire pour mettre à zéro
            // les ports P9 et nsh, y compris lors d'un arrêt de toute la famille.
            stop_process(s, target, Outcome::Exit(125), now);
        }
        s.processes.get_mut(target).service_owner = None;
        s.processes.release_observer(target);
    }
}

pub(in crate::sched::process) fn finish_session(manager: Child) {
    let deadline = ticks().saturating_add(STOP_LIMIT);
    loop {
        if manager.try_result().is_some() {
            break;
        }
        if ticks() >= deadline {
            with(|s| {
                if s.processes.get(manager.0).outcome.is_none() {
                    stop_process(s, manager.0, Outcome::Exit(125), hal::timer::counter())
                }
            });
            break;
        }
        sleep(1);
    }
    let result = manager.join();
    with(|s| s.processes.manager = None);
    hal::log(format_args!("SERVICES fin gestionnaire {result:?}"));
}
