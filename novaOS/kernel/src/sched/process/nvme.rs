//! Supervision du profil expérimental NVMe. Aucun accès disque classique ici.
use super::*;
use crate::{
    abi,
    hardware::{
        nvme::Controller,
        nvme_domain::{Admission, Domain},
    },
};
mod broker;
pub(in crate::sched::process) mod profile;
#[allow(dead_code)]
#[path = "../../../../user/nvme_protocol.rs"]
pub(super) mod wire;
use broker::Broker;

const IMAGES: [&[u8]; 3] = [
    include_bytes!(concat!(env!("OUT_DIR"), "/nvme_driver.bin")),
    include_bytes!(concat!(env!("OUT_DIR"), "/nvme_block.bin")),
    include_bytes!(concat!(env!("OUT_DIR"), "/nvme_client.bin")),
];
pub(super) struct Peers {
    pub(super) pids: [Pid; 3],
    pub(super) cfg: [[u64; 11]; 3],
    pub(super) app: Option<Pid>,
    pub(super) app_notice: u64,
    // Canaux client↔NovaFS et extrémité de la page IPC détenue par NovaFS.
    pub(super) file_links: [u64; 5],
}
impl Peers {
    fn file_authorized(&self, s: &Core) -> bool {
        use abi::{RECEIVE, SEND};
        let Some(app) = self.app else { return true };
        let media = self.pids[2];
        let [request, response, input, output, page] = self.file_links;
        s.processes.get(app).outcome.is_none()
            && [
                (app, request, SEND),
                (app, response, RECEIVE),
                (media, input, RECEIVE),
                (media, output, SEND),
                (media, page, SEND | RECEIVE),
                (self.pids[1], self.cfg[1][wire::PAGE_LANE], SEND | RECEIVE),
            ]
            .iter()
            .all(|&(p, cap, rights)| s.ipc.nvme_live(p, cap, rights))
    }
    // Contrôle sous la même garde que l'écriture du doorbell et la copie de
    // résultat. Une mort ou fermeture de n'importe quelle extrémité le précède ou le suit.
    fn authorized(&self, s: &Core) -> bool {
        use abi::{RECEIVE, SEND};
        use wire::*;
        let paths = [
            (0, BROKER_SEND, SEND),
            (0, BROKER_REPLY, RECEIVE),
            (1, BROKER_SEND, SEND),
            (1, BROKER_REPLY, RECEIVE),
            (0, DRIVER_REQUEST, RECEIVE),
            (1, DRIVER_REQUEST, SEND),
            (0, DRIVER_REPLY, SEND),
            (1, DRIVER_REPLY, RECEIVE),
            (1, FRONT_REQUEST, RECEIVE),
            (2, FRONT_REQUEST, SEND),
            (1, FRONT_REPLY, SEND),
            (2, FRONT_REPLY, RECEIVE),
        ];
        self.pids
            .iter()
            .all(|&p| s.processes.get(p).outcome.is_none())
            && paths
                .iter()
                .all(|&(r, k, rights)| s.ipc.nvme_live(self.pids[r], self.cfg[r][k], rights))
    }
}
const PERMISSION_TEST: u64 = 5;
const READ_PERMISSION_TEST: u64 = 6;
pub(crate) fn mode(s: &[u8]) -> Option<u64> {
    match s {
        b"nvme-permission" => Some(PERMISSION_TEST),
        b"nvme-permission-read" => Some(READ_PERMISSION_TEST),
        b"nvme-lecture" => Some(wire::NORMAL),
        b"nvme-mort" => Some(wire::DRIVER_DEATH),
        b"nvme-client" => Some(wire::CLIENT_DEATH),
        b"nvme-retrait" => Some(wire::REVOKE),
        b"nvme-delai" => Some(wire::TIMEOUT_TEST),
        _ => profile::parse(s),
    }
}
fn now_ms() -> u64 {
    ticks().saturating_mul(1000) / u64::from(HZ)
}

/// La demande postérieure au retrait vient du véritable pair Ring 3. Le
/// résultat est lu dans la même réponse que celle effectivement notifiée.
fn verify_withdrawn(
    peers: &Peers,
    b: &mut Broker,
    domain: &Domain,
    role: usize,
    op: u64,
    ticket: u64,
) -> Result<u64, &'static str> {
    let pid = peers.pids[role];
    let start = ticks();
    while ticks().wrapping_sub(start) < u64::from(HZ) * 3 {
        let reply = with(|s| {
            let Some(front) = s.ipc.nvme_front(pid, peers.cfg[role][wire::BROKER_SEND])? else {
                return Ok(None);
            };
            if front != (op, ticket) || !b.step(s, peers, domain, role, now_ms())? {
                return Err(abi::BAD_ARGUMENT);
            }
            let space = s.processes.get(pid).space.as_ref().ok_or(abi::DENIED)?;
            let mut bytes = [0; 24];
            let copied =
                heap::with_pages(|m| Ok(space.copy_from_user(m, wire::RESPONSE_VA, &mut bytes)))
                    .unwrap();
            if !copied || wire::word(&bytes, 0) != peers.cfg[role][wire::EPOCH] {
                return Err(abi::BAD_ARGUMENT);
            }
            Ok(Some(wire::word(&bytes, 2)))
        })
        .map_err(|_: u64| "demande apres retrait incorrecte")?;
        if let Some(status) = reply {
            return Ok(status);
        }
        sleep(1);
    }
    Err("absence de demande apres retrait")
}

fn supervise(
    peers: &Peers,
    b: &mut Broker,
    mode: u64,
    domain: &mut Domain,
) -> Result<(), &'static str> {
    let mut last_progress = ticks();
    let mut last_completed = 0;
    let mut deny_read = mode == READ_PERMISSION_TEST;
    let mut migrated = false;
    let mut migration_witness = false;
    let mut await_client = false;
    let mut identify_seen = false;
    loop {
        if ticks().wrapping_sub(last_progress) > u64::from(HZ) * 35 {
            return Err("echeance supervision");
        }
        if deny_read && b.active {
            if domain.deny_read_data(&b.controller).is_err() {
                b.failure = Some(crate::hardware::nvme::Error::Fault);
                return Err("retrait experimental de la traduction DATA");
            }
            deny_read = false;
        }
        let mut progress = false;
        let admitted = with(|s| {
            if await_client {
                let live = peers
                    .app
                    .is_some_and(|a| s.processes.get(a).outcome.is_none());
                return Ok(live);
            }
            if !peers.authorized(s) {
                return Ok(false);
            }
            if mode == profile::RACE && b.active && crate::smp::count() > 1 {
                if let Some(t) = s
                    .slots
                    .iter_mut()
                    .flatten()
                    .find(|t| t.owner == Some(peers.pids[0]))
                {
                    if !migrated
                        && t.last_cpu != 0
                        && matches!(t.state, State::Blocked(Wait::Ipc(_)))
                    {
                        t.cpu_mask = 1;
                        migrated = true;
                    } else if migrated && !migration_witness && t.last_cpu == 0 {
                        migration_witness = true;
                        hal::log(format_args!(
                            "NVME RW migration pilote cpu_origine={} cpu_final=0",
                            crate::smp::count() - 1
                        ));
                    }
                }
            }
            for role in [0, 1] {
                progress |= b.step(s, peers, domain, role, now_ms())?;
                if !identify_seen
                    && matches!(mode, profile::REUSE | profile::RECOVER)
                    && matches!(
                        b.flight(),
                        Some(crate::hardware::nvme::Command::IdentifyController)
                    )
                {
                    let cid = b.controller.inflight_cid().ok_or(abi::REFUSED)?;
                    hal::log(format_args!(
                        "NVME P3 identify epoque={} cid={cid}",
                        peers.cfg[0][wire::EPOCH]
                    ));
                    identify_seen = true;
                }
                if mode == profile::CLIENT_DEATH
                    && matches!(
                        b.flight(),
                        Some(crate::hardware::nvme::Command::Write { .. })
                    )
                {
                    let app = peers.app.ok_or(abi::DENIED)?;
                    if !s.ipc.lab_notify(app, peers.app_notice, 1) {
                        return Err(abi::DENIED);
                    }
                    s.flush_ipc();
                    await_client = true;
                    hal::log(format_args!("NVME RW client notifie apres doorbell WRITE"));
                    break;
                }
                if b.failure.is_some() || b.stopped || b.revoked {
                    break;
                }
                if mode == wire::REVOKE && b.submitted == 1 {
                    s.ipc
                        .lab_close_sender(peers.pids[0], peers.cfg[0][wire::BROKER_SEND]);
                    s.flush_ipc();
                    b.revoked = true;
                    hal::log(format_args!(
                        "NVME capacite retiree apres doorbell avant collecte"
                    ));
                    break;
                }
            }
            Ok::<_, u64>(true)
        })
        .map_err(|_| "trame ou reponse IPC invalide")?;
        let point = if matches!(mode, profile::REVOKE_BEFORE | profile::REVOKE_AFTER) {
            b.withdrawal_point(mode)
        } else {
            None
        };
        let revoke = match (mode, b.flight()) {
            _ if point.is_some() => Some(wire::WRITE_CAP),
            (profile::REVOKE_WRITE, Some(crate::hardware::nvme::Command::Write { .. })) => {
                Some(wire::WRITE_CAP)
            }
            (profile::REVOKE_FLUSH, Some(crate::hardware::nvme::Command::Flush { .. }))
                if b.writes > 0 =>
            {
                Some(wire::FLUSH_CAP)
            }
            _ => None,
        };
        if let Some(slot) = revoke {
            let target = crate::smp::count() - 1;
            set_affinity(1 << target).map_err(|_| "migration pour revocation")?;
            let cpu = crate::smp::index();
            let withdrawn = with(|s| {
                let holder = peers.pids[1];
                let cap = peers.cfg[1][slot];
                s.ipc.nvme_withdraw(holder, cap);
                s.flush_ipc();
                !s.ipc.nvme_live(holder, cap, abi::SEND)
            });
            set_affinity(1).map_err(|_| "retour apres revocation")?;
            if !withdrawn || cpu != target {
                return Err("revocation non confirmee");
            }
            if let Some(ticket) = point {
                let before = mode == profile::REVOKE_BEFORE;
                let (label, role, op) = if before {
                    ("avant", 0, wire::SUBMIT_WRITE)
                } else {
                    ("apres", 1, wire::RELEASE)
                };
                let counts = (b.submitted, b.completed);
                let status = verify_withdrawn(peers, b, domain, role, op, ticket)?;
                if status != abi::DENIED
                    || counts != (b.submitted, b.completed)
                    || b.flight().is_some()
                {
                    return Err("retrait sans refus verifie");
                }
                hal::log(format_args!(
                    "NVME P3 retrait point={label} cpu={cpu} ticket={ticket} operation={op} statut={status} soumises={} terminees={}",
                    b.submitted, b.completed
                ));
            } else {
                hal::log(format_args!(
                    "NVME RW revocation slot={slot} cpu={cpu} en_vol=true"
                ));
            }
            b.revoked = true;
        }
        if b.stopped {
            if mode == profile::RACE && crate::smp::count() > 1 && !migration_witness {
                return Err("migration sans temoin");
            }
            return Ok(());
        }
        if b.failure.is_some() {
            return Err("erreur controleur");
        }
        if b.revoked || !admitted {
            return Err("retrait ou mort du titulaire");
        }
        // La création P9 lit tout le bail. Seule une E/S effectivement
        // achevée renouvelle son délai ; des appels POLL vides ne le prolongent pas.
        if mode >= profile::INIT && b.completed != last_completed {
            last_completed = b.completed;
            last_progress = ticks();
        }
        if !progress {
            sleep(1);
        }
    }
}

struct Actors {
    children: [Child; 3],
    peers: Peers,
    app: Option<Child>,
}
impl Actors {
    fn all(&self) -> impl Iterator<Item = &Child> {
        self.children.iter().chain(self.app.iter())
    }
    fn wait(&self, limit: u64) -> bool {
        let start = ticks();
        while self.all().any(|c| c.try_result().is_none()) {
            if ticks().wrapping_sub(start) >= limit {
                return false;
            }
            sleep(1);
        }
        true
    }
    fn close(&self) -> bool {
        ipc::close_block_channels(&self.peers.pids);
        if let Some(app) = self.peers.app {
            ipc::close_block_channels(&[app]);
        }
        self.wait(u64::from(HZ) * 4)
    }
}

fn session(domain: &mut Domain, mode: u64) -> Result<(), &'static str> {
    let base = crate::memory::pinned_dma_frames()
        .checked_sub(domain.witness_pages() as u64)
        .ok_or("bilan initial des temoins")?;
    let (children, peers, app) = ipc::nvme_group(IMAGES, mode).map_err(|code| {
        hal::log(format_args!("NVME lancement refuse statut={code}"));
        "creation des acteurs"
    })?;
    let actors = Actors {
        children,
        peers,
        app,
    };
    measure("nvme_session");
    let result = exercise(&actors, domain, mode, base);
    if !actors.close() {
        return Err("collecte des processus non confirmee");
    }
    drop(actors);
    reap();
    measure("nvme_apres_retrait");
    if with(|s| s.ipc.pages()) != 0 {
        return Err("pages IPC non restituees");
    }
    result
}

fn exercise(
    actors: &Actors,
    domain: &mut Domain,
    mode: u64,
    base: u64,
) -> Result<(), &'static str> {
    let peers = &actors.peers;
    let pid = peers.pids[0].number;
    let resources = domain.prepare(pid).map_err(|_| "preparation VT-d")?;
    if matches!(
        mode,
        profile::REVOKE_BEFORE | profile::REVOKE_AFTER | profile::TIMEOUT_FLUSH | profile::REUSE
    ) {
        domain.trace_owners().map_err(|_| "propriete des cadres")?;
    }
    let denied = domain.private_frames().ok_or("comptabilite des cadres")?;
    let isolated = with(|s| {
        heap::with_pages(|m| {
            Ok(peers.pids.iter().chain(peers.app.iter()).try_for_each(|p| {
                let space = s
                    .processes
                    .get(*p)
                    .space
                    .as_ref()
                    .ok_or("acteur termine avant l'audit")?;
                space
                    .audit_driver(m, &denied)
                    .then_some(())
                    .ok_or("projection utilisateur de cadres prives")
            }))
        })
        .unwrap()
    });
    if let Err(reason) = isolated {
        domain
            .cancel_prepared()
            .map_err(|_| "retrait du bail avant soumission")?;
        return Err(reason);
    }
    let frames = domain.pinned();
    hal::log(format_args!(
        "NVME isolation tables_usager={} cadres_interdits={} alias=0",
        actors.all().count(),
        denied.iter().filter(|&&pa| pa != 0).count()
    ));
    // Sûreté : le domaine conserve l'exclusivité, les pages et la BAR UC
    // pendant toute l'incarnation, y compris en cas de mise en quarantaine.
    let controller = match unsafe { Controller::prepare(resources) } {
        Ok(c) => c,
        Err(_) => {
            domain
                .cancel_prepared()
                .map_err(|_| "retrait de la preparation non confirme")?;
            return Err("registres initiaux");
        }
    };
    {
        let guard = Guard::new();
        if !with_guard(&guard, |s| peers.authorized(s)) {
            domain
                .cancel_prepared()
                .map_err(|_| "retrait de la preparation non confirme")?;
            return Err("titulaire revoque avant ouverture PCI");
        }
        domain.enable().map_err(|_| "ouverture PCI")?;
    }
    let mut b = Broker::new(controller, mode);
    hal::log(format_args!(
        "NVME acteurs pilote={pid} bloc={} client={} cpu_pilote={} cpu_bsp={} mode={mode}",
        peers.pids[1].number,
        peers.pids[2].number,
        crate::smp::count() - 1,
        crate::smp::index()
    ));
    if let Err(reason) = supervise(peers, &mut b, mode, domain) {
        // Plus aucune admission utilisateur ne sera examinée. Seules les
        // suppressions de files d'une incarnation sans DMA en cours sont permises.
        let idle = b.close_idle(domain);
        let safe = if idle {
            let proof = b
                .controller
                .drained()
                .ok_or("absence de preuve de fin apres mort")?;
            domain
                .retire(&proof)
                .map_err(|_| "retrait apres mort non confirme")?;
            domain.pinned() == 0 && crate::memory::pinned_dma_frames() == base
        } else {
            domain.quarantine();
            domain.pinned() == frames
                && crate::memory::pinned_dma_frames() == base + frames as u64
                && domain.prepare(pid).is_err()
        };
        if b.failure.is_some() || matches!(mode, profile::REVOKE_BEFORE | profile::REVOKE_AFTER) {
            actors.wait(u64::from(HZ) * 2);
        } else if mode == wire::REVOKE {
            // Laisser le statut déjà émis traverser les canaux utilisateur,
            // même après la fermeture de toute admission matérielle.
            let start = ticks();
            while actors.children[0].try_result().is_none()
                && ticks().wrapping_sub(start) < u64::from(HZ) * 2
            {
                sleep(1);
            }
        }
        if !actors.close() {
            return Err("collecte des acteurs apres incident");
        }
        let expired = b.failure == Some(crate::hardware::nvme::Error::Timeout);
        hal::log(format_args!(
            "NVME fermeture motif={reason} erreur={:?} soumises={} terminees={} expiration={expired} quarantaine={} retrait_confirme={idle}",
            b.failure,
            b.submitted,
            b.completed,
            !idle && safe
        ));
        let expected = match mode {
            profile::DMA_FAULT => {
                let intact = b
                    .frozen_source()
                    .is_some_and(|src| domain.refused_write_source(src));
                hal::log(format_args!(
                    "NVME RW faute source_verifiee={intact} ack=false"
                ));
                intact
                    && b.failure == Some(crate::hardware::nvme::Error::DmaFault)
                    && matches!(
                        b.flight(),
                        Some(crate::hardware::nvme::Command::Write { .. })
                    )
            }
            profile::REVOKE_BEFORE | profile::REVOKE_AFTER => {
                b.revoked
                    && idle
                    && b.flight().is_none()
                    && b.writes == usize::from(mode == profile::REVOKE_AFTER)
                    && actors
                        .app
                        .as_ref()
                        .is_some_and(|a| a.try_result() == Some(Outcome::Exit(abi::DENIED)))
            }
            profile::TIMEOUT_FLUSH => {
                expired
                    && !idle
                    && b.writes > 0
                    && matches!(
                        b.flight(),
                        Some(crate::hardware::nvme::Command::Flush { .. })
                    )
                    && actors
                        .app
                        .as_ref()
                        .is_some_and(|a| a.try_result() == Some(Outcome::Exit(abi::TIMED_OUT)))
            }
            profile::TIMEOUT => {
                expired
                    && matches!(
                        b.flight(),
                        Some(crate::hardware::nvme::Command::Write { .. })
                    )
            }
            profile::REVOKE_WRITE | profile::REVOKE_FLUSH => b.revoked && b.flight().is_some(),
            profile::DRIVER_DEATH => {
                actors.children[0].try_result() == Some(Outcome::Exit(0))
                    && matches!(
                        b.flight(),
                        Some(crate::hardware::nvme::Command::Write { .. })
                    )
            }
            profile::CLIENT_DEATH => actors
                .app
                .as_ref()
                .is_some_and(|a| a.try_result() == Some(Outcome::Exit(abi::CANCELLED))),
            wire::DRIVER_DEATH => {
                b.submitted == 1 && actors.children[0].try_result() == Some(Outcome::Exit(0))
            }
            wire::CLIENT_DEATH => actors.children[2].try_result() == Some(Outcome::Exit(0)),
            wire::REVOKE => {
                b.revoked
                    && b.submitted == 1
                    && actors.children[0].try_result() == Some(Outcome::Exit(0))
            }
            PERMISSION_TEST | READ_PERMISSION_TEST => {
                let (operation, expected_reads) = if mode == PERMISSION_TEST {
                    ("Identify", 0)
                } else {
                    ("READ", 1)
                };
                let intact = domain.refused_data_intact();
                hal::log(format_args!(
                    "NVME isolation {operation} destination_absente={intact} donnees_client=0"
                ));
                !idle
                    && intact
                    && b.failure == Some(crate::hardware::nvme::Error::DmaFault)
                    && b.submitted == expected_reads
            }
            wire::TIMEOUT_TEST => {
                expired
                    && b.submitted == 1
                    && actors
                        .children
                        .iter()
                        .zip([abi::TIMED_OUT, abi::TIMED_OUT, abi::OK])
                        .all(|(c, status)| c.try_result() == Some(Outcome::Exit(status)))
            }
            _ => false,
        };
        return if expected
            && safe
            && (if mode >= profile::INIT {
                b.submitted == b.completed + usize::from(b.flight().is_some())
            } else {
                b.completed == 0
            }) {
            Ok(())
        } else {
            Err(reason)
        };
    }
    let proof = b.controller.drained().ok_or("absence de preuve de fin")?;
    if domain.witness_pages() > 0 && mode == profile::RECOVER {
        domain
            .observe_readback(&proof)
            .map_err(|_| "observation des lectures du second bail")?;
    }
    let retirement = domain.retire(&proof);
    if matches!(mode, profile::QI_ERROR | profile::QI_DELAY) {
        domain.quarantine();
        let held = retirement.is_err()
            && domain.failed_invalidation()
            && domain.pinned() == frames
            && crate::memory::pinned_dma_frames() == base + frames as u64
            && domain.prepare(pid).is_err();
        hal::log(format_args!(
            "NVME RW invalidation incertaine quarantaine={held} cadres={frames}"
        ));
        return if held
            && actors.wait(u64::from(HZ) * 5)
            && actors
                .all()
                .all(|c| c.try_result() == Some(Outcome::Exit(0)))
        {
            Ok(())
        } else {
            Err("preuve de retention QI")
        };
    }
    retirement.map_err(|_| "retrait non confirme")?;
    if !actors.wait(u64::from(HZ) * 5)
        || actors
            .all()
            .any(|c| c.try_result() != Some(Outcome::Exit(0)))
    {
        return Err("statut des acteurs");
    }
    let pages = crate::memory::pinned_dma_frames();
    let ipc_pages = with(|s| s.ipc.pages());
    if mode >= profile::INIT {
        hal::log(format_args!(
            "NVME RW bilan mode={mode} write={} flush={} completions={} pages_ipc={ipc_pages}",
            b.writes, b.flushes, b.completed
        ));
        if profile::writable(mode) != (b.writes > 0 && b.flushes > 0)
            || (!profile::writable(mode) && (b.writes != 0 || b.flushes != 0))
        {
            return Err("comptage des droits de publication");
        }
    }
    hal::log(format_args!(
        "NVME fin incarnation pid={pid} soumises={} terminees={} epingles={pages} ipc_pages={ipc_pages}",
        b.submitted, b.completed
    ));
    let expected = base
        .checked_add(domain.witness_pages() as u64)
        .ok_or("bilan des temoins")?;
    if pages != expected
        || ipc_pages != 0
        || b.submitted != b.completed
        || b.submitted == 0
        || (mode < profile::INIT && b.submitted != 3)
    {
        return Err("comptabilite finale");
    }
    Ok(())
}

pub(crate) fn run(mode: u64, admission: Admission) -> bool {
    if set_affinity(1).is_err() {
        return false;
    }
    let before = crate::memory::pinned_dma_frames();
    let [rid, id, class, _bar] = admission.identity();
    let manager = include_bytes!(concat!(env!("OUT_DIR"), "/nvme_manager.bin"));
    for (allowed, expected) in [(rid + 1, abi::DENIED), (0x30, abi::OK)] {
        let words = [
            rid,
            id & 0xffff,
            id >> 16,
            class,
            allowed,
            if mode >= profile::INIT {
                wire::DRIVER_IMAGE_RW
            } else {
                wire::DRIVER_IMAGE
            } as u64,
            if mode >= profile::INIT {
                wire::DRIVER_INTENTS_RW
            } else {
                wire::DRIVER_INTENTS_RO
            } as u64,
        ];
        let mut cfg = [0u8; 56];
        for (i, v) in words.into_iter().enumerate() {
            wire::put(&mut cfg, i, v);
        }
        let Ok(actor) = spawn_image(manager, &cfg, None, None) else {
            return false;
        };
        if actor.join() != Outcome::Exit(expected) {
            return false;
        }
    }
    let mut domain = match Domain::new(admission) {
        Ok(v) => v,
        Err(e) => {
            hal::log(format_args!("NVME domaine refuse {e:?}"));
            return false;
        }
    };
    let service_pages = crate::memory::pinned_dma_frames();
    let result = (|| {
        if mode == profile::REUSE {
            domain
                .arm_reuse_probe()
                .map_err(|_| "armement du temoin de reattribution")?;
        }
        session(&mut domain, mode)?;
        if mode == profile::REUSE {
            domain
                .observe_parked_witnesses()
                .map_err(|_| "observation entre les incarnations")?;
            session(&mut domain, profile::RECOVER)?;
        }
        if mode == wire::NORMAL {
            session(&mut domain, mode)?;
        }
        if domain.pinned() == 0 {
            domain.finish().map_err(|_| "fermeture du domaine")?;
        }
        if mode == profile::REUSE {
            if domain.witness_pages() != 0 || crate::memory::pinned_dma_frames() != service_pages {
                return Err("restitution incomplete des temoins");
            }
            hal::log(format_args!("NVME P3 bilan temoins=0 dma={service_pages}"));
        }
        Ok::<_, &'static str>(())
    })();
    if let Err(reason) = result {
        domain.quarantine();
        hal::log(format_args!("NVME echec {reason}"));
    }
    hal::log(format_args!(
        "NVME ressources initial={} final={} vcpus={}",
        before,
        crate::memory::pinned_dma_frames(),
        crate::smp::count()
    ));
    result.is_ok()
}
