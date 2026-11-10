//! Comparaisons P3/P4 sur le même support et les mêmes charges utiles.
use super::{IDS, LIVE, PAGE, cache::Cache, fixture, heap_meter::ALLOCATIONS};
use novafs::device::FileImage;
use novafs::pool::{GRAIN, Pool, Type};
use novafs::{Engine, Ids, Mode, Ref, Result};
use std::hint::black_box;
use std::sync::atomic::Ordering;
use std::time::Instant;

fn reference(block: u64) -> Ref {
    // Ces clés ne sortent pas du microbanc. L'admission isolée mesure des blocs
    // déjà validés, sans chronométrer le codec, le hachage ou le périphérique.
    Ref {
        block: block + 2,
        birth: 1,
        hash: [(block % 251) as u8; 32],
    }
}

fn cache_operations() {
    const PAGES: u64 = 256;
    const OPS: u64 = 4096;
    let mut cache = Cache::default();
    // Amorcer l'allocation de stdout pour exclure le journal des cycles.
    println!("microcache pages={PAGES} operations={OPS}");
    for series in 1..=5 {
        let baseline = LIVE.load(Ordering::Relaxed);
        let start = Instant::now();
        for b in 0..PAGES {
            cache.remember(None, reference(b), [b as u8; PAGE]);
        }
        let fill_ns = start.elapsed().as_nanos();
        assert_eq!(cache.stats().pages, PAGES as usize);
        let full = LIVE.load(Ordering::Relaxed);
        let start = Instant::now();
        for b in 0..OPS {
            let key = (b * 73) % PAGES;
            let page = black_box(cache.get(None, reference(key)).unwrap());
            assert_eq!(page[33], key as u8);
        }
        let hot_ns = start.elapsed().as_nanos();
        let evictions = cache.stats().evictions;
        let start = Instant::now();
        for b in PAGES..PAGES + OPS {
            cache.remember(None, reference(b), [b as u8; PAGE]);
        }
        let replace_ns = start.elapsed().as_nanos();
        assert_eq!(cache.stats().evictions - evictions, OPS);
        let start = Instant::now();
        assert_eq!(cache.reclaim(), PAGES as usize);
        let release_ns = start.elapsed().as_nanos();
        let after = LIVE.load(Ordering::Relaxed);
        assert_eq!(after, baseline, "mémoire résiduelle du cache");
        println!(
            "cout_cache serie={series} remplissage_ns={fill_ns} lectures_chaudes_ns={hot_ns} remplacements_ns={replace_ns} liberation_ns={release_ns} alloue={}",
            full - baseline
        );
    }
}

fn phases(sector: u32) -> Result<()> {
    let (mut fs, counts, _) = fixture(2048, sector, false)?;
    fs.create("/contenu")?;
    for i in 0..8 {
        fs.write("/contenu", i * 65536, &[i as u8; 65536])?;
    }
    for series in 1..=5 {
        fs.reclaim_cache();
        for phase in ["froid", "chaud"] {
            let before = fs.cache_stats();
            let reads = counts.reads.get();
            let start = Instant::now();
            for i in 0..8 {
                assert_eq!(fs.read("/contenu", i * 65536, 65536)?, [i as u8; 65536]);
            }
            let elapsed = start.elapsed().as_nanos();
            let after = fs.cache_stats();
            println!(
                "lectures_cache serie={series} phase={phase} lectures={} touches={} absences={} evictions={} duree_ns={elapsed}",
                counts.reads.get() - reads,
                after.hits - before.hits,
                after.misses - before.misses,
                after.evictions - before.evictions
            );
        }
        let d = fs.unmount()?;
        let start = Instant::now();
        fs = Engine::mount(d, IDS, Mode::ReadOnly)?;
        let mount_ns = start.elapsed().as_nanos();
        let start = Instant::now();
        fs.verify()?;
        println!(
            "controle serie={series} montage_ns={mount_ns} verification_ns={}",
            start.elapsed().as_nanos()
        );
    }
    Ok(())
}

const VOLUMES: u8 = 4;
const BATCH: u8 = 8;

fn ids(i: u8) -> Ids {
    Ids {
        fs: [32 + i; 16],
        lv: [64 + i; 16],
    }
}

fn volumes(sector: u32) -> Result<Pool<FileImage>> {
    let mut p = Pool::format(FileImage::create((u64::from(VOLUMES) + 1) * GRAIN, sector)?)?;
    for i in 0..VOLUMES {
        let ids = ids(i);
        p.create_for([8 + i; 16], ids.lv, Some(ids.fs), 1)?;
        let mut fs = Engine::format(p.view(ids.lv, Type::Preparing, ids.fs)?, ids)?;
        fs.create("/journal")?;
        drop(fs.unmount()?);
        p.mark_ready(ids.lv, ids.fs)?;
    }
    Ok(p)
}

fn contents(i: u8, j: u8, series: u8) -> [u8; PAGE] {
    [i * 32 + j + series; PAGE]
}

fn multi_volume(sector: u32) -> Result<()> {
    let mut p = volumes(sector)?;
    for series in 1..=5 {
        let allocs = ALLOCATIONS.load(Ordering::Relaxed);
        let start = Instant::now();
        for i in 0..VOLUMES {
            let ids = ids(i);
            let mut fs = Engine::mount(p.view(ids.lv, Type::Ready, ids.fs)?, ids, Mode::ReadWrite)?;
            for j in 0..BATCH {
                fs.write(
                    "/journal",
                    u64::from(j) * PAGE as u64,
                    &contents(i, j, series),
                )?;
            }
            drop(fs.unmount()?);
        }
        let elapsed = start.elapsed().as_nanos();
        let allocations = ALLOCATIONS.load(Ordering::Relaxed) - allocs;
        check_volumes(&mut p, series)?;
        println!(
            "multi_lv serie={series} volumes={VOLUMES} transactions={} octets={} duree_ns={elapsed} allocations={allocations}",
            VOLUMES * BATCH,
            PAGE * usize::from(VOLUMES * BATCH)
        );
    }
    Ok(())
}

fn check_volumes(p: &mut Pool<FileImage>, series: u8) -> Result<()> {
    for i in 0..VOLUMES {
        let ids = ids(i);
        let fs = Engine::mount(p.view(ids.lv, Type::Ready, ids.fs)?, ids, Mode::ReadOnly)?;
        for j in 0..BATCH {
            assert_eq!(
                fs.read("/journal", u64::from(j) * PAGE as u64, PAGE)?,
                contents(i, j, series)
            );
        }
        fs.verify()?;
    }
    Ok(())
}

pub(super) fn run(sector: u32) -> Result<()> {
    cache_operations();
    phases(sector)?;
    multi_volume(sector)
}

#[cfg(feature = "bench-coordinator")]
pub(super) fn coordinator(sector: u32) -> Result<()> {
    use novafs::pool::PoolCoordinator;
    let mut pool = volumes(sector)?;
    for series in 1..=5 {
        let start = Instant::now();
        let coord = PoolCoordinator::mount(pool)?;
        let mount_ns = start.elapsed().as_nanos();
        let start = Instant::now();
        for j in 0..BATCH {
            // Quatre tickets en vol, deux plans indépendants admis à la fois.
            let mut tickets = (0..VOLUMES)
                .map(|i| coord.request_write(ids(i).lv))
                .collect::<Result<Vec<_>>>()?;
            for (pair, batch) in tickets.chunks_mut(2).enumerate() {
                for (index, ticket) in batch.iter_mut().enumerate().rev() {
                    let volume = (pair * 2 + index) as u8;
                    ticket.prepare(
                        "/journal",
                        u64::from(j) * PAGE as u64,
                        &contents(volume, j, series),
                    )?;
                }
                for ticket in batch {
                    ticket.commit()?;
                }
            }
        }
        let elapsed = start.elapsed().as_nanos();
        pool = Pool::import(coord.into_device()?)?;
        check_volumes(&mut pool, series)?;
        println!(
            "coordinateur serie={series} volumes={VOLUMES} transactions={} octets={} montage_ns={mount_ns} duree_ns={elapsed}",
            VOLUMES * BATCH,
            PAGE * usize::from(VOLUMES * BATCH)
        );
    }
    Ok(())
}
