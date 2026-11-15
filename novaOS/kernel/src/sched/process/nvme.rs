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
    pub(super) cfg: [[u64; 8]; 3],
}
impl Peers {
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
        _ => None,
    }
}
fn now_ms() -> u64 {
    ticks().saturating_mul(1000) / u64::from(HZ)
}

fn supervise(
    peers: &Peers,
    b: &mut Broker,
    mode: u64,
    domain: &mut Domain,
) -> Result<(), &'static str> {
    let start = ticks();
    let mut deny_read = mode == READ_PERMISSION_TEST;
    loop {
        if ticks().wrapping_sub(start) > u64::from(HZ) * 35 {
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
            if !peers.authorized(s) {
                return Ok(false);
            }
            for role in [0, 1] {
                progress |= b.step(s, peers, domain, role, now_ms())?;
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
        if b.stopped {
            return Ok(());
        }
        if b.failure.is_some() {
            return Err("erreur controleur");
        }
        if b.revoked || !admitted {
            return Err("retrait ou mort du titulaire");
        }
        if !progress {
            sleep(1);
        }
    }
}

struct Actors {
    children: [Child; 3],
    peers: Peers,
}
impl Actors {
    fn wait(&self, limit: u64) -> bool {
        let start = ticks();
        while self.children.iter().any(|c| c.try_result().is_none()) {
            if ticks().wrapping_sub(start) >= limit {
                return false;
            }
            sleep(1);
        }
        true
    }
    fn close(&self) -> bool {
        ipc::close_block_channels(&self.peers.pids);
        self.wait(u64::from(HZ) * 4)
    }
}

fn session(domain: &mut Domain, mode: u64) -> Result<(), &'static str> {
    let base = crate::memory::pinned_dma_frames();
    let (children, peers) = ipc::nvme_group(IMAGES, mode).map_err(|_| "creation des acteurs")?;
    let actors = Actors { children, peers };
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
    let denied = domain.private_frames().ok_or("comptabilite des cadres")?;
    let isolated = with(|s| {
        heap::with_pages(|m| {
            Ok(peers.pids.iter().all(|p| {
                s.processes
                    .get(*p)
                    .space
                    .as_ref()
                    .is_some_and(|space| space.audit_driver(m, &denied))
            }))
        })
        .unwrap()
    });
    if !isolated {
        domain
            .cancel_prepared()
            .map_err(|_| "retrait du bail avant soumission")?;
        return Err("projection utilisateur de cadres prives");
    }
    hal::log(format_args!(
        "NVME isolation tables_usager=3 cadres_interdits=13 alias=0"
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
    let mut b = Broker::new(controller);
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
            domain.pinned() == 5
                && crate::memory::pinned_dma_frames() == base + 5
                && domain.prepare(pid).is_err()
        };
        if b.failure.is_some() {
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
        return if expected && safe && b.completed == 0 {
            Ok(())
        } else {
            Err(reason)
        };
    }
    let proof = b.controller.drained().ok_or("absence de preuve de fin")?;
    domain.retire(&proof).map_err(|_| "retrait non confirme")?;
    if !actors.wait(u64::from(HZ) * 5)
        || actors
            .children
            .iter()
            .any(|c| c.try_result() != Some(Outcome::Exit(0)))
    {
        return Err("statut des acteurs");
    }
    let pages = crate::memory::pinned_dma_frames();
    let ipc_pages = with(|s| s.ipc.pages());
    hal::log(format_args!(
        "NVME fin incarnation pid={pid} soumises={} terminees={} epingles={pages} ipc_pages={ipc_pages}",
        b.submitted, b.completed
    ));
    if pages != base || ipc_pages != 0 || b.submitted != 3 || b.completed != 3 {
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
            wire::DRIVER_IMAGE as u64,
            wire::DRIVER_INTENTS_RO as u64,
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
    let result = (|| {
        session(&mut domain, mode)?;
        if mode == wire::NORMAL {
            session(&mut domain, mode)?;
        }
        if domain.pinned() == 0 {
            domain.finish().map_err(|_| "fermeture du domaine")?;
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
