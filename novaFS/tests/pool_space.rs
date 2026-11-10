//! Comparaison des deux catalogues durables, des admissions volatiles et des
//! contenus réels des LV. Les quantités attendues sont calculées en grains.
use novafs::device::{Persistence, SimDevice, SimFault};
use novafs::pool::{GRAIN, Pool, PoolSpaceStats, Type, inspect_space};
use novafs::{Device, Engine, Error, Ids, Mode, Result};
use std::sync::Barrier;

const OWNER: [u8; 16] = [0x71; 16];
const IDS: Ids = Ids {
    lv: [0x21; 16],
    fs: [0x31; 16],
};

fn partition(s: PoolSpaceStats, allocated: u64, deferred: u64, pending: u64, free: u64) {
    assert_eq!(s.metadata_blocks, GRAIN);
    assert_eq!(s.assigned_blocks, allocated * GRAIN);
    assert_eq!(s.deferred_blocks, deferred * GRAIN);
    assert_eq!(s.reserved_blocks, pending * GRAIN);
    assert_eq!(s.available_blocks, free * GRAIN);
    assert_eq!(s.shared_blocks, 0);
    assert_eq!(
        s.total_blocks,
        s.metadata_blocks
            + s.tail_blocks
            + s.assigned_blocks
            + s.deferred_blocks
            + s.reserved_blocks
            + s.available_blocks
    );
}

#[test]
fn p3_pool_admissions_concurrentes_et_attributions_disjointes() -> Result<()> {
    for sector in [512, 4096] {
        let disk = SimDevice::new(5 * GRAIN + 13, sector)?;
        let ctl = disk.control();
        let mut pool = Pool::format(disk)?;
        let gate = Barrier::new(8);
        ctl.trace(None, None);
        let mut winners = std::thread::scope(|scope| {
            let mut handles = Vec::new();
            let pool = &pool;
            let gate = &gate;
            for i in 1u8..=8 {
                handles.push(scope.spawn(move || {
                    gate.wait();
                    (i, pool.reserve_volume([i; 16], [i; 16], None, 1))
                }));
            }
            let mut successes = Vec::new();
            for handle in handles {
                let (i, result) = handle.join().unwrap();
                match result {
                    Ok(ticket) => successes.push((i, ticket)),
                    Err(Error::PoolFull) => {}
                    Err(err) => panic!("admission inattendue {err}"),
                }
            }
            successes
        });
        assert_eq!(winners.len(), 4);
        assert_eq!(pool.space_stats()?.pending_volumes, 4);
        assert_eq!(pool.space_stats()?.tail_blocks, 13);
        partition(pool.space_stats()?, 0, 0, 4, 0);
        partition(inspect_space(&ctl.snapshot().device())?, 0, 0, 0, 4);
        drop(winners.pop().unwrap());
        partition(pool.space_stats()?, 0, 0, 3, 1);
        winners.push((9, pool.reserve_volume([9; 16], [9; 16], None, 1)?));
        assert!(ctl.events().is_empty());
        let mut ids = Vec::new();
        for (i, ticket) in winners.into_iter().rev() {
            pool.commit_volume(ticket)?;
            ids.push(i);
            assert!(pool.view([i; 16], Type::Raw, [0; 16]).is_err());
            assert!(pool.raw_for([0x99; 16], [i; 16]).is_err());
            let mut vol = pool.raw_for([i; 16], [i; 16])?;
            for b in 0..GRAIN {
                assert_eq!(vol.read(b)?, [0; 4096]);
                vol.write(b, &[i; 4096])?;
            }
            vol.flush()?;
        }
        partition(pool.space_stats()?, 4, 0, 0, 0);
        assert!(matches!(
            pool.reserve_volume(OWNER, IDS.lv, Some(IDS.fs), u64::MAX),
            Err(Error::QuotaExceeded)
        ));
        assert!(matches!(
            pool.reserve_volume(OWNER, IDS.lv, Some(IDS.fs), 1),
            Err(Error::PoolFull)
        ));
        let mut restored = Pool::import(pool.into_device())?;
        for i in ids {
            let vol = restored.raw_for([i; 16], [i; 16])?;
            for b in 0..GRAIN {
                assert_eq!(vol.read(b)?, [i; 4096]);
            }
        }
        assert_eq!(
            restored.space_stats()?,
            inspect_space(&ctl.snapshot().device())?
        );
    }
    Ok(())
}

#[test]
fn p3_pool_quota_garantie_fichiers_et_recyclage() -> Result<()> {
    for sector in [512, 4096] {
        let disk = SimDevice::new(5 * GRAIN, sector)?;
        let ctl = disk.control();
        let mut pool = Pool::format(disk)?;
        let first = pool.reserve_volume(OWNER, IDS.lv, Some(IDS.fs), 2)?;
        assert!(matches!(
            pool.reserve_volume(OWNER, [3; 16], None, 3),
            Err(Error::QuotaExceeded)
        ));
        let second = pool.reserve_volume([0x72; 16], [0x41; 16], None, 2)?;
        pool.commit_volume(first)?;
        let mut fs = Engine::format(pool.view(IDS.lv, Type::Preparing, IDS.fs)?, IDS)?;
        fs.create("/recu")?;
        fs.write("/recu", 0, b"marqueur du volume A")?;
        fs.unmount()?;
        pool.mark_ready(IDS.lv, IDS.fs)?;
        pool.commit_volume(second)?;
        partition(pool.space_stats()?, 4, 0, 0, 0);
        let volume = pool.view(IDS.lv, Type::Ready, IDS.fs)?;
        assert_eq!(volume.logical_quota_bytes(), Some(2 * GRAIN * 4096));
        let mut fs = Engine::mount(volume, IDS, Mode::ReadWrite)?;
        let receipt = fs.receipt();
        ctl.trace(None, None);
        assert!(matches!(
            fs.write("/recu", 2 * GRAIN * 4096, &[9]),
            Err(Error::QuotaExceeded)
        ));
        assert_eq!(fs.receipt(), receipt);
        assert_eq!(fs.read("/recu", 0, 128)?, b"marqueur du volume A");
        assert!(ctl.events().is_empty());
        fs.unmount()?;

        let saved = pool.space_stats()?;
        pool.remove([0x41; 16])?;
        partition(pool.space_stats()?, 2, 2, 0, 0);
        assert!(matches!(
            pool.reserve_volume(OWNER, [0x41; 16], None, 1),
            Err(Error::Busy)
        ));
        assert!(matches!(
            pool.reserve_volume([0x72; 16], [0x51; 16], None, 1),
            Err(Error::PoolFull)
        ));
        let mut restored = Pool::import(pool.into_device())?;
        assert_eq!(
            restored.space_stats()?,
            inspect_space(&ctl.snapshot().device())?
        );
        restored.advance()?;
        partition(restored.space_stats()?, 2, 0, 0, 2);
        {
            let _aborted = restored.reserve_volume(OWNER, [0x61; 16], None, 2)?;
            partition(restored.space_stats()?, 2, 0, 2, 0);
        }
        partition(restored.space_stats()?, 2, 0, 0, 2);
        restored.create_for([0x72; 16], [0x51; 16], None, 2)?;
        assert_eq!(restored.space_stats()?, saved);
        let fs = Engine::mount(
            restored.view(IDS.lv, Type::Ready, IDS.fs)?,
            IDS,
            Mode::ReadOnly,
        )?;
        fs.verify()?;
        assert_eq!(fs.read("/recu", 0, 128)?, b"marqueur du volume A");
        fs.unmount()?;
        restored.remove(IDS.lv)?;
        assert!(matches!(
            restored.reserve_volume(OWNER, [0x62; 16], Some(IDS.fs), 1),
            Err(Error::Busy)
        ));
        restored.advance()?;
        let _new = restored.reserve_volume(OWNER, [0x62; 16], Some(IDS.fs), 1)?;
    }
    Ok(())
}

#[test]
fn p3_pool_coupure_et_erreurs_de_publication() -> Result<()> {
    for sector in [512, 4096] {
        let disk = SimDevice::new(3 * GRAIN, sector)?;
        let ctl = disk.control();
        let mut pool = Pool::format(disk)?;
        pool.create_for(OWNER, [1; 16], None, 1)?;
        let mut raw = pool.raw_for(OWNER, [1; 16])?;
        raw.write(GRAIN - 1, &[0x91; 4096])?;
        raw.flush()?;
        drop(raw);
        let baseline = ctl.snapshot();
        let stale = pool.reserve_volume(OWNER, [2; 16], None, 1)?;
        ctl.power_cut(Persistence::None)?;
        let mut after = Pool::import(pool.into_device())?;
        assert!(matches!(after.commit_volume(stale), Err(Error::Invalid(_))));
        assert_eq!(baseline.bytes()?, ctl.snapshot().bytes()?);
        partition(after.space_stats()?, 1, 0, 0, 1);
        for (fault, published, indeterminate) in [
            (
                SimFault::Write {
                    block: 2 * GRAIN,
                    prefix: 99,
                },
                false,
                false,
            ),
            (
                SimFault::Flush {
                    ordinal: 1,
                    persist_newest: 4,
                },
                false,
                false,
            ),
            (
                SimFault::Flush {
                    ordinal: 2,
                    persist_newest: 0,
                },
                false,
                true,
            ),
            (
                SimFault::Flush {
                    ordinal: 2,
                    persist_newest: 1,
                },
                true,
                true,
            ),
        ] {
            let disk = baseline.device();
            let ctl = disk.control();
            let mut p = Pool::import(disk)?;
            let tx = p.reserve_volume(OWNER, [2; 16], None, 1)?;
            ctl.inject(fault)?;
            let fail = p.commit_volume(tx).unwrap_err();
            if indeterminate {
                assert!(matches!(fail, Error::Publication { .. }));
                assert!(matches!(p.space_stats(), Err(Error::UnknownOutcome)));
            } else {
                assert!(matches!(fail, Error::Io(_)));
                assert!(matches!(p.space_stats(), Err(Error::Unavailable)));
            }
            ctl.power_cut(Persistence::None)?;
            let mut mounted = Pool::import(p.into_device())?;
            partition(
                mounted.space_stats()?,
                1 + u64::from(published),
                0,
                0,
                u64::from(!published),
            );
            assert_eq!(
                mounted.raw_for(OWNER, [1; 16])?.read(GRAIN - 1)?,
                [0x91; 4096]
            );
            if published {
                let new = mounted.raw_for(OWNER, [2; 16])?;
                for b in 0..GRAIN {
                    assert_eq!(new.read(b)?, [0; 4096]);
                }
            }
        }
    }
    Ok(())
}

#[test]
fn p3_pool_corruption_pendant_derniere_reservation() -> Result<()> {
    for sector in [512, 4096] {
        let disk = SimDevice::new(3 * GRAIN, sector)?;
        let ctl = disk.control();
        let mut p = Pool::format(disk)?;
        p.create_for(OWNER, [1; 16], None, 1)?;
        let ticket = p.reserve_volume(OWNER, [2; 16], None, 1)?;
        partition(p.space_stats()?, 1, 0, 1, 0);
        let image = ctl.snapshot().device();
        let slot = p.generation() % 2;
        let mut bad = image.read(slot)?;
        bad[82] ^= 0x40;
        ctl.replace_stable(slot, bad)?;
        let before = ctl.snapshot().bytes()?;
        ctl.trace(None, None);
        assert!(matches!(p.commit_volume(ticket), Err(Error::Corrupt(_))));
        assert!(matches!(p.space_stats(), Err(Error::Unavailable)));
        assert!(matches!(
            inspect_space(&ctl.snapshot().device()),
            Err(Error::Corrupt(_))
        ));
        assert!(Pool::import(ctl.snapshot().device()).is_err());
        assert!(ctl.events().is_empty());
        assert_eq!(before, ctl.snapshot().bytes()?);
    }
    Ok(())
}

#[test]
fn p3_pool_comptes_logiques_hors_ligne_sans_ecriture() -> Result<()> {
    for sector in [512, 4096] {
        let dev = SimDevice::new(4 * GRAIN + 11, sector)?;
        let ctl = dev.control();
        let mut pool = Pool::format(dev)?;
        pool.create_for(OWNER, IDS.lv, Some(IDS.fs), 1)?;
        let mut fs = Engine::format(pool.view(IDS.lv, Type::Preparing, IDS.fs)?, IDS)?;
        fs.create("/creux")?;
        fs.write("/creux", 8192, &[0x39; 4096])?;
        fs.unmount()?;
        pool.mark_ready(IDS.lv, IDS.fs)?;
        pool.create_for(OWNER, [0x55; 16], None, 1)?;
        pool.create_for(OWNER, [0x56; 16], Some([0x57; 16]), 1)?;
        let disk = pool.into_device();
        let before = ctl.snapshot().bytes()?;
        ctl.trace(None, None);
        let (stats, volumes) = novafs::pool::inspect_usage(&disk)?;
        partition(stats, 3, 0, 0, 0);
        assert_eq!(volumes.len(), 3);
        assert_eq!(volumes[0].0.kind, Type::Ready);
        let ready = volumes[0].1.as_ref().unwrap();
        assert_eq!(ready.logical_bytes, 12288);
        assert_eq!(ready.logical_quota_bytes, Some(GRAIN * 4096));
        assert_eq!(volumes[1].0.kind, Type::Raw);
        assert!(volumes[1].1.is_none());
        assert_eq!(volumes[2].0.kind, Type::Preparing);
        assert!(volumes[2].1.is_none());
        assert!(ctl.events().is_empty());
        assert_eq!(before, ctl.snapshot().bytes()?);
        let damaged = before
            .chunks_exact(4096)
            .position(|block| block == [0x39; 4096])
            .unwrap() as u64;
        ctl.replace_stable(damaged, [0xa8; 4096])?;
        assert_eq!(novafs::pool::inspect_space(&disk)?, stats);
        assert!(matches!(
            novafs::pool::inspect_usage(&disk),
            Err(Error::Corrupt(_))
        ));
        assert!(ctl.events().is_empty());
    }
    Ok(())
}

#[test]
fn p3_pool_publication_initiale_et_relecture() -> Result<()> {
    use novafs::device::Event;
    for sector in [512, 4096] {
        let dev = SimDevice::new(3 * GRAIN + 3, sector)?;
        let ctl = dev.control();
        ctl.trace(None, None);
        let pool = Pool::format(dev)?;
        let flushes: Vec<_> = ctl
            .events()
            .into_iter()
            .filter_map(|e| match e {
                Event::FlushCompleted(n) => Some(n),
                _ => None,
            })
            .collect();
        assert_eq!(flushes, [1, 2]);
        partition(pool.space_stats()?, 0, 0, 0, 2);
        for slot in [0, 1] {
            let dev = SimDevice::new(3 * GRAIN + 3, sector)?;
            let ctl = dev.control();
            ctl.inject(SimFault::Write {
                block: slot,
                prefix: 53,
            })?;
            assert!(matches!(
                Pool::format(dev),
                Err(Error::Publication {
                    phase: "ancre initiale du pool",
                    ..
                })
            ));
            ctl.power_cut(Persistence::Newest(usize::MAX))?;
            assert!(Pool::import(ctl.snapshot().device()).is_err());
        }
        let dev = SimDevice::new(3 * GRAIN + 3, sector)?;
        let ctl = dev.control();
        ctl.inject(SimFault::Flush {
            ordinal: 2,
            persist_newest: 2,
        })?;
        assert!(matches!(
            Pool::format(dev),
            Err(Error::Publication {
                phase: "F2 initial du pool",
                ..
            })
        ));
        ctl.power_cut(Persistence::None)?;
        let recovered = Pool::import(ctl.snapshot().device())?;
        assert_eq!(recovered.generation(), 1);
        partition(recovered.space_stats()?, 0, 0, 0, 2);
        assert_eq!(recovered.space_stats()?, pool.space_stats()?);
    }
    Ok(())
}
