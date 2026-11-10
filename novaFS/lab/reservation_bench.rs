//! Mesures P3 sur fichiers ordinaires anonymes. Les compteurs de tas proviennent
//! du même allocateur de laboratoire que les campagnes de référence.
use super::{IDS, LIVE, PAGE, PHASE_PEAK, fixture};
use novafs::device::FileImage;
use novafs::pool::{GRAIN, Pool, Type, VolumeReservation};
use novafs::{Engine, Ids, Result, VolumePolicy};
use std::sync::atomic::Ordering;
use std::time::Instant;

fn live() -> usize {
    LIVE.load(Ordering::Relaxed)
}

fn plans(sector: u32) -> Result<()> {
    let (mut fs, io, _) = fixture(2048, sector, false)?;
    fs.create("/temoin")?;
    fs.write("/temoin", 0, b"valide")?;
    fs.set_volume_policy(VolumePolicy {
        maintenance_blocks: 128,
        logical_quota_bytes: Some(131072),
    })?;
    for bytes in [1, 65536] {
        let payload = vec![0x5a; bytes];
        let offset = 4095;
        fs.prepare_write("/temoin", offset, &payload)?.cancel();
        let receipt = fs.receipt();
        let accounting = fs.accounting()?;
        let baseline = live();
        let writes = io.writes.get();
        let flushes = io.flushes.get();
        PHASE_PEAK.store(baseline, Ordering::Relaxed);
        let start = Instant::now();
        let mut held = 0;
        let mut cost = (0, 0);
        for _ in 0..64 {
            let plan = fs.prepare_write("/temoin", offset, &payload)?;
            held = live() - baseline;
            let c = plan.cost();
            cost = (c.data_blocks, c.metadata_blocks);
            assert_eq!(
                plan.accounting().transaction_reserved,
                c.data_blocks + c.metadata_blocks
            );
            plan.cancel();
            assert_eq!(live(), baseline);
        }
        let duration = start.elapsed().as_nanos();
        let peak = PHASE_PEAK.load(Ordering::Relaxed);
        assert_eq!(fs.receipt(), receipt);
        assert_eq!((io.writes.get(), io.flushes.get()), (writes, flushes));
        assert_eq!(
            fs.accounting()?.available_blocks,
            accounting.available_blocks
        );
        let after = live();
        let start = Instant::now();
        fs.prepare_write("/temoin", offset, &payload)?.commit()?;
        let transaction_ns = start.elapsed().as_nanos();
        assert_eq!(io.flushes.get() - flushes, 2);
        assert_eq!(io.writes.get() - writes, cost.0 + cost.1 + 1);
        assert_eq!(fs.read("/temoin", offset, bytes)?, payload);
        println!(
            "plan octets={bytes} donnees={} metadonnees={} allocation_ram={} tas_avant={baseline} tas_apres={after} plan_tas={held} pic_absolu={peak} cycles=64 duree_ns={duration} commit_ns={transaction_ns} flushs=2",
            cost.0, cost.1, accounting.allocation_metadata_bytes
        );
    }
    fs.verify()?;
    drop(fs.unmount()?);
    Ok(())
}

fn pool(sector: u32) -> Result<()> {
    let disk = FileImage::create(17 * GRAIN, sector)?;
    let before_pool = live();
    let mut pool = Pool::format(disk)?;
    let reserve = |i: usize| {
        pool.reserve_volume(
            [i as u8 + 1; 16],
            [i as u8 + 32; 16],
            Some([i as u8 + 64; 16]),
            1,
        )
    };
    let mut tickets: [Option<VolumeReservation>; 16] = std::array::from_fn(|_| None);
    let cold = live();
    for (i, ticket) in tickets.iter_mut().enumerate() {
        *ticket = Some(reserve(i)?);
    }
    let first = live();
    for ticket in &mut tickets {
        drop(ticket.take());
    }
    let baseline = live();
    PHASE_PEAK.store(baseline, Ordering::Relaxed);
    let start = Instant::now();
    for _ in 0..64 {
        for (i, ticket) in tickets.iter_mut().enumerate() {
            *ticket = Some(reserve(i)?);
        }
        let stats = pool.space_stats()?;
        assert_eq!(
            (
                stats.pending_volumes,
                stats.reserved_blocks,
                stats.available_blocks
            ),
            (16, 16 * GRAIN, 0)
        );
        for ticket in &mut tickets {
            drop(ticket.take());
        }
        assert_eq!(live(), baseline);
    }
    let duration = start.elapsed().as_nanos();
    let peak = PHASE_PEAK.load(Ordering::Relaxed);
    assert_eq!(pool.space_stats()?.available_blocks, 16 * GRAIN);
    println!(
        "tickets nombre=16 octets_un_handle={} tas_froid={cold} tas_seize={first} tas_avant={baseline} tas_apres={} pic_absolu={peak} cycles=64 duree_ns={duration}",
        size_of::<VolumeReservation>(),
        live()
    );
    for i in 0..16 {
        let lv = [i + 32; 16];
        let fsid = [i + 64; 16];
        let ids = Ids { lv, fs: fsid };
        let ticket = pool.reserve_volume([i + 1; 16], lv, Some(fsid), 1)?;
        pool.commit_volume(ticket)?;
        {
            let mut fs = Engine::format(pool.view(lv, Type::Preparing, fsid)?, ids)?;
            fs.create("/id")?;
            fs.write("/id", 0, &[i])?;
            drop(fs.unmount()?);
        }
        pool.mark_ready(lv, fsid)?;
    }
    let disk = pool.into_device();
    let pool = Pool::import(disk)?;
    let mounted_bytes = live() - before_pool;
    assert_eq!(pool.volumes().count(), 16);
    assert_eq!(pool.space_stats()?.assigned_blocks, 16 * GRAIN);
    assert_eq!(pool.space_stats()?.reserved_blocks, 0);
    drop(pool);
    assert_eq!(live(), before_pool);
    println!(
        "catalogue lv=16 octets_pool_vide={} octets_pool_remonte={mounted_bytes} octets_apres_destruction={}",
        cold - before_pool,
        live() - before_pool
    );
    Ok(())
}

pub(super) fn run(sector: u32) -> Result<()> {
    println!(
        "espace secteur={sector} bloc_octets={PAGE} fs={:02x}",
        IDS.fs[0]
    );
    // Chauffer le runtime et l'instrumentation avant le contrôle inter-cycles.
    plans(sector)?;
    pool(sector)?;
    let warm = live();
    plans(sector)?;
    pool(sector)?;
    assert_eq!(
        live(),
        warm,
        "ressources retenues après remontage et destruction"
    );
    println!("restitution_generale avant={warm} apres={}", live());
    Ok(())
}
