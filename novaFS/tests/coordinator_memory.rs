//! Une seule épreuve dans cet exécutable : le compteur global du laboratoire
//! peut ainsi comparer les allocations vivantes du processus entre deux cycles.
#[path = "../lab/heap_meter.rs"]
mod heap_meter;
use novafs::device::{Persistence, SimDevice, SimFault, SimSnapshot};
use novafs::pool::{GRAIN, Pool, PoolCoordinator, Type};
use novafs::{Engine, Error, Ids, Result};
use std::sync::atomic::Ordering;

fn base(sector: u32) -> Result<SimSnapshot> {
    let disk = SimDevice::new(3 * GRAIN, sector)?;
    let control = disk.control();
    let mut p = Pool::format(disk)?;
    for n in 1..=2 {
        let ids = Ids {
            lv: [n; 16],
            fs: [n + 32; 16],
        };
        p.create(ids.lv, Some(ids.fs), 1)?;
        {
            let mut fs = Engine::format(p.view(ids.lv, Type::Preparing, ids.fs)?, ids)?;
            fs.create("/f")?;
            fs.write("/f", 0, b"init")?;
        }
        p.mark_ready(ids.lv, ids.fs)?;
    }
    Ok(control.snapshot())
}

fn cycle(snapshot: &SimSnapshot, unknown: bool) -> Result<()> {
    let disk = snapshot.device();
    let ctl = disk.control();
    let c = PoolCoordinator::mount(Pool::import(disk)?)?;
    for n in 1..=2 {
        assert_eq!(c.request_read([n; 16])?.read("/f", 0, 4)?, b"init");
    }
    {
        let mut first = c.request_write([1; 16])?;
        let mut second = c.request_write([2; 16])?;
        first.prepare("/f", 0, &[91; 65536])?;
        second.prepare("/f", 0, &[22; 65536])?;
        assert!(c.reclaim_caches()? > 0);
        if unknown {
            ctl.inject(SimFault::Flush {
                ordinal: 2,
                persist_newest: 0,
            })?;
            assert!(matches!(
                first.commit(),
                Err(Error::Publication { phase: "F2", .. })
            ));
            assert!(matches!(second.commit(), Err(Error::UnknownOutcome)));
        }
        // Les deux tickets sont détruits ici, préparés ou déjà terminés.
    }
    let disk = c.into_device()?;
    ctl.power_cut(Persistence::None)?;
    let clean = PoolCoordinator::mount(Pool::import(disk)?)?;
    for n in 1..=2 {
        clean.request_read([n; 16])?.verify()?;
        assert_eq!(clean.request_read([n; 16])?.read("/f", 0, 4)?, b"init");
    }
    Ok(())
}

#[test]
fn restitution_apres_annulation_et_issue_inconnue() -> Result<()> {
    #[cfg(target_os = "macos")]
    assert!(heap_meter::current_stack_size() > 0);
    for sector in [512, 4096] {
        let seed = base(sector)?;
        for unknown in [false, true] {
            cycle(&seed, unknown)?;
            let before = heap_meter::LIVE.load(Ordering::Relaxed);
            heap_meter::PHASE_PEAK.store(before, Ordering::Relaxed);
            for _ in 0..64 {
                cycle(&seed, unknown)?;
                assert_eq!(heap_meter::LIVE.load(Ordering::Relaxed), before);
            }
            println!(
                "coord_restitution secteur={sector} F2_indetermine={unknown} cycles=64 tas_avant={before} tas_apres={} pic={}",
                heap_meter::LIVE.load(Ordering::Relaxed),
                heap_meter::PHASE_PEAK.load(Ordering::Relaxed)
            );
        }
    }
    Ok(())
}
