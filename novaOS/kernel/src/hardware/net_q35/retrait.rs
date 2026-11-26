//! Propriété exclusive des cadres en attente d'une preuve du DRHD du Q35 simple.
use super::*;
use crate::sched::sync::Mutex;

struct Bail {
    generation: u32,
    cadres: [Frame; 30],
    attente: Option<RecuQi>,
}

static QUARANTAINE: Mutex<Option<Bail>> = Mutex::new(None);

pub(super) fn disponible() -> bool {
    QUARANTAINE.lock().is_none()
}

pub(super) fn garer(
    cadres: &mut [Frame; 30],
    generation: u32,
    attente: Option<RecuQi>,
) -> Result<(), &'static str> {
    let mut place = QUARANTAINE.lock();
    if place.is_some() {
        return Err("titulaire de quarantaine present");
    }
    *place = Some(Bail {
        generation,
        cadres: core::mem::replace(cadres, [EMPTY; 30]),
        attente,
    });
    Ok(())
}

pub(super) fn reprendre(
    reg: Region,
    generation: u32,
    cookie: u32,
) -> Result<[Frame; 30], &'static str> {
    let mut place = QUARANTAINE.lock();
    let bail = place.as_ref().ok_or("aucun retrait en attente")?;
    let recu = bail.attente.ok_or("arret NIC incertain")?;
    if bail.generation != generation || recu.cookie != cookie {
        return Err("autre retrait");
    }
    recu.collecter(reg)?;
    if !pairs_fermes()
        || reg.r32(0x1c) != ROOT | QI | TE
        || (0..512).any(|i| bail.cadres[0].mem.r64(i * 8) != 0)
    {
        return Err("retrait incertain");
    }
    Ok(place.take().unwrap().cadres)
}

pub(super) fn attendre(
    reg: Region,
    cadres: &mut [Frame; 30],
    generation: u32,
    recu: RecuQi,
) -> Result<(), &'static str> {
    garer(cadres, generation, Some(recu))?;
    if matches!(
        option_env!("NOVAOS_ETH3_CASE"),
        Some("recu-vtd-absent" | "recu-vtd-differe")
    ) {
        let epingles = QUARANTAINE
            .lock()
            .as_ref()
            .unwrap()
            .cadres
            .iter()
            .filter(|f| f.pa != 0)
            .count();
        if disponible()
            || reprendre(reg, generation - 1, recu.cookie).err() != Some("autre retrait")
            || reprendre(reg, generation, recu.cookie - 1).err() != Some("autre retrait")
        {
            return Err("identite du retrait");
        }
        if option_env!("NOVAOS_ETH3_CASE") == Some("recu-vtd-absent") {
            hal::log(format_args!(
                "E3C VT-d recu non livre generation={generation} cookie={} epingles={epingles} rattachement_refuse=true",
                recu.cookie
            ));
            return Err("recu de retrait indisponible");
        }
        hal::log(format_args!(
            "E3C retrait differe VT-d generation={generation} cookie={} anciennes_references_refusees=true epingles={epingles}",
            recu.cookie
        ));
        return Err("livraison differee");
    }
    *cadres = reprendre(reg, generation, recu.cookie)?;
    Ok(())
}
