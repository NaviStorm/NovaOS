//! ELF de développement x86_64. Le mode sans interlocuteur effectue un
//! diagnostic CPU. Seul le marqueur explicite du banc accepte la boucle L2.
#![no_std]
#![no_main]

mod adaptateur;
#[path = "../ethernet/backend.rs"]
mod backend;
mod boucle;
#[allow(dead_code)]
#[path = "../ethernet/canal.rs"]
mod canal;
mod profil;
#[allow(dead_code)]
#[path = "../storage/runtime.rs"]
mod runtime;
mod service_simule;

use boucle::{Boucle, PAIR, STATION};
use ethernet_v1::Binding;
use runtime::*;

fn identite_diagnostic() -> Binding {
    // Identité fictive exclusivement pour le diagnostic sans interlocuteur.
    Binding {
        boot: *b"E2400-model-5A!!",
        port: 1,
        owner: 2,
        incarnation: 1,
        channel: 1,
        epoch: 1,
    }
}

fn diagnostic(tampons: &mut boucle::Tampons) -> Result<()> {
    if profil::cadres_dma() != 259
        || !profil::bases_anneaux_valides([0, 4096, 8192])
        || profil::REGIONS
            .iter()
            .filter(|r| r.acces == profil::AccesNic::Lecture)
            .map(|r| r.cadres())
            .sum::<usize>()
            != 130
        || profil::REGIONS[2].role != "rrd:w"
    {
        return Err(REFUSED);
    }
    let mut port = Boucle::nouvelle(identite_diagnostic(), tampons).map_err(|_| REFUSED)?;
    let mut trame = [0u8; 1514];
    trame[..6].copy_from_slice(&PAIR);
    trame[6..12].copy_from_slice(&STATION);
    trame[12..14].copy_from_slice(&[0x88, 0xb5]);
    let mut retour = [0u8; 1514];
    for lot in 0..33 {
        for coupon in 0..8 {
            trame[14..18].copy_from_slice(&(u32::from(8u16 * lot + coupon as u16)).to_be_bytes());
            port.preparer(coupon, &trame).map_err(|_| REFUSED)?;
        }
        port.publier(3).map_err(|_| REFUSED)?;
        port.publier(5).map_err(|_| REFUSED)?;
        if port.completions(8) != 255 {
            return Err(REFUSED);
        }
        for i in 0..8 {
            if port.recevoir(&mut retour).map_err(|_| REFUSED)? != Some(trame.len())
                || retour[..6] != STATION
                || retour[6..12] != PAIR
                || retour[18..].iter().any(|&b| b != 0xa5)
            {
                return Err(REFUSED);
            }
            let seq = 8u16 * lot + i;
            let mut attendu = u32::from(seq).to_be_bytes();
            if seq < 16 {
                for b in &mut attendu[1..] {
                    *b ^= 0xa5;
                }
            }
            if retour[14..18] != attendu {
                return Err(REFUSED);
            }
        }
    }
    port.fermer().map_err(|_| REFUSED)?;
    let k = identite_diagnostic();
    let mut a = adaptateur::Adaptateur::nouveau(
        k,
        adaptateur::Filtre::laboratoire(STATION, PAIR).map_err(|_| REFUSED)?,
    )
    .map_err(|_| REFUSED)?;
    for i in 0..4 {
        a.preparer(k, i, &trame, &mut retour).map_err(|_| REFUSED)?;
    }
    let perte = a.publication_incertaine(k, 3).map_err(|_| REFUSED)?;
    if (
        perte.tx_incertains,
        perte.tx_annules,
        perte.cadres_a_conserver_si_exposes,
    ) != (3, 1, 259)
        || a.completions(k, 3) != Err(adaptateur::Erreur::Quarantaine)
    {
        return Err(REFUSED);
    }
    let texte = b"E2400 5A simulation CPU 264 TX/RX partiel=3+5 boucle=OK incertains=3 annule=1";
    value(call(
        DEBUG_WRITE,
        texte.as_ptr() as u64,
        texte.len() as u64,
        0,
    ))?;
    Ok(())
}

const MODE_SIMULATION: u64 = 0x4532_3430_3053_494d;
#[unsafe(no_mangle)]
extern "C" fn run() -> ! {
    static mut MEMOIRE: boucle::Tampons = boucle::Tampons::VIDES;
    static DISPONIBLE: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(true);
    if !DISPONIBLE.swap(false, core::sync::atomic::Ordering::AcqRel) {
        exit(REFUSED);
    }
    // Sûreté : une seule prise pour la vie du processus. Cette zone .bss est
    // exclusivement CPU, même dans le mode connecté. Elle n'est jamais exportée.
    let tampons = unsafe { &mut *(&raw mut MEMOIRE) };
    let resultat = match (config(0), config(1), config(2)) {
        (0, 0, 0) => diagnostic(tampons),
        (MODE_SIMULATION, 0, epoch) if epoch != 0 => {
            let mut boot = [0; 16];
            boot[..8].copy_from_slice(&config(7).to_le_bytes());
            boot[8..].copy_from_slice(&config(8).to_le_bytes());
            let session = Binding {
                boot,
                port: 1,
                owner: config(5),
                incarnation: config(18),
                channel: config(9),
                epoch,
            };
            canal::Canal::new(config(15), config(14), config(14))
                .and_then(|canal| service_simule::servir(canal, session, tampons, config(16) == 3))
        }
        _ => Err(REFUSED),
    };
    exit(resultat.err().unwrap_or(OK))
}
