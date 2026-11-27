//! Une racine SEND non délégable. Aucune ressource DMA ne traverse l'IPC.
use super::*;
const IMAGE: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/net_preparation.bin"));

#[cfg(feature = "net-ethernet-lab")]
fn lots() -> u64 {
    match option_env!("NOVAOS_NET_SERIE") {
        Some("endurance") => 6250,
        Some("perf-long-rx" | "stockage") => 62500,
        Some("perf-tx" | "perf-rx" | "perf-duplex" | "perf-rtt") => 31250,
        Some("court-tx" | "court-rx" | "court-duplex" | "court-rtt") => 6,
        Some("rearmement" | "redemarrage" | "pression-rx") => 3,
        _ => 1,
    }
}

#[cfg(feature = "net-ethernet-lab")]
fn profil() -> u64 {
    match option_env!("NOVAOS_NET_SERIE") {
        Some("perf-tx" | "court-tx")=>1, Some("perf-rx" | "perf-long-rx" | "court-rx")=>2,
        Some("perf-duplex" | "court-duplex")=>3, Some("perf-rtt" | "court-rtt")=>4,
        _=>0,
    }
}

#[cfg(feature = "gestionnaire-4d")]
pub(crate) mod dynamique;

fn creer(mode: u64, observateur: Option<u64>, generation: u64) -> Option<(Child, u64)> {
    let _guard = Guard::new();
    let child = create_image(IMAGE, &mode.to_le_bytes(), None, None).ok()?;
    installer_courtier(child, mode, observateur, generation)
}

fn installer_courtier(child: Child, mode: u64, observateur: Option<u64>, generation: u64) -> Option<(Child,u64)> {
    let pid = child.0;
    let handle = with(|s| {
        let handle = match observateur {
            Some(autre) => autre,
            None => s.ipc.lab_sender(pid)?,
        };
        let count = s.ipc.racines_pilotes(pid);
        assert_eq!(count, usize::from(observateur.is_none()));
        heap::with_pages(|m| {
            let space = s.processes.get(pid).space.as_ref().unwrap();
            space.write_data(m, 1, handle);
            space.write_data(m, 2, generation);
            #[cfg(feature = "net-ethernet-lab")]
            if mode == 3 {
                space.write_data(m, 7, lots());
                space.write_data(m, 9, profil());
                if option_env!("NOVAOS_NET_SERIE")==Some("redemarrage") {space.write_data(m,8,generation.checked_mul(65536).ok_or(crate::memory::Error::Geometry)?);}
            }
            Ok(())
        })
        .map_err(|_| crate::abi::NO_MEMORY)?;
        hal::log(format_args!(
            "NET {} pilote pid={} mode={mode} racines={count} generation={generation}",
            if cfg!(target_arch = "aarch64") {
                "ARM"
            } else {
                "Q35"
            },
            pid.number
        ));
        Ok::<_, u64>(handle)
    });
    match handle {
        Ok(h) => Some((child, h)),
        Err(_) => {
            with(|s| {
                s.ipc.death(pid, ticks());
                s.ipc.collect();
            });
            ipc::undo_unpublished(child);
            None
        }
    }
}
fn demande(pid: Pid, handle: u64, immediat: bool) -> Option<(u64, u64, u64)> {
    let debut = ticks();
    loop {
        match with(|s| s.ipc.lab_pop(pid, handle)) {
            Ok(Some(v)) => return Some(v),
            Ok(None) if ticks().wrapping_sub(debut) < u64::from(HZ) * 3 => if immediat {yield_now()} else {sleep(1)},
            _ => return None,
        }
    }
}
pub(crate) fn tester(generation: u64, preparer: impl FnOnce() -> bool) -> bool {
    let initial = with(|s| s.ipc.pages());
    let Some((actif, handle)) = creer(1, None, generation) else {
        return false;
    };
    let pid = actif.0;
    let mut action = Some(preparer);
    let mut ok =
        creer(2, Some(handle), generation).is_some_and(|(p, _)| p.join() == Outcome::Exit(0));
    for phase in 1..=2 {
        let requete = demande(pid, handle, false);
        let _gate = Guard::new();
        let vivant = with_guard(&_gate, |s| {
            s.processes.get(pid).outcome.is_none() && s.ipc.lab_can_submit(pid, handle)
        });
        let attendu = if phase == 1 {
            generation - 1
        } else {
            generation
        };
        // Tous les champs sont vérifiés à la consommation, sur l'endpoint possédé par ce PID.
        let valide = vivant && requete == Some((1, attendu, 1));
        let statut = if valide && phase == 2 && ok && action.take().is_some_and(|f| f()) {
            crate::abi::OK
        } else {
            crate::abi::DENIED
        };
        ok &= valide && (phase == 1 || statut == crate::abi::OK);
        if !vivant {
            break;
        }
        with_guard(&_gate, |s| {
            heap::with_pages(|m| {
                let space = s.processes.get(pid).space.as_ref().unwrap();
                space.write_data(m, 4, statut);
                space.write_data(m, 3, phase);
                Ok(())
            })
            .unwrap();
        });
        hal::log(format_args!(
            "NET ARM preparation phase={phase} statut={statut} BME=0"
        ));
    }
    let fin = actif.join();
    let (pages, racines) = with(|s| (s.ipc.pages(), s.ipc.racines_pilotes(pid)));
    hal::log(format_args!(
        "NET ARM pilote collecte resultat={fin:?} racines={racines} ipc_avant={initial} ipc_apres={pages}"
    ));
    ok && fin == Outcome::Exit(0) && pages == initial && racines == 0
}

#[cfg(feature = "net-ethernet-lab")]
pub(crate) fn mesurer(phase:&str) { measure(phase); }

#[cfg(feature = "ethernet-v1-port")]
#[path="net_preparation/port_v1.rs"]
pub(crate) mod port_v1;
#[cfg(feature = "ethernet-v1-port")]
pub(crate) use port_v1::ethernet;
#[cfg(feature = "net-q35-duo")]
pub(crate) use port_v1::{EthernetActor, ethernet_actor, nic_exited, disk_isolated, await_first_batch, permit_next_batch};
#[cfg(all(feature = "net-ethernet-lab",not(feature="ethernet-v1-port")))]
pub(crate) fn ethernet(
    generation: u64,
    pendant: impl FnOnce() -> bool,
    mut operation: impl FnMut(u64, usize, &mut [u8]) -> Result<usize, u64>,
) -> bool {
    measure("net_avant_pilote");
    let initial = with(|s| s.ipc.pages());
    let Some((actif, handle)) = creer(3, None, generation) else {
        return false;
    };
    let pid = actif.0;
    let mut copie = [0u8; 1526];
    let mut ok = true;
    let mut pendant=Some(pendant);
    let initial_ticks=ticks();
    let initial_idle=with(|s|s.stats.idle_ticks);
    let ops=core::iter::once((1,false)).chain((0..lots()).flat_map(|lot|{
        let milieu=lot!=0 && lot+1!=lots();
        let rx_seul=milieu && profil()==2;
        let collecte=if rx_seul {8} else if milieu && profil()==1 {7} else {3};
        (0..34).filter_map(move |i|match i {
            0..=16 if rx_seul => None,
            0..=16 =>Some((2,i==16)),
            17..=32 =>Some((collecte,false)),
            _=>Some((if lot+1==lots() {4} else {6},false)),
        })
    }));
    for (indice,(tag,pression)) in ops.enumerate() {
        let seq=indice as u64+1;
        if seq == 19 {
            measure("net_files_pleines");
            if !pendant.take().is_some_and(|f|f()) {ok=false;break}
        }
        if option_env!("NOVAOS_NET_SERIE")==Some("pression-rx") && tag==6 && seq==35 {
            hal::log(format_args!("NET RX fenetre 16 completions avant rearmement"));
            sleep(u64::from(HZ)/8);
        }
        if demande(pid, handle, lots()>1) != Some((tag, generation, seq)) {
            ok = false;
            break;
        }
        let debut = ticks();
        loop {
            // Le contrôle d'instance, l'accès à la copie et la publication de descripteur
            // sont indivisibles sur le BSP. Relâcher la garde avant l'attente d'une IRQ.
            let gate = Guard::new();
            let res = with_guard(&gate, |s| {
                if !s.ipc.lab_can_submit(pid, handle) || s.processes.get(pid).outcome.is_some() {
                    return Err(crate::abi::REVOKED);
                }
                let space = s
                    .processes
                    .get(pid)
                    .space
                    .as_ref()
                    .ok_or(crate::abi::REVOKED)?;
                let len = heap::with_pages(|m| Ok(space.read_data(m, 6)))
                    .map_err(|_| crate::abi::NO_MEMORY)? as usize;
                if (tag == 2
                    && (!(26..=1526).contains(&len)
                        || !heap::with_pages(|m| {
                            Ok(space.copy_from_user(m, 0x401100, &mut copie[..len]))
                        })
                        .unwrap_or(false)))
                    || (tag != 2 && len != 0)
                {
                    return Err(crate::abi::BAD_ARGUMENT);
                }
                let n = operation(tag, len, &mut copie)?;
                if matches!(tag,3|8)
                    && (n > copie.len()
                        || !heap::with_pages(|m| Ok(space.copy_to_user(m, 0x401100, &copie[..n])))
                            .unwrap_or(false))
                {
                    return Err(crate::abi::BAD_ARGUMENT);
                }
                Ok(n)
            });
            let res = if matches!(tag,3|7|8)
                && res == Err(crate::abi::WOULD_BLOCK)
                && ticks().wrapping_sub(debut) >= u64::from(HZ) * 3
            {
                Err(crate::abi::TIMED_OUT)
            } else {
                res
            };
            if matches!(tag,3|7|8) && res == Err(crate::abi::WOULD_BLOCK) {
                drop(gate);
                sleep(1);
                continue;
            }
            with_guard(&gate, |s| {
                if let Some(space) = s.processes.get(pid).space.as_ref() {
                    heap::with_pages(|m| {
                        space.write_data(m, 5, res.unwrap_or(0) as u64);
                        space.write_data(m, 4, res.err().unwrap_or(crate::abi::OK));
                        space.write_data(m, 3, seq);
                        Ok(())
                    })
                    .unwrap();
                }
            });
            if pression && res == Err(crate::abi::WOULD_BLOCK) {
                if seq!=18 {break}
                hal::log(format_args!(
                    "NET L2 saturation TX=16 statut={}",
                    crate::abi::WOULD_BLOCK
                ));
                break;
            }
            if pression {
                ok = false;
            }
            if let Err(reason) = res {
                hal::log(format_args!(
                    "NET L2 refus seq={seq} tag={tag} statut={reason}"
                ));
                ok = false;
            }
            break;
        }
        if !ok {
            break;
        }
    }
    // Fermer l'endpoint avant la collecte. L'appelant retire le domaine et les pages.
    let total_ticks=ticks().wrapping_sub(initial_ticks);
    let idle=with(|s|s.stats.idle_ticks).wrapping_sub(initial_idle);
    hal::log(format_args!("NET L2 CPU ticks={total_ticks} idle={idle} hz={HZ}"));
    ok &= operation(5, 0, &mut copie).is_ok();
    with(|s| s.ipc.lab_close_sender(pid, handle));
    let fin = actif.join();
    let (pages, racines) = with(|s| (s.ipc.pages(), s.ipc.racines_pilotes(pid)));
    measure("net_apres_pilote");
    hal::log(format_args!(
        "NET L2 pilote fin={fin:?} racines={racines} ipc_avant={initial} ipc_apres={pages}"
    ));
    ok && fin == Outcome::Exit(0) && racines == 0 && pages == initial
}
