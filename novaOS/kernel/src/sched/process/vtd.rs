//! Service privé du profil Q35 EDU. Un seul superviseur possède le matériel et
//! consomme les demandes IPC. Aucun pointeur issu d'une trame ne devient une IOVA.
use super::*;
use crate::hardware::{
    discovery::{Drhd, VtdProfile},
    edu_dma::{self, Device, Transfer},
    physical_dma::Direction::*,
    vtd_lab::{Engine, FrameStamp, Lease},
};
#[path = "../../../../user/edu_dma_protocol.rs"]
mod wire;
use wire::*;

const IMAGE: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/edu_dma.bin"));
const LIMIT: u64 = 3 * HZ as u64;
const SENTINEL: [u8; 4096] = [0xc3; 4096];
static ARMED: AtomicU64 = AtomicU64::new(0);
static DEAD_RUNNING: AtomicU64 = AtomicU64::new(0);

pub(super) fn died(p: Pid) {
    if ARMED.load(Ordering::Acquire) == p.number && edu_dma::running_at_death() {
        DEAD_RUNNING.store(p.number, Ordering::Release);
    }
}

fn until(mut predicate: impl FnMut() -> bool) -> bool {
    let start = ticks();
    while !predicate() {
        if ticks().wrapping_sub(start) >= LIMIT {
            return false;
        }
        sleep(1);
    }
    true
}
type Check<T> = core::result::Result<T, &'static str>;
fn check(value: bool, reason: &'static str) -> Check<()> {
    if value { Ok(()) } else { Err(reason) }
}

struct Client {
    child: Child,
    send: u64,
    reply: u64,
}
impl Client {
    fn new(mode: u64) -> Check<Self> {
        let _guard = Guard::new();
        let child = create_image(
            IMAGE,
            &mode.to_le_bytes(),
            (mode == WITNESS).then_some(&SENTINEL[..]),
            None,
        )
        .map_err(|_| "creation pilote")?;
        let p = child.0;
        let handles = with(|s| {
            let (cmd, reply) = if mode == REFUSED {
                (0, 0)
            } else {
                (s.ipc.lab_sender(p)?, s.ipc.lab_receiver(p)?)
            };
            heap::with_pages(|m| {
                let space = s.processes.get(p).space.as_ref().unwrap();
                for (i, v) in [(1, p.number), (2, cmd), (3, reply)] {
                    space.write_data(m, i, v);
                }
                Ok(())
            })
            .map_err(|_| crate::abi::NO_MEMORY)?;
            s.slots
                .iter_mut()
                .flatten()
                .find(|t| t.owner == Some(p))
                .unwrap()
                .cpu_mask = 1 << (crate::smp::count() - 1);
            Ok::<_, u64>((cmd, reply))
        });
        match handles {
            Ok((send, reply)) => Ok(Self { child, send, reply }),
            Err(_) => {
                with(|s| {
                    s.ipc.death(p, ticks());
                    s.ipc.collect();
                });
                ipc::undo_unpublished(child);
                Err("allocation IPC")
            }
        }
    }
    fn pid(&self) -> u64 {
        self.child.0.number
    }
    /// Linéarise une opération privilégiée avec la mort du titulaire et la
    /// clôture de son endpoint. L'action ne dort pas. Le drainage se fait après.
    fn while_authorized<T>(&self, action: impl FnOnce() -> T) -> Option<T> {
        let guard = Guard::new();
        let live = with_guard(&guard, |s| {
            s.processes.get(self.child.0).outcome.is_none()
                && s.ipc.lab_can_submit(self.child.0, self.send)
        });
        live.then(action)
    }
    fn next(&self) -> Check<(u64, u64, u64)> {
        let mut got = None;
        let ok = until(|| {
            with(|s| match s.ipc.lab_pop(self.child.0, self.send) {
                Ok(Some(v)) => {
                    got = Some(v);
                    true
                }
                Ok(None) => false,
                Err(_) => true,
            })
        });
        check(ok, "delai IPC")?;
        got.ok_or("endpoint de commande ferme")
    }
    fn request(&self, tag: u64, owner: u64) -> Check<()> {
        check(self.next()? == (tag, owner, 0), "requete IPC")
    }
    fn say(&self, result: u64) -> Check<()> {
        check(
            with(|s| {
                let ok = s.ipc.lab_notify(self.child.0, self.reply, result);
                s.flush_ipc();
                ok
            }),
            "reponse IPC",
        )
    }
    fn block(&self) {
        with(|s| {
            s.ipc.lab_close_sender(self.child.0, self.send);
            s.flush_ipc();
        });
    }
    fn finish(&self) -> Check<()> {
        check(
            until(|| self.child.try_result().is_some())
                && self.child.try_result() == Some(Outcome::Exit(0)),
            "sortie Ring3",
        )
    }
    fn witness(&self) -> Check<user::PrivateWitness> {
        with(|s| {
            heap::with_pages(|m| {
                s.processes
                    .get(self.child.0)
                    .space
                    .as_ref()
                    .and_then(|space| space.vtd_witness(m))
                    .ok_or(crate::memory::Error::Reserved)
            })
        })
        .map_err(|_| "page temoin")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Case {
    Normal,
    WarmWrite,
    WrongWrite,
    WrongRead,
    Unmapped,
    Other,
    Revoke,
    Death,
    Late,
    Restart,
    Receipt,
}
impl Case {
    fn mode(self) -> u64 {
        match self {
            Self::Normal | Self::Restart => NORMAL,
            Self::Revoke | Self::Receipt => REVOKE,
            Self::Death => DEATH,
            Self::Late => LATE,
            _ => FAULT_PROBE,
        }
    }
}

static STATION: crate::sched::sync::Mutex<Option<Bench>> = crate::sched::sync::Mutex::new(None);

struct Bench {
    hw: Device,
    engine: Engine,
    reused: bool,
    previous: Option<(Lease, [FrameStamp; 2])>,
    current: Option<Lease>,
    witness: Option<user::PrivateWitness>,
}
impl Bench {
    fn run_io(&mut self, c: &Client, lease: Lease, op: Transfer) -> Check<()> {
        check(
            c.while_authorized(|| self.hw.submit(&self.engine, lease, op)) == Some(true),
            "soumission EDU",
        )?;
        check(until(|| self.hw.completed(lease)), "fin EDU")?;
        check(!self.engine.fault_pending(), "faute DMA inattendue")?;
        if op == Transfer::ReadSource {
            check(
                self.hw.confirm_source(&self.engine, lease),
                "lecture sans faute",
            )?;
        }
        Ok(())
    }
    fn prepare(&mut self, c: &Client, pattern: u8) -> Check<Lease> {
        let id = c.pid();
        c.request(PREPARE, id)?;
        let lease = c
            .while_authorized(|| self.prepare_domain(id, pattern))
            .ok_or("titulaire retire")??;
        self.run_io(c, lease, Transfer::ReadSource)?;
        c.say(ACCEPTED)?;
        Ok(lease)
    }
    fn prepare_domain(&mut self, id: u64, pattern: u8) -> Check<Lease> {
        if self.previous.is_none() {
            let trial = self.engine.attach(id).map_err(|_| "preparation initiale")?;
            self.engine
                .map(trial, 0, ReadByDevice)
                .map_err(|_| "page non publiee")?;
            check(
                self.engine.map(trial, 2, ReadByDevice).is_err(),
                "mapping invalide",
            )?;
            self.engine
                .cancel_preparation(trial)
                .map_err(|_| "annulation avant publication")?;
            check(
                self.engine.parked()
                    && self.engine.pinned() == 0
                    && self.engine.publish(trial).is_err(),
                "bail abandonne",
            )?;
            hal::log(format_args!("VTD preparation annulee aucun cadre publie"));
        }
        let lease = self.engine.attach(id).map_err(|_| "attach")?;
        self.current = Some(lease);
        check(
            lease.owner() == id && self.engine.map(lease, 2, WrittenByDevice).is_err(),
            "borne de slot",
        )?;
        self.engine
            .map(lease, 0, ReadByDevice)
            .map_err(|_| "map source")?;
        check(
            self.engine.map(lease, 0, WrittenByDevice).is_err(),
            "double mapping",
        )?;
        self.engine
            .map(lease, 1, WrittenByDevice)
            .map_err(|_| "map destination")?;
        check(
            self.engine.matches(lease, 0, 0) && self.engine.matches(lease, 1, 0),
            "pages neuves non nulles",
        )?;
        let stamps = [
            self.engine.frame_stamp(lease, 0).ok_or("cadre source")?,
            self.engine
                .frame_stamp(lease, 1)
                .ok_or("cadre destination")?,
        ];
        if let Some((old, prev)) = self.previous {
            check(
                old.epoch() < lease.epoch()
                    && old.iova(0) != lease.iova(0)
                    && self.engine.map(old, 0, ReadByDevice).is_err()
                    && !self.engine.active(old),
                "ancienne generation",
            )?;
            self.reused |= prev.iter().any(|v| stamps.contains(v));
        }
        self.previous = Some((lease, stamps));
        self.engine
            .fill(lease, 0, pattern)
            .map_err(|_| "remplissage source")?;
        self.engine
            .fill(lease, 1, !pattern)
            .map_err(|_| "remplissage destination")?;
        self.engine
            .publish(lease)
            .map_err(|_| "publication contexte")?;
        check(
            self.engine.cancel_preparation(lease).is_err(),
            "annulation apres publication",
        )?;
        check(self.hw.open(&self.engine, lease), "activation BME")?;
        Ok(lease)
    }
    fn competitor(&mut self, lease: Lease) -> Check<()> {
        let c = Client::new(CONTENDER)?;
        c.request(PREPARE, c.pid())?;
        check(
            self.engine.attach(c.pid()).is_err()
                && self.engine.active(lease)
                && self.engine.pinned() == 2,
            "attribution concurrente",
        )?;
        c.say(REJECTED)?;
        c.finish()?;
        hal::log(format_args!(
            "VTD concurrent pid={} titulaire={} refus=true",
            c.pid(),
            lease.owner()
        ));
        Ok(())
    }
    fn retire(&mut self, lease: Lease, delayed: bool) -> Check<()> {
        self.engine.close(lease).map_err(|_| "fermeture domaine")?;
        check(
            !self.hw.submit(&self.engine, lease, Transfer::ReadSource),
            "soumission apres cloture",
        )?;
        let before = crate::memory::pinned_dma_frames();
        check(until(|| self.hw.drain(lease).is_some()), "drain EDU et BME")?;
        let quiet = self.hw.drain(lease).ok_or("relecture drainage")?;
        if delayed {
            check(
                self.engine.retire_delayed(lease, &quiet).is_err()
                    && self.engine.pinned() == 2
                    && crate::memory::pinned_dma_frames() == before
                    && self.engine.attach(lease.owner()).is_err(),
                "quarantaine WAIT",
            )?;
            hal::log(format_args!(
                "VTD quarantaine WAIT masque cadres_epingles=2 nouvelles_admissions=0"
            ));
        }
        self.engine
            .retire(lease, &quiet)
            .map_err(|_| "invalidation confirmee")?;
        check(
            self.engine.parked()
                && self.engine.pinned() == 0
                && crate::memory::pinned_dma_frames().checked_add(2) == Some(before),
            "restitution unique",
        )?;
        check(
            self.engine.retire(lease, &quiet).is_err()
                && crate::memory::pinned_dma_frames().checked_add(2) == Some(before),
            "double restitution",
        )?;
        self.current = None;
        Ok(())
    }
    fn fault(&mut self, lease: Lease, iova: u64, read: bool, active: bool) -> Check<()> {
        let mut entry = None;
        let mut failed = false;
        let ready = until(|| match self.engine.take_fault(lease) {
            Ok(None) => false,
            Ok(Some(f)) => {
                entry = Some(f);
                true
            }
            Err(_) => {
                failed = true;
                true
            }
        });
        check(!failed, "registre de faute incoherent")?;
        let f = entry.ok_or("FRCD ou IRQ absente")?;
        check(
            ready
                && f.sid == 48
                && f.iova == iova
                && f.read == read
                && f.did == active.then_some(1)
                && f.status & 2 != 0
                && f.irqs != 0,
            "identite FRCD",
        )?;
        hal::log(format_args!(
            "VTD faute sid={} did={:?} iova={:#x} lecture={} raison={} statut={:#x} irq={}",
            f.sid, f.did, f.iova, f.read, f.reason, f.status, f.irqs
        ));
        Ok(())
    }
    fn sample(&mut self, case: Case) -> Check<()> {
        let c = Client::new(case.mode())?;
        let id = c.pid();
        let pattern = (id as u8).wrapping_mul(7).wrapping_add(0x38);
        let lease = self.prepare(&c, pattern)?;
        match case {
            Case::Normal | Case::Restart => {
                if case == Case::Normal {
                    self.competitor(lease)?;
                }
                let mut counts = [0; 3];
                // Le client choisit la commande. L'identité de l'endpoint et le
                // bail, détenu par le courtier, bornent toutes les opérations.
                for _ in 0..5 {
                    let (tag, generation, reserved) = c.next()?;
                    let authenticated = generation == id
                        && lease.owner() == id
                        && self.engine.active(lease)
                        && reserved == 0;
                    let result = match tag {
                        READ if authenticated => {
                            self.run_io(&c, lease, Transfer::ReadSource)?;
                            counts[0] += 1;
                            ACCEPTED
                        }
                        WRITE if authenticated => {
                            self.run_io(&c, lease, Transfer::WriteDestination)?;
                            check(
                                self.engine.matches(lease, 0, pattern)
                                    && self.engine.matches(lease, 1, pattern),
                                "copie de 4096 octets",
                            )?;
                            counts[1] += 1;
                            ACCEPTED
                        }
                        _ => {
                            counts[2] += 1;
                            REJECTED
                        }
                    };
                    c.say(result)?;
                }
                check(counts == [1, 1, 3], "couverture des commandes Ring3")?;
                c.finish()?;
                self.retire(lease, false)?;
                if case == Case::Normal {
                    check(
                        self.hw.post_revoke(&self.engine, lease),
                        "sonde contexte retire",
                    )?;
                    self.fault(lease, lease.iova(1), false, false)?;
                }
            }
            Case::Late => {
                c.request(READ, id)?;
                c.say(ACCEPTED)?; // extraction seulement, le pilote sort avant soumission
                c.finish()?;
                check(
                    self.engine.active(lease)
                        && c.while_authorized(|| {
                            self.hw.submit(&self.engine, lease, Transfer::ReadSource)
                        })
                        .is_none()
                        && !self.hw.running(lease),
                    "commande tardive apres mort",
                )?;
                hal::log(format_args!("VTD demande tardive refusee avant doorbell"));
                self.retire(lease, false)?;
            }
            Case::Revoke | Case::Death | Case::Receipt => {
                c.request(START_INFLIGHT, id)?;
                if case == Case::Death {
                    ARMED.store(id, Ordering::Release);
                }
                check(
                    c.while_authorized(|| {
                        self.hw.submit(&self.engine, lease, Transfer::ReadSource)
                            && self.hw.running(lease)
                    }) == Some(true),
                    "admission en vol",
                )?;
                c.say(ACCEPTED)?;
                if case == Case::Death {
                    c.finish()?;
                    ARMED.store(0, Ordering::Release);
                    check(
                        DEAD_RUNNING.load(Ordering::Acquire) == id,
                        "mort pendant RUN",
                    )?;
                } else {
                    c.request(RETIRE_READY, id)?;
                    c.block();
                    check(
                        c.while_authorized(|| ()).is_none(),
                        "endpoint revoque encore actif",
                    )?;
                }
                self.engine.close(lease).map_err(|_| "cloture en vol")?;
                check(
                    self.hw.running(lease)
                        && self.hw.drain(lease).is_none()
                        && self.engine.pinned() == 2,
                    "retrait premature",
                )?;
                let remote = crate::smp::count() - 1;
                set_affinity(1 << remote).map_err(|_| "migration superviseur")?;
                self.retire(lease, case == Case::Receipt)?;
                let cpu = crate::smp::index();
                check(cpu == remote, "CPU invalidation")?;
                hal::log(format_args!(
                    "VTD retrait en_vol=true mort={} depart_cpu=0 invalidation_cpu={cpu}",
                    case == Case::Death
                ));
                set_affinity(1).map_err(|_| "retour BSP")?;
                if case != Case::Death {
                    c.say(DRAINED)?;
                    c.finish()?;
                }
            }
            _ => {
                c.request(PROBE_READY, id)?;
                let witness = if case == Case::Other {
                    let w = Client::new(WITNESS)?;
                    w.request(PROBE_READY, w.pid())?;
                    Some(w)
                } else {
                    None
                };
                // Le tampon EDU contient toujours pattern ; la cible RO reçoit un
                // autre motif CPU pour rendre observable une éventuelle écriture.
                let expected_source = if matches!(case, Case::WrongWrite | Case::WarmWrite) {
                    let distinct = pattern ^ 0xa5;
                    check(
                        self.engine.replace_read_source(lease, distinct, &self.hw),
                        "separation motifs RO",
                    )?;
                    distinct
                } else {
                    pattern
                };
                if case == Case::WrongWrite {
                    self.engine
                        .resync(lease)
                        .map_err(|_| "invalidation avant essai a froid")?;
                }
                if let Some(w) = witness.as_ref() {
                    check(pattern != SENTINEL[0], "motifs distincts temoin")?;
                    self.witness = Some(w.witness()?);
                }
                let (submitted, address, read) = c
                    .while_authorized(|| match case {
                        Case::WrongRead => (
                            self.hw
                                .submit(&self.engine, lease, Transfer::ReadDestination),
                            lease.iova(1),
                            true,
                        ),
                        Case::WarmWrite | Case::WrongWrite => (
                            self.hw.submit(&self.engine, lease, Transfer::WriteSource),
                            lease.iova(0),
                            false,
                        ),
                        Case::Unmapped => (
                            self.hw.submit(&self.engine, lease, Transfer::WriteUnmapped),
                            lease.iova(2),
                            false,
                        ),
                        Case::Other => {
                            let w = self.witness.as_ref().unwrap();
                            (self.hw.probe_other(&self.engine, lease, w), w.iova(), false)
                        }
                        _ => unreachable!(),
                    })
                    .ok_or("sonde sans titulaire")?;
                check(
                    submitted && until(|| self.hw.completed(lease)),
                    "fin sonde de permissions",
                )?;
                let intact = self.engine.matches(lease, 0, expected_source)
                    && self.engine.matches(lease, 1, !pattern);
                hal::log(format_args!(
                    "VTD observation cas={case:?} sentinelles={intact} faute_enregistree={}",
                    self.engine.fault_pending()
                ));
                check(intact, "modification interdite")?;
                if case != Case::WarmWrite || self.engine.fault_pending() {
                    self.fault(lease, address, read, true)?;
                }
                check(
                    self.engine.matches(lease, 0, expected_source)
                        && self.engine.matches(lease, 1, !pattern),
                    "sentinelles apres faute",
                )?;
                self.retire(lease, false)?;
                if let Some(w) = witness {
                    w.say(ACCEPTED)?;
                    w.finish()?;
                    check(
                        self.witness.take().is_some_and(|pin| pin.release()),
                        "restitution temoin",
                    )?;
                }
                c.say(ACCEPTED)?;
                c.finish()?;
            }
        }
        hal::log(format_args!(
            "VTD cas={case:?} pid={id} epoque={} ok=true",
            lease.epoch()
        ));
        Ok(())
    }
    fn queue_error(&mut self) -> Check<()> {
        let c = Client::new(LATE)?;
        let id = c.pid();
        let lease = self.prepare(&c, 0x73)?;
        c.request(READ, id)?;
        c.say(ACCEPTED)?;
        c.finish()?;
        self.engine
            .close(lease)
            .map_err(|_| "fermeture avant IQE")?;
        check(until(|| self.hw.drain(lease).is_some()), "drain avant IQE")?;
        let remote = crate::smp::count() - 1;
        set_affinity(1 << remote).map_err(|_| "migration IQE")?;
        let quiet = self.hw.drain(lease).ok_or("relecture avant IQE")?;
        check(
            self.engine.retire_invalid(lease, &quiet)
                == Err(crate::hardware::vtd_lab::Error::Quarantine),
            "defaut QI attendu",
        )?;
        check(
            until(|| self.engine.queue_error_holds(lease)),
            "absence IQE",
        )?;
        check(
            self.engine.retire(lease, &quiet).is_err()
                && self.engine.attach(id).is_err()
                && self.engine.pinned() == 2
                && crate::memory::pinned_dma_frames() == 10
                && self.engine.queue_error_holds(lease)
                && crate::smp::index() == remote,
            "quarantaine IQE",
        )?;
        hal::log(format_args!(
            "VTD IQE depart_cpu=0 invalidation_cpu={remote}"
        ));
        set_affinity(1).map_err(|_| "retour BSP apres IQE")?;
        check(self.hw.close(), "fermeture PCI apres IQE")?;
        hal::log(format_args!(
            "VTD erreur QI confirmee cadres_clients=2 cadres_service=8 admissions=0"
        ));
        Ok(())
    }
    fn campaign(&mut self, delayed: bool) -> Check<()> {
        let stable = crate::memory::pinned_dma_frames();
        let base = heap::audit(true);
        let cases: &[Case] = if delayed {
            &[Case::Receipt, Case::Restart]
        } else {
            &[
                Case::Normal,
                Case::WarmWrite,
                Case::WrongWrite,
                Case::WrongRead,
                Case::Unmapped,
                Case::Other,
                Case::Revoke,
                Case::Death,
                Case::Late,
                Case::Restart,
            ]
        };
        for &case in cases {
            hal::log(format_args!("VTD debut {case:?}"));
            self.sample(case)?;
            reap();
            check(
                with(|s| s.ipc.pages()) == 0
                    && self.engine.parked()
                    && crate::memory::pinned_dma_frames() == stable
                    && heap::audit(true) == base,
                "ressources apres cas",
            )?;
        }
        hal::log(format_args!(
            "VTD ressources epingles_service={stable} epingles_clients=0 ipc=0 tas_restitue=true reemploi={}",
            self.reused
        ));
        check(self.reused, "reemploi reel de cadre")
    }
}
pub(crate) fn run(base: u64, drhd: Option<Drhd>, profile: VtdProfile) -> bool {
    if set_affinity(1).is_err() {
        return false;
    }
    let Some(mut device) = Device::adopt(base) else {
        crate::hardware::lab_irq::shutdown();
        return false;
    };
    let before = crate::memory::pinned_dma_frames();
    let engine = drhd.map(Engine::initialize);
    let mut bench = match engine {
        Some(Ok(engine)) => Bench {
            hw: device,
            engine,
            current: None,
            previous: None,
            reused: false,
            witness: None,
        },
        failure => {
            let safe = match failure {
                Some(Err(e)) => {
                    hal::log(format_args!("VTD initialisation refusee {e:?}"));
                    e == crate::hardware::vtd_lab::Error::Feature
                }
                None => true,
                _ => unreachable!(),
            };
            let child = Client::new(REFUSED).and_then(|c| c.finish()).is_ok();
            let closed = device.close();
            hal::log(format_args!(
                "VTD temoin sans domaine ring3_refuse={child} pci_ferme={closed}"
            ));
            return safe && child && closed && crate::memory::pinned_dma_frames() == before;
        }
    };
    let result = match profile {
        VtdProfile::QueueError => bench.queue_error(),
        p => bench.campaign(p == VtdProfile::DelayedReceipt),
    };
    if let Err(reason) = result {
        if let Some(lease) = bench.current {
            let _ = bench.engine.close(lease);
            let _ = until(|| bench.hw.drain(lease).is_some());
        }
        hal::log(format_args!(
            "VTD echec {reason} epingles_clients={} admissions_fermees=true",
            bench.engine.pinned()
        ));
        *STATION.lock() = Some(bench);
        return false;
    }
    let closed = profile == VtdProfile::QueueError || (bench.engine.parked() && bench.hw.close());
    *STATION.lock() = Some(bench);
    closed
}
