//! Un unique test dans ce processus pour attribuer les allocations globales à chaque cycle.
#[path = "../lab/heap_meter.rs"]
mod heap_meter;
use novafs::device::{SimDevice, SimFault, SimSnapshot};
use novafs::shared::SharedPool;
use novafs::shared_export::{export_view, restore_view};
use novafs::{Ids, Result};
use std::io::{Seek, SeekFrom};
use std::sync::atomic::Ordering;

fn cycle(base: &SimSnapshot, id: u64, sector: u32, err: bool) -> Result<()> {
    let dev = base.device();
    let ctl = dev.control();
    let mut pool = SharedPool::open(dev)?;
    let s = pool.snapshot(id)?;
    let c = pool.clone_snapshot(s, [25; 16])?;
    pool.write_file(c, "/f", 0, &[0xcd; 4096])?;
    let mut export = tempfile::tempfile()?;
    export_view(&pool, c, sector, &mut export)?;
    pool.remove(s)?;
    pool.remove(id)?;
    pool.advance()?;
    if err {
        ctl.inject(SimFault::Flush {
            ordinal: 2,
            persist_newest: 1,
        })?;
        assert!(pool.remove(c).is_err());
    } else {
        pool.remove(c)?;
    }
    drop(pool);
    export.seek(SeekFrom::Start(0))?;
    let dev = SimDevice::new(2048, sector)?;
    let mut dest = SharedPool::format(dev, [99; 16])?;
    let restored = restore_view(&mut dest, export, [41; 16])?;
    assert_eq!(dest.read_file(restored, "/f", 0, 8192)?, [0xcd; 4096]);
    Ok(())
}

#[test]
fn restitution_exacte_apres_cycles_normaux_et_f2_en_erreur() -> Result<()> {
    assert!(heap_meter::current_stack_size() > 0);
    for sector in [512, 4096] {
        let dev = SimDevice::new(2048, sector)?;
        let ctl = dev.control();
        let mut p = SharedPool::format(dev, [52; 16])?;
        let id = p.create(
            [23; 16],
            Ids {
                fs: [11; 16],
                lv: [12; 16],
            },
            256,
        )?;
        p.create_file(id, "/f")?;
        p.write_file(id, "/f", 0, &[0x43; 4096])?;
        let base = ctl.snapshot();
        for err in [false, true] {
            cycle(&base, id, sector, err)?;
            let before = heap_meter::LIVE.load(Ordering::Relaxed);
            for _ in 0..32 {
                cycle(&base, id, sector, err)?;
                assert_eq!(heap_meter::LIVE.load(Ordering::Relaxed), before);
            }
            println!(
                "RESTITUTION secteur={sector} F2_erreur={err} cycles=32 avant={before} après={}",
                heap_meter::LIVE.load(Ordering::Relaxed)
            );
        }
    }
    Ok(())
}
