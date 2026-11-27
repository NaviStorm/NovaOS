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
}

// Le superviseur garde les cadres épinglés tant que cette vue est installée.
// Elle est retirée avant le premier release_dma, y compris en cas de reprise.
static VUE: Mutex<Option<Vue>> = Mutex::new(None);

pub(crate) fn verifier_port_selectionne(id: u64, sid: u64) -> bool {
    sid == u64::from(SID) && id == 0x10411af4 && u64::from(pci_ports::port(SID, 0, 4, None)) == id
        && commande(SID) & 4 == 0
}

pub(super) fn selectionner() -> Result<(dynamique::Cycle, dynamique::Decision), &'static str> {
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
        return (f[7].mem.r32(0) == cookie && reg.r64(0x80) == reg.r64(0x88)
            && pairs_fermes() && (0..512).all(|i| f[0].mem.r64(i * 8) == 0)).then_some(0);
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
        *vue = Some(Vue {generation: g, reg, frames: *frames, recu: None});
    }
    let pages = core::array::from_fn(|i| frames[8+i].pa);
    if !reprise::lot_vtd(g, pages) { return Err("bail Q35 non corrobore"); }
    EthernetActor::reserve(g, Some(decision)).ok_or("admission dynamique Q35")
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

pub(super) fn restitue(g: u64) {
    reprise::rendu(g, [0; 22]);
    dynamique::fin_retrait();
}
