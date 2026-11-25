//! Une racine SEND non délégable. Aucune ressource DMA ne traverse l'IPC.
use super::*;
const IMAGE: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/net_preparation.bin"));

fn creer(mode: u64, observateur: Option<u64>, generation: u64) -> Option<(Child, u64)> {
    let _guard = Guard::new();
    let child = create_image(IMAGE, &mode.to_le_bytes(), None, None).ok()?;
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
            Ok(())
        })
        .map_err(|_| crate::abi::NO_MEMORY)?;
        hal::log(format_args!(
            "NET ARM pilote pid={} mode={mode} racines={count} generation={generation}",
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
fn demande(pid: Pid, handle: u64) -> Option<(u64, u64, u64)> {
    let debut = ticks();
    loop {
        match with(|s| s.ipc.lab_pop(pid, handle)) {
            Ok(Some(v)) => return Some(v),
            Ok(None) if ticks().wrapping_sub(debut) < u64::from(HZ) * 3 => sleep(1),
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
        let requete = demande(pid, handle);
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
