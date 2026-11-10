//! Un seul test dans ce processus pour comparer les allocations vivantes après chaque cycle.
#[path = "../lab/heap_meter.rs"]
#[allow(dead_code)]
mod heap_meter;
use novafs::device::{SimDevice, SimFault};
use novafs::shared::SharedPool;
use novafs::{Compression, Ids, Result};
use std::sync::atomic::Ordering;

fn cycle(sector: u32, echec: bool) -> Result<()> {
    let disk = SimDevice::new(2048, sector)?;
    let ctl = disk.control();
    let mut p = SharedPool::format_p6(disk, [22; 16])?;
    let o = p.create(
        [1; 16],
        Ids {
            fs: [4; 16],
            lv: [5; 16],
        },
        256,
    )?;
    p.set_compression(o, Compression::Lz4)?;
    p.create_file(o, "/f")?;
    p.write_file(o, "/f", 0, &[7; 65536])?;
    let s = p.snapshot(o)?;
    let c = p.clone_snapshot(s, [3; 16])?;
    if echec {
        ctl.inject(SimFault::Flush {
            ordinal: 2,
            persist_newest: 0,
        })?;
        assert!(p.write_file(c, "/f", 4095, &[8; 8192]).is_err());
    } else {
        p.write_file(c, "/f", 4095, &[8; 8192])?;
        p = SharedPool::open(p.into_device())?;
        assert_eq!(p.read_file(s, "/f", 4095, 8192)?, [7; 8192]);
        p.remove(o)?;
        p.remove(s)?;
        p.remove(c)?;
        p.advance()?;
    }
    Ok(())
}

#[test]
fn restitution_tampons_et_aretes_compressees() -> Result<()> {
    for sector in [512, 4096] {
        for echec in [false, true] {
            cycle(sector, echec)?;
            let before = heap_meter::LIVE.load(Ordering::Relaxed);
            for _ in 0..32 {
                cycle(sector, echec)?;
                assert_eq!(heap_meter::LIVE.load(Ordering::Relaxed), before);
            }
            println!(
                "P6_MEMOIRE secteur={sector} F2_erreur={echec} cycles=32 avant={before} apres={before}"
            );
        }
    }
    Ok(())
}
