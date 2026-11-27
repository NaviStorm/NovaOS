//! Témoin du superviseur Q35. Le NIC demeure désarmé pendant la construction
//! et l'inspection de son domaine. Le noyau conserve les tables jusqu'à l'arrêt.
use super::{
    acpi,
    block::io::{EMPTY, Frame, Region, barrier, wait},
    discovery, pci_inventory as pci, pci_ports,
};
use crate::{
    boot, hal,
    memory::{self, firmware},
};

#[cfg(feature="ethernet-v1-port")]
mod retrait;
#[cfg(feature = "gestionnaire-4f")]
pub(in crate::hardware) mod gestionnaire;

const SID: u16 = 7 << 3;
const IOVA: u64 = 0x1000_0000;
const ROOT: u32 = 1 << 30;
const QI: u32 = 1 << 26;
const TE: u32 = 1 << 31;
const PCI: [(u8, u16, u16); 5] = [
    (0, 0x8086, 0x29c0),
    (56, 0x1af4, 0x1041),
    (248, 0x8086, 0x2918),
    (250, 0x8086, 0x2922),
    (251, 0x8086, 0x2930),
];

fn commande(sid: u16) -> u32 {
    pci_ports::port(sid, 4, 2, None)
}
fn pairs_fermes() -> bool {
    PCI.iter().all(|(s, _, _)| commande(u16::from(*s)) & 4 == 0)
}

fn topologie() -> Option<u64> {
    let mut functions = alloc::vec::Vec::new();
    let mut exact = true;
    pci::enumerate(
        &mut discovery::Config,
        pci::BusRange {
            segment: 0,
            first: 0,
            last: 0,
        },
        |a, f| {
            if let Ok(f) = f
                && functions.len() < 5
            {
                functions.push((a, f));
            } else {
                exact = false;
            }
        },
    )
    .ok()?;
    if !exact || functions.len() != PCI.len() {
        return None;
    }
    for ((a, f), &(sid, vendor, model)) in functions.iter().zip(&PCI) {
        if a.segment != 0
            || a.bus != 0
            || a.device * 8 + a.function != sid
            || f.vendor != vendor
            || f.device != model
            || f.header & 127 != 0
            || f.rom_enabled
            || f.msi_armed
            || (sid == 56 && cfg!(feature="ethernet-v1-port") && f.pcie)
            || (sid == 56 && (f.bus_master || f.class != 2 || f.subclass != 0 || f.msi || f.msix))
        {
            return None;
        }
    }
    let inv = acpi::discover(boot::rsdp_physical()?, firmware::copy_acpi).ok()?;
    let drhd = inv.q35_net?;
    if inv.ecam.as_slice()
        != [acpi::Ecam {
            base: 0xe000_0000,
            segment: 0,
            first_bus: 0,
            last_bus: 255,
        }]
        || drhd.host_bits != 48
        || !firmware::unlisted_extent(drhd.base, memory::PAGE)
    {
        return None;
    }
    if !discovery::vtd::quiesce_peers(&functions, functions[1].0) || !pairs_fermes() {
        return None;
    }
    Some(drhd.base)
}

fn publier(frames: &[Frame], line: usize) {
    for f in frames {
        for off in (0..4096).step_by(line) {
            // Sûreté : cadres privés épinglés, géométrie de cache vérifiée sur le BSP.
            unsafe {
                core::arch::x86_64::_mm_clflush((f.mem.base() + off) as *const u8);
            }
        }
    }
    barrier()
}

#[derive(Clone, Copy)]
struct RecuQi {
    root: u64,
    queue: u64,
    statut: Frame,
    fin: u64,
    cookie: u32,
}

impl RecuQi {
    fn collecter(self, reg: Region) -> Result<(), &'static str> {
        if reg.r64(0x20) != self.root || reg.r64(0x90) != self.queue || reg.r64(0x88) != self.fin {
            return Err("identite QI");
        }
        wait(|| reg.r32(0x34) != 0 ||
            (reg.r64(0x80) == self.fin && self.statut.mem.r32(0) == self.cookie))
            .map_err(|_| "expiration QI")?;
        if reg.r32(0x34) != 0 || reg.r64(0x80) != self.fin || reg.r64(0x88) != self.fin
            || self.statut.mem.r32(0) != self.cookie {
            return Err("faute QI");
        }
        Ok(())
    }
}

fn soumettre_invalidation(reg: Region, frames: &[Frame], index: u64, cookie: u32, line: usize) -> Result<RecuQi, &'static str> {
    if index >= 4096 || index % 16 != 0 || cookie == 0
        || frames[7].mem.r32(0) != cookie-1 || reg.r64(0x90) != frames[6].pa
        || reg.r64(0x20) != frames[0].pa || reg.r64(0x80) != index
        || reg.r64(0x88) != index || reg.r32(0x34) != 0 {
        return Err("file occupee");
    }
    let q = frames[6].mem;
    // CC global et IOTLB global, DR et DW. Les mots réservés restent nuls.
    let slot = |offset: u64| ((index + offset) % 4096) as usize;
    for (off, lo, hi) in [(0, 0x11, 0), (16, 0xd2, 0),
        (32, (u64::from(cookie) << 32) | 0x25, frames[7].pa)] {
        q.w64(slot(off + 8), hi);
        q.w64(slot(off), lo);
    }
    publier(&frames[6..7], line);
    let fin = (index + 48) % 4096;
    reg.w64(0x88, fin);
    Ok(RecuQi { root: frames[0].pa, queue: frames[6].pa, statut: frames[7], fin, cookie })
}

fn invalider(reg: Region, frames: &[Frame], index: u64, cookie: u32, line: usize) -> Result<(), &'static str> {
    soumettre_invalidation(reg, frames, index, cookie, line)?.collecter(reg)
}

fn permanent(i: usize) -> bool { matches!(i, 0 | 6 | 7) }

fn reserver_cadres(frames: &mut [Frame;30], reprise: bool) -> Result<(), &'static str> {
    for i in 0..frames.len() {
        if reprise && permanent(i) { continue; }
        if frames[i].pa != 0 { return Err("cadre deja occupe"); }
        match Frame::allocate() {
            Ok(f) if f.pa != 0 && f.pa < 1 << 48 && f.pa % 4096 == 0 => {
                for off in (0..4096).step_by(8) { f.mem.w64(off, 0); }
                frames[i] = f;
            }
            other => {
                // Aucun cadre de ce lot n'a été publié dans le contexte matériel.
                for (j, f) in frames[..i].iter_mut().enumerate() {
                    if !reprise || !permanent(j) {
                        unsafe { memory::release_dma(f.pa) }
                        *f = EMPTY;
                    }
                }
                if let Ok(f)=other { unsafe {memory::release_dma(f.pa)} }
                return Err("cadres");
            }
        }
    }
    Ok(())
}

pub(crate) fn executer(retrait: bool) -> Result<(), &'static str> {
    #[cfg(feature="ethernet-v1-port")]
    if !retrait::disponible() { return Err("NIC en quarantaine"); }
    if crate::smp::index() != 0 || crate::smp::count() != 1 {
        return Err("cpu");
    }
    let id = core::arch::x86_64::__cpuid(0x40000000);
    if [id.ebx, id.ecx, id.edx]
        != [
            u32::from_le_bytes(*b"TCGT"),
            u32::from_le_bytes(*b"CGTC"),
            u32::from_le_bytes(*b"GTCG"),
        ]
    {
        return Err("TCG");
    }
    let reg =
        Region::map(topologie().ok_or("topologie ou pairs")?, 344, 4096).map_err(|_| "MMIO")?;
    let cap = reg.r64(8);
    let ext = reg.r64(16);
    if reg.r32(0) != 0x10
        || reg.r32(0x1c) != 0
        || reg.r64(0x20) != 0
        || reg.r64(0x80) != 0
        || reg.r64(0x88) != 0
        || reg.r64(0x90) != 0
        || reg.r32(0x34) != 0
        || (cap >> 16) & 63 != 47
        || cap & (3 << 54) != 3 << 54
        || cap & (1 << 10) == 0
        || cap & 16 != 0
        || ext & 2 == 0
    {
        return Err("etat initial");
    }
    let cpu = core::arch::x86_64::__cpuid(1);
    let line = ((cpu.ebx >> 8) & 255) as usize * 8;
    if cpu.edx & (1 << 19) == 0 || !line.is_power_of_two() || line > 4096 {
        return Err("cache");
    }
    #[cfg(feature="net-ethernet-lab")]
    crate::sched::process::net_preparation::mesurer("net_initial");
    let reference = memory::pinned_dma_frames();
    let mut frames = [EMPTY; 30];
    #[cfg(feature = "gestionnaire-4f")]
    let mut premier = Some(gestionnaire::selectionner()?);
    reserver_cadres(&mut frames, false)?;
    #[cfg(feature="ethernet-v1-port")]
    let mut generation_exposee = 0;
    let operation = (|| {
    // Après RTADDR, conserver tous les cadres en cas de retour indéterminé.
    publier(&frames[..8], line);
    reg.w64(0x20, frames[0].pa);
    reg.w32(0x18, ROOT);
    wait(|| reg.r32(0x1c) == ROOT && reg.r64(0x20) == frames[0].pa).map_err(|_| "RTPS")?;
    reg.w64(0x90, frames[6].pa);
    reg.w32(0x18, QI);
    wait(|| reg.r32(0x1c) == ROOT | QI && reg.r64(0x90) == frames[6].pa).map_err(|_| "QIES")?;
    invalider(reg, &frames, 0, 1, line)?;
    reg.w32(0x18, QI | TE);
    wait(|| reg.r32(0x1c) == ROOT | QI | TE).map_err(|_| "TES")?;
    if !pairs_fermes() {
        return Err("PCI");
    }
    // Le chargeur peut déjà avoir restitué ses pages au second cycle.
    #[cfg(feature="net-event-idx")]
    let scenario_ethernet = boot::scenario() == b"net-q35-ethernet";
    let cycles = if cfg!(feature="ethernet-v1-port") {
        match option_env!("NOVAOS_ETH3_CASE") {
            Some("cycles" | "cycles-court") => 4,
            #[cfg(feature = "gestionnaire-4f")]
            Some("cycles-256-court") => 256,
            Some("rattrapage" | "recu-vtd-differe") => 2,
            _ => 1,
        }
    } else if cfg!(feature="net-ethernet-lab") && option_env!("NOVAOS_NET_SERIE")==Some("redemarrage") && boot::scenario()==b"net-q35-ethernet" {4u32} else {1};
    for cycle in 0..cycles {
    #[cfg(feature="ethernet-v1-port")]
    { generation_exposee = cycle + 1; }
    #[cfg(feature = "gestionnaire-4f")]
    let (_mesure, decision) = match premier.take() {
        Some(p) => p,
        None => gestionnaire::selectionner()?,
    };
    if cycle!=0 {
        if !pairs_fermes() || reg.r32(0x1c) != ROOT | QI | TE || frames[0].mem.r64(0)!=0 { return Err("controleur non retire"); }
        reserver_cadres(&mut frames,true)?;
    }
    let cookie=2+2*cycle;
    let offset=(48+96*u64::from(cycle)) % 4096;
    // Une seule feuille de second niveau, exactement 22 cadres NIC.
    for (idx, shift) in [39, 30, 21].into_iter().enumerate() {
        frames[2 + idx]
            .mem
            .w64(8 * ((IOVA >> shift) & 511) as usize, frames[3 + idx].pa | 3);
    }
    for i in 0..22 {
        let dir = if matches!(i, 2 | 5 | 6..=13) { 2 } else { 1 };
        frames[5].mem.w64(i * 8, frames[8 + i].pa | dir);
    }
    frames[1].mem.w64(usize::from(SID) * 16 + 8, (3 << 8) | 2);
    frames[1].mem.w64(usize::from(SID) * 16, frames[2].pa | 1);
    publier(&frames[1..6], line);
    frames[0].mem.w64(0, frames[1].pa | 1);
    publier(&frames[..1], line);
    invalider(reg, &frames, offset, cookie, line)?;
    if !pairs_fermes() || reg.r32(0x1c) != ROOT | QI | TE {
        return Err("etat terminal");
    }
    hal::log(format_args!(
        "NET Q35 domaine=3 sid=56 iova={IOVA:#x} root={:#x} qi={:#x} recu={:#x} cookie={cookie} blocs=22 cadres=30",
        frames[0].pa, frames[6].pa, frames[7].pa
    ));
    #[cfg(feature = "gestionnaire-4f")]
    let acteur = gestionnaire::admettre(reg, &frames, u64::from(cycle) + 1, decision)?;
    #[cfg(feature = "net-event-idx")]
    let negotiation = match scenario_ethernet {
        #[cfg(feature="net-ethernet-lab")]
        true => {
            // Le domaine appartient exclusivement au titulaire. Une issue incertaine
            // conserve tous ses cadres épinglés jusqu'à arrêt de la VM.
            let trafic = super::block::net_event::trafic_q35(frames[8..].try_into().unwrap(),u64::from(cycle)+1,
                #[cfg(feature = "gestionnaire-4f")] acteur);
            if trafic.is_err() || reg.r32(0x34) != 0 {
                hal::log(format_args!("NET Q35 L2 quarantaine={trafic:?}"));
                #[cfg(feature="ethernet-v1-port")]
                {
                    retrait::garer(&mut frames,cycle+1,None)?;
                    hal::log(format_args!("E3C VT-d arret incertain generation={} epingles={} rattachement_refuse={}",cycle+1,memory::pinned_dma_frames()-reference,!retrait::disponible()));
                }
                return Err("trafic non confirme");
            }
            Ok(())
        }
        _ => super::block::net_event::q35(frames[8..].try_into().expect("22 cadres du NIC")),
    };
    #[cfg(feature = "net-event-idx")]
    let retrait = retrait || negotiation.is_err();
    if retrait {
        // BME est coupé et le NIC arrêté. Supprimer la seule racine avant CC global,
        // IOTLB global avec DR/DW et WAIT sur un nouveau reçu de cette file.
        if !pairs_fermes() || frames[0].mem.r64(0) != frames[1].pa | 1 {
            return Err("retrait non admissible");
        }
        frames[0].mem.w64(0, 0);
        publier(&frames[..1], line);
        #[cfg(feature="ethernet-v1-port")]
        {
            let attente = match soumettre_invalidation(reg,&frames,(offset+48)%4096,cookie+1,line) {
                Ok(r) => r,
                Err(e) => { retrait::garer(&mut frames,cycle+1,None)?; return Err(e); }
            };
            match retrait::attendre(reg, &mut frames, cycle+1, attente) {
                Err("livraison differee" | "expiration QI") => {
                    frames = retrait::reprendre(reg, cycle+1, attente.cookie)?;
                    if retrait::reprendre(reg, cycle+1, attente.cookie).err() != Some("aucun retrait en attente") {
                        return Err("recu de retrait reutilisable");
                    }
                }
                resultat => resultat?,
            }
            hal::log(format_args!("E3C VT-d generation={} bme=0 reset=0 cc=global iotlb=global dr=1 dw=1 iqh={} iqt={} cookie={} fsts=0",cycle+1,reg.r64(0x80),reg.r64(0x88),frames[7].mem.r32(0)));
        }
        #[cfg(not(feature="ethernet-v1-port"))]
        {
            invalider(reg,&frames,(offset+48)%4096,cookie+1,line)?;
            if !pairs_fermes() || reg.r32(0x1c) != ROOT | QI | TE
                || (0..512).any(|i| frames[0].mem.r64(i*8)!=0) { return Err("retrait incertain"); }
        }
        #[cfg(feature = "gestionnaire-4f")]
        gestionnaire::autoriser_restitution(u64::from(cycle) + 1, &frames, cookie + 1)?;
        for (i, f) in frames.iter_mut().enumerate() {
            if !permanent(i) {
                unsafe { memory::release_dma(f.pa) }
                *f=EMPTY;
            }
        }
        #[cfg(feature = "gestionnaire-4f")]
        gestionnaire::restitue(u64::from(cycle) + 1);
        #[cfg(feature="net-ethernet-lab")]
        crate::sched::process::net_preparation::mesurer("net_final");
        let gardes = memory::pinned_dma_frames()
            .checked_sub(reference)
            .ok_or("comptabilite")?;
        if gardes != 3 {
            return Err("restitution");
        }
        hal::log(format_args!(
            "NET Q35 retrait cookie={} racine_vide=true rendus=27 permanents={gardes}",cookie+1
        ));
    }
    #[cfg(feature = "net-event-idx")]
    negotiation.map_err(|_| "negociation EVENT_IDX")?;
    }
    Ok(())
    })();
    #[cfg(feature="ethernet-v1-port")]
    if operation.is_err() && frames.iter().any(|f| f.pa != 0) {
        retrait::garer(&mut frames, generation_exposee, None)?;
    }
    operation
}
