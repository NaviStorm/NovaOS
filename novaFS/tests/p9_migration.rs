// SPDX-License-Identifier: MPL-2.0
use novafs::crypto::{Credential, HostEntropy, Secret};
use novafs::device::{SimDevice, SimSnapshot};
use novafs::secure_pool::{
    SecurePool, ViewParent, inspect_header,
    migration::{Enrollment, MigrationReceipt},
};
use novafs::time::Times;
use novafs::{Compression, Device, Error, Ids, Result};
fn identical(a: &SimSnapshot, b: &SimSnapshot) -> Result<bool> {
    let a = a.device();
    let b = b.device();
    for i in 0..a.blocks() {
        if a.read(i)? != b.read(i)? {
            return Ok(false);
        }
    }
    Ok(true)
}
#[test]
fn migration_chiffree_lignees_et_recus() -> Result<()> {
    for sector in [512, 4096] {
        let admin = Secret::generate(&mut HostEntropy)?;
        let next = Secret::generate(&mut HostEntropy)?;
        let recovery = Secret::generate(&mut HostEntropy)?;
        let other = Secret::generate(&mut HostEntropy)?;
        let disk = SimDevice::new(3600, sector)?;
        let control = disk.control();
        let mut src = SecurePool::format(disk, [2; 16], &admin)?;
        let o = src.create(
            [1; 16],
            Ids {
                fs: [6; 16],
                lv: [7; 16],
            },
            256,
            Compression::Lz4,
            Some((b"ancienne", &recovery)),
        )?;
        let d = src.domain(o)?;
        src.mkdir(o, "/rep")?;
        src.create_file(o, "/rep/f")?;
        let original = vec![77; 65536];
        src.write_file(o, "/rep/f", 0, &original)?;
        let s = src.snapshot(o)?;
        let c = src.clone_snapshot(s, [5; 16])?;
        src.write_file(c, "/rep/f", 77, b"xy")?;
        let twin = src.snapshot(c)?;
        let distinct = src.create(
            [9; 16],
            Ids {
                fs: [16; 16],
                lv: [17; 16],
            },
            256,
            Compression::None,
            Some((b"indep", &recovery)),
        )?;
        let second = src.domain(distinct)?;
        src.remove(o)?;
        let saved = control.snapshot();
        let enroll = [
            Enrollment {
                old_domain: d,
                phrase: b"nouveau",
                recovery: &other,
            },
            Enrollment {
                old_domain: second,
                phrase: b"second",
                recovery: &other,
            },
        ];
        let (disk, r) =
            src.migrate_p9(SimDevice::new(3600, sector)?, &next, &enroll, HostEntropy)?;
        assert!(r.units() > 0);
        src.verify_migration_source(&r)?;
        assert!(identical(&saved, &control.snapshot())?);
        let id = inspect_header(&disk)?;
        assert_ne!(id, [2; 16]);
        let mut pool = SecurePool::open(disk, id, &next)?;
        let new = pool.domain(s)?;
        let new2 = pool.domain(distinct)?;
        assert_ne!(new, d);
        assert_ne!(new, new2);
        assert_ne!(new2, second);
        assert!(pool.unlock(new, Credential::Recovery(&recovery)).is_err());
        pool.unlock(new, Credential::Recovery(&other))?;
        pool.unlock(new2, Credential::Passphrase(b"second"))?;
        pool.check()?;
        assert_eq!(pool.migration_receipt()?, Some(r.clone()));
        src.verify_migration_pair(&mut pool)?;
        assert_eq!(pool.inspect_file(s, "/rep/f", 0, 65536)?, original);
        let mut changed = original.clone();
        changed[77..79].copy_from_slice(b"xy");
        assert_eq!(pool.inspect_file(c, "/rep/f", 0, 65536)?, changed);
        assert_eq!(pool.inspect_file(twin, "/rep/f", 0, 65536)?, changed);
        assert_eq!(pool.times(c, "/rep/f")?, Times::UNKNOWN);
        assert_eq!(pool.parent(c)?, ViewParent::HistoricalUnknown);
        let snap = pool.snapshot(c)?;
        let clone = pool.clone_snapshot(snap, [10; 16])?;
        assert!(matches!(
            src.verify_migration_pair(&mut pool),
            Err(Error::Invalid(_))
        ));
        assert_eq!(pool.parent(snap)?, ViewParent::Exact(c));
        assert_eq!(pool.parent(clone)?, ViewParent::Exact(snap));
        pool.remove(snap)?;
        pool.advance()?;
        pool.write_file(clone, "/rep/f", 0, b"x")?;
        assert!(pool.times(clone, "/rep/f")?.mtime.parts().is_some());
        assert_eq!(pool.times(s, "/rep/f")?, Times::UNKNOWN);
        let mut reopen = SecurePool::open(pool.into_device(), id, &next)?;
        reopen.unlock(new, Credential::Recovery(&other))?;
        assert_eq!(reopen.parent(clone)?, ViewParent::Exact(snap));
        assert!(matches!(reopen.parent(snap), Err(Error::NotFound)));
        let mut tampered = r.as_bytes().to_vec();
        tampered[160] ^= 1;
        assert!(
            src.verify_migration_source(&MigrationReceipt::from_bytes(&tampered)?)
                .is_err()
        );
    }
    Ok(())
}

#[test]
fn historiques_publics_et_temoin() -> Result<()> {
    use novafs::secure_pool::migration::image_digest;
    use novafs::shared::SharedPool;
    let key = Secret::generate(&mut HostEntropy)?;
    let witness = Secret::generate(&mut HostEntropy)?;
    for sector in [512, 4096] {
        for packed in [false, true] {
            let d = SimDevice::new(1000, sector)?;
            let ctl = d.control();
            let mut old = if packed {
                SharedPool::format_p6(d, [4; 16])?
            } else {
                SharedPool::format(d, [4; 16])?
            };
            let v = old.create_with_compression(
                [1; 16],
                Ids {
                    lv: [7; 16],
                    fs: [8; 16],
                },
                256,
                if packed {
                    Compression::Lz4
                } else {
                    Compression::None
                },
            )?;
            old.create_file(v, "/f")?;
            old.write_file(v, "/f", 0, &[91; 12000])?;
            let s = old.snapshot(v)?;
            let c = old.clone_snapshot(s, [8; 16])?;
            old.write_file(c, "/f", 0, b"clone")?;
            let sha = image_digest(&ctl.snapshot().device())?;
            assert!(
                old.migrate_p9(
                    SimDevice::new(2300, sector)?,
                    [0; 32],
                    &witness,
                    &key,
                    HostEntropy
                )
                .is_err()
            );
            let (disk, r) = old.migrate_p9(
                SimDevice::new(2300, sector)?,
                sha,
                &witness,
                &key,
                HostEntropy,
            )?;
            r.verify_clear_source(&ctl.snapshot().device(), sha, &witness)?;
            let mut new = SecurePool::open(disk, r.destination(), &key)?;
            new.check()?;
            assert_eq!(new.inspect_file(s, "/f", 0, 12000)?, [91; 12000]);
            assert_eq!(new.inspect_file(c, "/f", 0, 5)?, b"clone");
            assert_eq!(new.times(c, "/f")?, Times::UNKNOWN);
            assert_eq!(new.parent(s)?, ViewParent::HistoricalUnknown);
        }
    }
    Ok(())
}

#[test]
fn catalogue_ancien_volumes_prets_et_refus_du_brut() -> Result<()> {
    use novafs::{
        Engine,
        pool::{Pool, Type},
        secure_pool::migration::{image_digest, migrate_ready_pool},
    };
    let a = Secret::generate(&mut HostEntropy)?;
    let witness = Secret::generate(&mut HostEntropy)?;
    for sector in [512, 4096] {
        let mut pool = Pool::format(SimDevice::new(1024, sector)?)?;
        let ids = Ids {
            fs: [3; 16],
            lv: [4; 16],
        };
        pool.create_for([9; 16], ids.lv, Some(ids.fs), 1)?;
        let mut fs = Engine::format(pool.view(ids.lv, Type::Preparing, ids.fs)?, ids)?;
        fs.create("/f")?;
        fs.write("/f", 4095, b"abc")?;
        fs.unmount()?;
        pool.mark_ready(ids.lv, ids.fs)?;
        let source = pool.into_device();
        let expected = image_digest(&source)?;
        let (d, r) = migrate_ready_pool(
            &source,
            SimDevice::new(1024, sector)?,
            SimDevice::new(2400, sector)?,
            expected,
            &witness,
            &a,
            HostEntropy,
        )?;
        r.verify_clear_source(&source, expected, &witness)?;
        let mut target = SecurePool::open(d, r.destination(), &a)?;
        let v = target.list()?[0];
        assert_eq!(v.ids, ids);
        assert_eq!(target.inspect_file(v.id, "/f", 4095, 3)?, b"abc");
        let mut raw = Pool::format(SimDevice::new(1024, sector)?)?;
        raw.create_for([9; 16], [7; 16], None, 1)?;
        let raw = raw.into_device();
        let digest = image_digest(&raw)?;
        assert!(
            migrate_ready_pool(
                &raw,
                SimDevice::new(1024, sector)?,
                SimDevice::new(2400, sector)?,
                digest,
                &witness,
                &a,
                HostEntropy
            )
            .is_err()
        );
        assert_eq!(image_digest(&raw)?, digest);
    }
    Ok(())
}

#[test]
fn pool_vide_erreur_de_lecture_et_attestation_source_incomplete() -> Result<()> {
    use novafs::device::SimFault;
    for sector in [512, 4096] {
        let key = Secret::generate(&mut HostEntropy)?;
        let admin = Secret::generate(&mut HostEntropy)?;
        let dev = SimDevice::new(1800, sector)?;
        let base = SecurePool::format(dev, SRC_EMPTY, &key)?
            .into_device()
            .control()
            .snapshot();
        let material = base.bytes()?;
        let mut src = SecurePool::open(base.device(), SRC_EMPTY, &key)?;
        let (target, receipt) =
            src.migrate_p9(SimDevice::new(1800, sector)?, &admin, &[], HostEntropy)?;
        assert_eq!(receipt.units(), 0);
        assert_eq!(receipt.views().count(), 0);
        let mut dest = SecurePool::open(target, receipt.destination(), &admin)?;
        src.verify_migration_pair(&mut dest)?;
        for damage in [false, true] {
            let disk = base.device();
            let ctl = disk.control();
            let mut old = SecurePool::open(disk, SRC_EMPTY, &key)?;
            if damage {
                ctl.replace_stable(4, [0; 4096])?;
            } else {
                ctl.inject(SimFault::Read { block: 0 })?;
            }
            let target = SimDevice::new(1800, sector)?;
            let writes = target.control();
            writes.trace(None, None);
            assert!(old.migrate_p9(target, &admin, &[], HostEntropy).is_err());
            assert!(writes.events().is_empty());
        }
        assert_eq!(base.bytes()?, material);
    }
    Ok(())
}
const SRC_EMPTY: [u8; 16] = [29; 16];
