// SPDX-License-Identifier: MPL-2.0
use novafs::device::SimDevice;
use novafs::time::{Atime, Clock, Times, Timestamp};
use novafs::{Engine, Ids, Mode, Result};
use std::sync::{Arc, Mutex};
struct TestClock(Mutex<Timestamp>);
impl Clock for TestClock {
    fn now(&self) -> Timestamp {
        *self.0.lock().unwrap()
    }
}
impl TestClock {
    fn tick(&self, t: Timestamp) {
        *self.0.lock().unwrap() = t;
    }
}

#[test]
fn horloge_profils_et_cycles() -> Result<()> {
    for sector in [512, 4096] {
        let ids = Ids {
            fs: [1; 16],
            lv: [2; 16],
        };
        let stamp = |s| Timestamp::utc(s, 550);
        let clock = Arc::new(TestClock(Mutex::new(stamp(-10)?)));
        let mut fs = Engine::format_p9(
            SimDevice::new(1024, sector)?,
            ids,
            Atime::Relative,
            Some(clock.clone()),
        )?;
        fs.mkdir("/a")?;
        fs.create("/a/f")?;
        let fichier = fs.lookup_at(fs.stat("/a")?.id, "f")?.id;
        assert_eq!(fs.times("/a/f")?, Times::created(stamp(-10)?));
        clock.tick(stamp(40)?);
        fs.write("/a/f", 0, b"abc")?;
        assert_eq!(fs.times("/a/f")?.mtime, stamp(40)?);
        let original = fs.times("/a/f")?;
        clock.tick(stamp(44)?);
        fs.write("/a/f", 1, b"b")?;
        assert_eq!(fs.times("/a/f")?, original);
        clock.tick(stamp(35)?);
        fs.rename("/a/f", "/a/g")?;
        let t = fs.times_id(fichier)?;
        assert_eq!(t, fs.times("/a/g")?);
        assert_eq!(
            (t.btime, t.ctime, t.mtime),
            (stamp(-10)?, stamp(35)?, stamp(40)?)
        );
        clock.tick(stamp(100)?);
        assert_eq!(fs.read_access("/a/g", 0, 3)?, b"abc");
        assert_eq!(fs.times("/a/g")?.atime, stamp(100)?);
        clock.tick(stamp(101)?);
        fs.read_access("/a/g", 0, 3)?;
        assert_eq!(fs.times("/a/g")?.atime, stamp(100)?);
        clock.tick(stamp(86500)?);
        fs.read_access("/a/g", 0, 3)?;
        assert_eq!(fs.times("/a/g")?.atime, stamp(86500)?);
        fs.set_atime_policy(Atime::No)?;
        clock.tick(stamp(86600)?);
        fs.read_access("/a/g", 0, 3)?;
        assert_eq!(fs.times("/a/g")?.atime, stamp(86500)?);
        fs.set_atime_policy(Atime::Strict)?;
        fs.read_access("/a/g", 0, 3)?;
        fs.list_access("/a", None, 8)?;
        assert_eq!(fs.times("/a/g")?.atime, stamp(86600)?);
        assert_eq!(fs.times("/a")?.atime, stamp(86600)?);
        clock.tick(Timestamp::UNKNOWN);
        fs.write("/a/g", 1, b"x")?;
        assert_eq!(fs.times("/a/g")?.mtime, Timestamp::UNKNOWN);
        fs.set_times_id(fichier, None, Some(stamp(i64::MIN)?))?;
        fs.rename("/a/g", "/h")?;
        let t = fs.times("/h")?;
        assert_eq!(Times::decode(&t.encode())?, t);
        assert_eq!(t.mtime, stamp(i64::MIN)?);
        let mut fs = Engine::mount_current(fs.into_device(), ids, Mode::ReadOnly)?;
        assert_eq!(fs.read_access("/h", 0, 3)?, b"axc");
        assert_eq!(fs.times("/h")?, t);
        assert_eq!(fs.read_access_id(fichier, 0, 3)?, b"axc");
        assert_eq!(
            fs.list_access_id(fs.stat("/a")?.id, None, 8)?,
            Vec::<String>::new()
        );
        let mut b = Times::created(Timestamp::utc(i64::MAX, 999_999_999)?).encode();
        assert!(Times::decode(&b).is_ok());
        b[8..12].copy_from_slice(&1_000_000_000u32.to_le_bytes());
        assert!(Times::decode(&b).is_err());
        assert!(Times::decode(&b[..47]).is_err());
    }
    Ok(())
}

#[test]
fn migration_autonome_anciens_objets_et_source_immuable() -> Result<()> {
    use novafs::Device;
    let ids = Ids {
        fs: [21; 16],
        lv: [22; 16],
    };
    for sector in [512, 4096] {
        for populated in [false, true] {
            let disk = SimDevice::new(512, sector)?;
            let ctl = disk.control();
            let mut fs = Engine::format(disk, ids)?;
            let data = (0..65536).map(|n| (n % 251) as u8).collect::<Vec<_>>();
            if populated {
                fs.mkdir("/a")?;
                fs.mkdir("/a/b")?;
                fs.create("/a/b/f")?;
                fs.write("/a/b/f", 0, &data)?;
            }
            let before = ctl.snapshot().bytes()?;
            let output = fs.migrate_timestamps(SimDevice::new(512, sector)?)?;
            assert_eq!(before, ctl.snapshot().bytes()?);
            assert_ne!(output.read(0)?, ctl.snapshot().device().read(0)?);
            let snapshot = output.control().snapshot();
            let reader = snapshot.device();
            let read_control = reader.control();
            read_control.trace(None, None);
            assert!(Engine::mount(reader, ids, Mode::ReadWrite).is_err());
            assert!(read_control.events().is_empty());
            let mut p9 = Engine::mount_current(output, ids, Mode::ReadWrite)?;
            assert_eq!(p9.atime_policy(), Some(Atime::Relative));
            assert_eq!(p9.times("/")?, Times::UNKNOWN);
            if populated {
                assert_eq!(p9.inspect_id(p9.stat("/a/b/f")?.id, 0, 65536)?, data);
                assert_eq!(p9.times("/a/b/f")?, Times::UNKNOWN);
                p9.rename("/a/b/f", "/a/g")?;
                assert!(p9.times("/a/g")?.ctime.parts().is_some());
                assert_eq!(p9.times("/a/g")?.mtime, Timestamp::UNKNOWN);
                let id = p9.stat("/a/g")?.id;
                p9.unlink("/a/g")?;
                assert!(matches!(p9.times_id(id), Err(novafs::Error::NotFound)));
                assert!(matches!(
                    p9.set_times_id(id, None, None),
                    Err(novafs::Error::NotFound)
                ));
            }
        }
    }
    Ok(())
}

#[test]
fn migration_autonome_interrompue_relecture_de_chaque_phase() -> Result<()> {
    use novafs::device::{Event, Persistence, SimFault};
    use novafs::secure_pool::migration::incomplete;
    let ids = Ids {
        fs: [23; 16],
        lv: [24; 16],
    };
    for sector in [512, 4096] {
        let d = SimDevice::new(512, sector)?;
        let control = d.control();
        let mut old = Engine::format(d, ids)?;
        old.create("/f")?;
        old.write("/f", 0, b"hier")?;
        let before = control.snapshot().bytes()?;
        let d = SimDevice::new(512, sector)?;
        let recorder = d.control();
        recorder.trace(None, None);
        old.migrate_timestamps(d)?;
        let trace = recorder.events();
        for i in 0..trace.len() {
            let d = SimDevice::new(512, sector)?;
            let ctl = d.control();
            ctl.trace(Some(i), None);
            assert!(old.migrate_timestamps(d).is_err());
            ctl.power_cut(Persistence::Newest(1))?;
            let image = ctl.snapshot().device();
            if let Ok(fs) = Engine::mount_current(image, ids, Mode::ReadOnly) {
                assert_eq!(fs.read("/f", 0, 4)?, b"hier");
                assert_eq!(fs.times("/f")?, Times::UNKNOWN);
                assert_eq!(fs.atime_policy(), Some(Atime::Relative));
                fs.verify()?;
            } else if i
                >= trace
                    .iter()
                    .position(|e| *e == Event::FlushCompleted(1))
                    .unwrap()
            {
                assert!(incomplete(&ctl.snapshot().device())?);
            }
        }
        let d = SimDevice::new(512, sector)?;
        let c = d.control();
        c.inject(SimFault::Flush {
            ordinal: 3,
            persist_newest: 1,
        })?;
        assert!(matches!(
            old.migrate_timestamps(d),
            Err(novafs::Error::Publication { .. })
        ));
        assert_eq!(before, control.snapshot().bytes()?);
        eprintln!(
            "P9 contenu autonome secteur={sector} coupures={}",
            trace.len()
        );
    }
    Ok(())
}
