//! La file QI reste la propriété du superviseur de stockage. Le worker réseau ne reçoit
//! que les pages déjà vérifiées de son bail et son instance Ring 3 non transférable.
use super::{Domain, Error, Result, State};
use crate::hardware::block::{io::Frame, net_event};
use crate::sched::{self, process::net_preparation::EthernetActor};

struct Work { actor: EthernetActor, frames: [Frame;22], epoch: u64 }

pub(in crate::hardware) struct Drain { owner:u64, epoch:u64 }

impl Drain {
    pub(in crate::hardware) fn matches(&self, owner:u64, epoch:u64)->bool {
        self.owner==owner && self.epoch==epoch
    }
}

static WORK: sched::sync::Mutex<Option<Work>> = sched::sync::Mutex::new(None);
static DRAIN: sched::sync::Mutex<Option<Drain>> = sched::sync::Mutex::new(None);

fn run() {
    let pending = WORK.lock().take();
    if let Some(work) = pending {
        let owner=work.actor.owner();
        let epoch=work.epoch;
        let result=net_event::q35_session(&work.frames,epoch,Some(work.actor));
        crate::hal::log(format_args!("DUO réseau instance={owner} génération={epoch} drainage={result:?}"));
        // Le succès exige les identifiants de chaque complétion, DMA désarmé, le reset,
        // la fermeture de la ligne IRQ, la collecte du PID et de son endpoint.
        if result.is_ok() { *DRAIN.lock()=Some(Drain{owner,epoch}); }
    }
}

impl Domain {
    pub(crate) fn disk_first(&self)->bool {self.disk_first}

    pub(crate) fn reserve_network(&mut self, owner:u64)->Result<[u64;26]> {
        if !self.network || !matches!(self.state,State::Prepared) || !self.bar.fenced() {
            return Err(Error::Denied);
        }
        let layout = self.layout();
        self.engine.prepare_nic(owner,self.lease.ok_or(Error::Denied)?,layout)?;
        self.engine.trace_duo();
        Ok(self.engine.network_frames())
    }

    pub(crate) fn launch_network(&mut self, actor: EthernetActor)->Result<sched::Id> {
        if WORK.lock().is_some() || DRAIN.lock().is_some() {
            actor.cancel();
            return Err(Error::Quarantine);
        }
        let (frames,epoch)=self.engine.start_network(actor.owner())?;
        *WORK.lock()=Some(Work{actor,frames,epoch});
        sched::spawn_on(run,1).map_err(|_| {
            if let Some(work)=WORK.lock().take(){work.actor.cancel();}
            Error::Quarantine
        })
    }

    pub(crate) fn drain_network(&mut self, worker:sched::Id)->Result<()> {
        let start=sched::ticks();
        while sched::live(worker) {
            if sched::ticks().wrapping_sub(start)>=120*u64::from(sched::HZ) {return Err(Error::Quarantine);}
            sched::sleep(1);
        }
        let proof=DRAIN.lock().take().ok_or(Error::Quarantine)?;
        self.engine.retire_network(proof)?;
        self.engine.trace_duo();
        Ok(())
    }
}
