//! Qualification AArch64 à un vCPU. Un client NovaFS réel et le pilote NIC ont des canaux distincts.
use super::{Error, Result, net_arm::Preparation};
use crate::{hal, sched};
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

static SESSION: sched::sync::Mutex<Option<(u64,[u8;16])>>=sched::sync::Mutex::new(None);
static TERMINE: AtomicBool=AtomicBool::new(false);
static LOT: AtomicU32=AtomicU32::new(0);
static IO: [AtomicU64;2]=[const {AtomicU64::new(0)};2];
static CYCLES:[AtomicU64;3]=[const {AtomicU64::new(0)};3];
static FLUSH:sched::sync::Mutex<Vec<u64>>=sched::sync::Mutex::new(Vec::new());
static MESURER:AtomicBool=AtomicBool::new(false);

pub(super) fn progression(rx:u32) {LOT.store(rx,Ordering::Release);}
pub(super) fn octets(sens:usize,taille:u64) {
    if MESURER.load(Ordering::Acquire) {IO[sens].fetch_add(taille,Ordering::Relaxed);}
}
pub(super) fn mesure(sens:usize,cycles:u64) {
    if !MESURER.load(Ordering::Acquire) {return;}
    CYCLES[sens].fetch_add(cycles,Ordering::Relaxed);
    if sens==2 {
        let mut f=FLUSH.lock();
        if f.len()<4096 {f.push(cycles);}
    }
}

// La longueur du segment de données appartient à la commande dont le statut vient d'être confirmé.
pub(super) fn completion(transfer: &super::Transport, cycles: u64) {
    if transfer.kind < 2 {
        octets(transfer.kind, u64::from(transfer.q.pages[0].mem.r32(16 + 8)));
    }
    mesure(transfer.kind, cycles);
}

fn stockage() {
    let session=SESSION.lock().take().expect("session disque réservée");
    let rx0=LOT.load(Ordering::Acquire);
    let t0=hal::timer::counter();
    MESURER.store(true,Ordering::Release);
    let ok=sched::process::storage::volume_detail(25,Some(session),true);
    MESURER.store(false,Ordering::Release);
    let cycles=hal::timer::counter()-t0;
    let rx1=LOT.load(Ordering::Acquire);
    let hz=hal::timer::counter_hz();
    let io=IO.each_ref().map(|v|v.load(Ordering::Relaxed));
    let temps=CYCLES.each_ref().map(|v|v.load(Ordering::Relaxed));
    let flush=core::mem::take(&mut *FLUSH.lock());
    for paquet in flush.chunks(32) {
        hal::log(format_args!("NET P9 FLUSH cycles={paquet:?}"));
    }
    hal::log(format_args!("NET P9 resultat={ok} hz={hz} cycles={cycles} rx_debut={rx0} rx_fin={rx1} octets={io:?} temps={temps:?} flush={}",flush.len()));
    TERMINE.store(ok && (256..4096).contains(&flush.len()),Ordering::Release);
}

pub(super) fn essai(nic:Preparation,key:u64,id:[u8;16],trafic:bool)->Result<()> {
    *SESSION.lock()=Some((key,id));
    let mut travail=None;
    let resultat=if trafic {
        nic.trames(||{
            travail=sched::spawn(stockage).ok();
            travail.is_some()
        })
    } else {
        stockage();
        nic.fermer()
    };
    if let Some(t)=travail {
        let debut=sched::ticks();
        while sched::live(t) && sched::ticks().saturating_sub(debut)<300*u64::from(sched::HZ) {
            sched::sleep(1);
        }
        if sched::live(t) {return Err(Error::Timeout);}
    } else if SESSION.lock().take().is_some() {
        super::retire()?;
        return Err(Error::Route);
    }
    resultat?;
    let fin=crate::memory::pinned_dma_frames();
    hal::log(format_args!("NET P9 coexistence trafic={trafic} cadres_fin={fin}"));
    if !TERMINE.load(Ordering::Acquire) || fin!=0 {return Err(Error::Fault);}
    Ok(())
}
