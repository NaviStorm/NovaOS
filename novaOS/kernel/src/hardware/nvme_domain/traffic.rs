//! La file QI reste la propriété du superviseur de stockage. Le worker réseau ne reçoit
//! que les pages déjà vérifiées de son bail et son instance Ring 3 non transférable.
use super::{Domain, Error, Result, State};
use crate::hardware::block::{io::Frame, net_event};
use crate::sched::{self, process::net_preparation::EthernetActor};

mod requests;
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum LocalFault {
    None,
    Nic,
    Disk,
    NicFenced(u64, u64),
}

static FAILED_NIC: sched::sync::Mutex<Option<(u64, u64)>> = sched::sync::Mutex::new(None);
use requests::{Key, Owner, Requests};
static REQUESTS: sched::sync::Mutex<Requests> = sched::sync::Mutex::new(Requests::new());

struct Work {
    actor: EthernetActor,
    frames: [Frame; 22],
    epoch: u64,
    contention: bool,
    fault: LocalFault,
    fence: Option<crate::hardware::vtd_lab::StorageFence>,
}

pub(in crate::hardware) struct Drain {
    owner: u64,
    epoch: u64,
}

impl Drain {
    pub(in crate::hardware) fn matches(&self, owner: u64, epoch: u64) -> bool {
        self.owner == owner && self.epoch == epoch
    }
}

static WORK: sched::sync::Mutex<Option<Work>> = sched::sync::Mutex::new(None);
static DRAIN: sched::sync::Mutex<Option<Drain>> = sched::sync::Mutex::new(None);

fn run() {
    let pending = WORK.lock().take();
    if let Some(work) = pending {
        let owner = work.actor.owner();
        let epoch = work.epoch;
        let result = net_event::q35_session(&work.frames, epoch, Some(work.actor), || {
            work.fence.as_ref().is_none_or(|f|
                !crate::sched::process::net_preparation::disk_isolated() || f.permits_network_io())
        });
        crate::hal::log(format_args!(
            "DUO réseau instance={owner} génération={epoch} drainage={result:?}"
        ));
        // Le succès exige les identifiants de chaque complétion, DMA désarmé, le reset,
        // la fermeture de la ligne IRQ, la collecte du PID et de son endpoint.
        if work.fault == LocalFault::Nic && result == Err(crate::hardware::block::Error::Quarantine)
        {
            *FAILED_NIC.lock() = Some((owner, epoch));
        }
        if result.is_ok() {
            *DRAIN.lock() = Some(Drain { owner, epoch });
            if work.contention {
                let key = Key(owner, epoch);
                if post(Owner::Nic, key, "NIC").is_err() {
                    return;
                }
                let start = sched::ticks();
                while !REQUESTS.lock().collect(Owner::Nic, key) {
                    if REQUESTS.lock().held(Owner::Nic, key) || expired(start) {
                        return;
                    }
                    sched::sleep(1);
                }
                crate::hal::log(format_args!(
                    "DUO réponse NIC collectée pid={owner} generation={epoch}"
                ));
            }
        }
    }
}

fn expired(start: u64) -> bool {
    sched::ticks().wrapping_sub(start) >= 120 * u64::from(sched::HZ)
}

fn post(owner: Owner, key: Key, device: &str) -> Result<()> {
    let mut requests = REQUESTS.lock();
    if !requests.post(owner, key) || requests.post(owner, key) || requests.collect(owner, key) {
        return Err(Error::Quarantine);
    }
    crate::hal::log(format_args!(
        "DUO demande {device} pid={} generation={} doublon=refuse avant_WAIT=refuse",
        key.0, key.1
    ));
    Ok(())
}

impl Domain {
    pub(crate) fn local_nic(&self) -> bool {
        matches!(self.fault, LocalFault::Nic | LocalFault::NicFenced(..))
    }
    pub(crate) fn local_disk(&self) -> bool {
        self.fault == LocalFault::Disk
    }

    pub(crate) fn observe_nic_failure(&mut self) -> Result<()> {
        let start = sched::ticks();
        let id = loop {
            if let Some(id) = *FAILED_NIC.lock() {
                break id;
            }
            if expired(start) {
                return Err(Error::Quarantine);
            }
            sched::sleep(1);
        };
        let lease = self.lease.ok_or(Error::Denied)?;
        if !self.local_nic()
            || !crate::sched::process::net_preparation::nic_exited(id.0)
            || !self.engine.failed_nic_fenced(id, lease, self.layout())
        {
            return Err(Error::Quarantine);
        }
        self.fault = LocalFault::NicFenced(id.0, id.1);
        crate::hal::log(format_args!(
            "DUO panne NIC pid={} generation={} BME=0 DMA_retenu=26 NVMe_verifie=true",
            id.0, id.1
        ));
        Ok(())
    }

    pub(super) fn disk_survivor_ok(&self) -> bool {
        match self.fault {
            LocalFault::NicFenced(pid, epoch) => self.lease.is_some_and(|lease| {
                self.engine
                    .failed_nic_fenced((pid, epoch), lease, self.layout())
            }),
            _ => true,
        }
    }

    pub(crate) fn kept_network_frames(&self) -> bool {
        let frames = self.engine.network_frames();
        frames
            .iter()
            .enumerate()
            .all(|(i, pa)| *pa != 0 && !frames[..i].contains(pa))
    }

    pub(crate) fn check_failed_disk(&self) -> Result<()> {
        if !self.local_disk()
            || !self.bar.fenced()
            || !self.engine.fenced_storage_tables(
                self.lease.ok_or(Error::Denied)?,
                self.layout(),
                true,
            )
        {
            return Err(Error::Quarantine);
        }
        crate::hal::log(format_args!(
            "DUO panne NVMe BME=0 DMA_retenu=6 NIC_verifie=true"
        ));
        Ok(())
    }

    pub(crate) fn network_epoch(&self) -> core::result::Result<u64, &'static str> {
        self.engine
            .next_network_epoch()
            .ok_or("génération NIC épuisée")
    }

    pub(crate) fn disk_first(&self) -> bool {
        self.disk_first
    }

    pub(crate) fn reserve_network(&mut self, owner: u64) -> Result<[u64; 26]> {
        if !self.network || !matches!(self.state, State::Prepared) || !self.bar.fenced() {
            return Err(Error::Denied);
        }
        let layout = self.layout();
        self.engine
            .prepare_nic(owner, self.lease.ok_or(Error::Denied)?, layout)?;
        self.engine.trace_duo();
        Ok(self.engine.network_frames())
    }

    pub(crate) fn launch_network(&mut self, actor: EthernetActor) -> Result<sched::Id> {
        if WORK.lock().is_some() || DRAIN.lock().is_some() {
            actor.cancel();
            return Err(Error::Quarantine);
        }
        let (frames, epoch) = self.engine.start_network(actor.owner())?;
        let fence = self.local_disk().then(|| self.engine.storage_fence());
        *WORK.lock() = Some(Work {
            actor,
            frames,
            epoch,
            contention: self.contention,
            fault: self.fault,
            fence,
        });
        sched::spawn_on(run, 1).map_err(|_| {
            if let Some(work) = WORK.lock().take() {
                work.actor.cancel();
            }
            Error::Quarantine
        })
    }

    pub(crate) fn retire_contended(
        &mut self,
        disk: &super::super::nvme::Drained<'_>,
        worker: sched::Id,
    ) -> Result<()> {
        let lease = self.lease.ok_or(Error::Denied)?;
        if !self.contention || !disk.confirmed(self.memory.as_ref().ok_or(Error::Denied)?) {
            return Err(Error::Quarantine);
        }
        let key = Key(lease.owner(), lease.epoch());
        post(Owner::Disk, key, "NVMe")?;
        let start = sched::ticks();
        let (disk_id, nic_id) = loop {
            if expired(start) || !sched::live(worker) {
                return Err(Error::Quarantine);
            }
            if let Some(pair) = REQUESTS.lock().pair() {
                break pair;
            }
            sched::sleep(1);
        };
        let net = DRAIN.lock().take().ok_or(Error::Quarantine)?;
        if key != disk_id || disk_id.0 == nic_id.0 || !net.matches(nic_id.0, nic_id.1) {
            return Err(Error::Quarantine);
        }
        crate::hal::log(format_args!(
            "DUO contention demandes=2 premier_disque={} dma={} octets_slots={} octets_mutex={}",
            self.disk_first,
            crate::memory::pinned_dma_frames(),
            core::mem::size_of::<Requests>(),
            core::mem::size_of_val(&REQUESTS)
        ));
        crate::sched::process::net_preparation::mesurer("duo_deux_demandes");
        // L'intention ne vaut pas acquittement. Ces chemins relisent le drain et
        // le WAIT de l'effecteur avant toute restitution de pages ou notification.
        if self.disk_first {
            if let Err(err) = self.retire(disk) {
                crate::hal::log(format_args!("DUO issue retrait NVMe={err:?}"));
                self.retain_pending(disk_id, nic_id, worker)?;
                return Err(err);
            }
            self.complete_disk_request(disk_id)?;
        }
        if let Err(err) = self.engine.retire_network(net) {
            crate::hal::log(format_args!("DUO issue retrait NIC={err:?}"));
            self.retain_pending(disk_id, nic_id, worker)?;
            return Err(err);
        }
        if !REQUESTS.lock().complete(Owner::Nic, nic_id) {
            return Err(Error::Quarantine);
        }
        crate::hal::log(format_args!(
            "DUO restitution NIC dma={}",
            crate::memory::pinned_dma_frames()
        ));
        if !self.disk_first {
            if let Err(err) = self.retire(disk) {
                REQUESTS.lock().hold_pending();
                self.quarantine();
                return Err(err);
            }
            self.complete_disk_request(disk_id)?;
        }
        while sched::live(worker) {
            if expired(start) {
                return Err(Error::Quarantine);
            }
            sched::sleep(1);
        }
        Ok(())
    }

    // Aucune réponse de succès sur le chemin d'incertitude commune. La
    // lecture du cookie après le refus ne réarme ni les baux ni la file QI.
    fn retain_pending(&mut self, disk: Key, nic: Key, worker: sched::Id) -> Result<()> {
        REQUESTS.lock().hold_pending();
        self.quarantine();
        if !self.engine.common_wait_held() {
            return Err(Error::Quarantine);
        }
        let observed = self.engine.wait_snapshot();
        let retained = crate::memory::pinned_dma_frames();
        if observed != (0, 176, 192) || retained != 40 {
            return Err(Error::Quarantine);
        }
        crate::hal::log(format_args!(
            "DUO contention WAIT incertain cookie={} iqh={} iqt={} dma={retained}",
            observed.0, observed.1, observed.2
        ));
        crate::sched::process::net_preparation::mesurer("duo_wait_incertain");
        let begin = sched::ticks();
        while self.engine.wait_snapshot() == observed
            && sched::ticks().wrapping_sub(begin) < 6 * u64::from(sched::HZ)
        {
            sched::sleep(1);
        }
        let snapshot = self.engine.wait_snapshot();
        let mut requests = REQUESTS.lock();
        let blocked = requests.held(Owner::Disk, disk)
            && requests.held(Owner::Nic, nic)
            && !requests.post(Owner::Disk, disk)
            && !requests.post(Owner::Nic, nic)
            && !requests.collect(Owner::Disk, disk)
            && !requests.collect(Owner::Nic, nic);
        drop(requests);
        if !blocked
            || !self.engine.common_wait_held()
            || sched::live(worker)
            || retained != crate::memory::pinned_dma_frames()
            || !self.bar.fenced()
            || !matches!(snapshot, (0, 176, 192) | (4, 192, 192))
        {
            return Err(Error::Quarantine);
        }
        crate::hal::log(format_args!(
            "DUO contention deux_retraits_refuses PID={}/{} nouvelles_demandes=refusees cookie={} iqh={} iqt={} dma={retained}",
            disk.0, nic.0, snapshot.0, snapshot.1, snapshot.2
        ));
        Ok(())
    }

    fn complete_disk_request(&self, key: Key) -> Result<()> {
        let mut requests = REQUESTS.lock();
        if !requests.complete(Owner::Disk, key) || !requests.collect(Owner::Disk, key) {
            return Err(Error::Quarantine);
        }
        crate::hal::log(format_args!(
            "DUO restitution NVMe dma={}",
            crate::memory::pinned_dma_frames()
        ));
        Ok(())
    }

    pub(crate) fn drain_network(&mut self, worker: sched::Id) -> Result<()> {
        let start = sched::ticks();
        while sched::live(worker) {
            if sched::ticks().wrapping_sub(start) >= 120 * u64::from(sched::HZ) {
                return Err(Error::Quarantine);
            }
            sched::sleep(1);
        }
        if self.local_disk()
            && (!self.bar.fenced()
                || !self.engine.fenced_storage_tables(
                    self.lease.ok_or(Error::Denied)?,
                    self.layout(),
                    false,
                ))
        {
            return Err(Error::Quarantine);
        }
        let proof = DRAIN.lock().take().ok_or(Error::Quarantine)?;
        self.engine.retire_network(proof)?;
        self.engine.trace_duo();
        Ok(())
    }
}
