//! Vue de corroboration du propriétaire VT-d. Aucun cadre ni registre ne passe en Ring 3.
use super::*;
use crate::sched::process::net_preparation::{dynamique::{self, reprise}, port_v1::EthernetActor};
use crate::sched::sync::Mutex;

#[derive(Clone, Copy)]
struct Vue {
    generation: u64,
    reg: Region,
    frames: [Frame; 30],
    recu: Option<u32>,
    #[cfg(feature = "net-q35-duo")]
    partage: Option<BailDouble>,
}

#[cfg(feature = "net-q35-duo")]
#[derive(Clone, Copy)]
struct BailDouble {
    titulaire: u64,
    disque: u64,
    epoque: u64,
    contexte_nvme: [u64; 2],
}

// Le superviseur garde les cadres épinglés tant que cette vue est installée.
// Elle est retirée avant le premier release_dma, y compris en cas de reprise.
static VUE: Mutex<Option<Vue>> = Mutex::new(None);

pub(crate) fn verifier_port_selectionne(id: u64, sid: u64) -> bool {
    sid == u64::from(SID) && id == 0x10411af4 && u64::from(pci_ports::port(SID, 0, 4, None)) == id
        && commande(SID) & 4 == 0
}

pub(crate) fn selectionner() -> Result<(dynamique::Cycle, dynamique::Decision), &'static str> {
    let cycle = dynamique::Cycle::ouvrir();
    let id = u64::from(pci_ports::port(SID, 0, 4, None));
    if !verifier_port_selectionne(id, u64::from(SID)) || VUE.lock().is_some() {
        return Err("inventaire Q35");
    }
    let decision = dynamique::selection(id, u64::from(SID)).ok_or("selection Q35")?;
    Ok((cycle, decision))
}

pub(crate) fn carte_bail_nic(generation: u64, cadres: [u64; 22]) -> Option<u64> {
    let lock = VUE.lock();
    let vue = lock.as_ref()?;
    let f = &vue.frames;
    let reg = vue.reg;
    if generation != vue.generation || !f[8..].iter().map(|p| p.pa).eq(cadres)
        || reg.r32(0x34) != 0 || reg.r32(0x1c) != ROOT | QI | TE
        || reg.r64(0x20) != f[0].pa || reg.r64(0x90) != f[6].pa {
        return None;
    }
    if let Some(cookie) = vue.recu {
        #[cfg(feature = "net-q35-duo")]
        if let Some(bail) = vue.partage {
            let ctx = usize::from(SID) * 16;
            return (bail.disque != 0 && bail.epoque != 0 && bail.titulaire != 0
                && commande(SID) & 4 == 0 && commande(48) & 4 != 0
                && f[0].mem.r64(0) == f[1].pa | 1
                && f[1].mem.r64(ctx) == 0 && f[1].mem.r64(ctx+8) == 0
                && [f[1].mem.r64(48*16), f[1].mem.r64(48*16+8)] == bail.contexte_nvme
                && f[7].mem.r32(0) >= cookie && reg.r64(0x80) == reg.r64(0x88)).then_some(0);
        }
        return (f[7].mem.r32(0) == cookie && reg.r64(0x80) == reg.r64(0x88)
            && pairs_fermes() && (0..512).all(|i| f[0].mem.r64(i * 8) == 0)).then_some(0);
    }
    #[cfg(feature = "net-q35-duo")]
    if let Some(bail) = vue.partage {
        if [f[1].mem.r64(48*16), f[1].mem.r64(48*16+8)] != bail.contexte_nvme {
            return None;
        }
    }
    let ctx = usize::from(SID) * 16;
    if f[0].mem.r64(0) != f[1].pa | 1 || f[1].mem.r64(ctx) != f[2].pa | 1
        || f[1].mem.r64(ctx + 8) != (3 << 8) | 2 {
        return None;
    }
    for (n, shift) in [39, 30, 21].into_iter().enumerate() {
        if f[2+n].mem.r64(8 * ((IOVA >> shift) & 511) as usize) != f[3+n].pa | 3 {
            return None;
        }
    }
    for i in 0..512 {
        let expected = if i < 22 {
            cadres[i] | if matches!(i, 2 | 5 | 6..=13) { 2 } else { 1 }
        } else { 0 };
        if f[5].mem.r64(i * 8) != expected { return None; }
    }
    Some((1 << 22) - 1)
}

pub(super) fn admettre(reg: Region, frames: &[Frame; 30], g: u64, decision: dynamique::Decision)
    -> Result<EthernetActor, &'static str> {
    {
        let mut vue = VUE.lock();
        if vue.is_some() { return Err("ancien bail Q35"); }
        *vue = Some(Vue {generation: g, reg, frames: *frames, recu: None,
            #[cfg(feature="net-q35-duo")] partage: None});
    }
    let pages = core::array::from_fn(|i| frames[8+i].pa);
    if !reprise::lot_vtd(g, pages) { return Err("bail Q35 non corrobore"); }
    EthernetActor::reserve(g, Some(decision)).ok_or("admission dynamique Q35")
}

#[cfg(feature = "net-q35-duo")]
pub(in crate::hardware) fn enregistrer_double(reg: Region, frames: [Frame; 30], generation: u64,
    titulaire: u64, disque: u64, epoque: u64) -> Result<(), &'static str> {
    let contexte_nvme = [frames[1].mem.r64(48*16), frames[1].mem.r64(48*16+8)];
    if titulaire == 0 || disque == 0 || epoque == 0 || titulaire == disque || contexte_nvme[0] & 1 == 0 {
        return Err("identité des deux domaines");
    }
    {
        let mut lock = VUE.lock();
        if lock.is_some() {return Err("ancien bail matériel");}
        *lock = Some(Vue {reg, frames, generation, recu: None,
            partage: Some(BailDouble {titulaire, disque, epoque, contexte_nvme})});
    }
    let pages = core::array::from_fn(|i| frames[8+i].pa);
    reprise::lot_vtd(generation, pages).then_some(()).ok_or("bail double non corroboré")
}

#[cfg(feature = "net-q35-duo")]
pub(in crate::hardware) fn autoriser_double(id: (u64, u64), cookie: u32, cadres: [Frame; 26]) -> bool {
    let frames = {
        let lock = VUE.lock();
        let Some(v) = lock.as_ref() else {return false};
        if v.generation != id.1 || v.partage.is_none_or(|b| b.titulaire != id.0)
            || !cadres[..4].iter().zip(&v.frames[2..6]).all(|(a,b)| a.pa == b.pa)
            || !cadres[4..].iter().zip(&v.frames[8..]).all(|(a,b)| a.pa == b.pa) {return false;}
        v.frames
    };
    autoriser_restitution(id.1, &frames, cookie).is_ok()
}

// Appel exclusivement après la collecte du WAIT par le propriétaire matériel.
pub(super) fn autoriser_restitution(g: u64, frames: &[Frame; 30], cookie: u32) -> Result<(), &'static str> {
    let pages = core::array::from_fn(|i| frames[8+i].pa);
    {
        let mut vue = VUE.lock();
        let v = vue.as_mut().ok_or("bail absent")?;
        if v.generation != g || v.recu.is_some() {return Err("recu VT-d perime");}
        v.recu = Some(cookie);
    }
    if carte_bail_nic(g, pages) != Some(0)
        || !reprise::ressources(g, pages, 0, u64::from(cookie), "reconciliation")
        || !reprise::liberation_permises(g, pages) {
        reprise::geler_carte(g);
        return Err("quarantaine VT-d");
    }
    VUE.lock().take();
    Ok(())
}

pub(in crate::hardware) fn restitue(g: u64) {
    reprise::rendu(g, [0; 22]);
    dynamique::fin_retrait();
}
