// SPDX-License-Identifier: MPL-2.0
use novafs::crypto::{Credential, HostEntropy, Secret};
use novafs::device::SimDevice;
use novafs::secure_export;
use novafs::secure_pool::{SecurePool, ViewParent};
use novafs::time::{Atime, Times, Timestamp};
use novafs::{Compression, Device, Ids, Result};
#[test]
fn restaurer_deux_archives_sans_source_dans_un_meme_pool() -> Result<()> {
    for sector in [512, 4096] {
        for historical in [false, true] {
            let a = Secret::generate(&mut HostEntropy)?;
            let k = Secret::generate(&mut HostEntropy)?;
            let backup = Secret::generate(&mut HostEntropy)?;
            let secret = Secret::generate(&mut HostEntropy)?;
            let ids = Ids {
                fs: [7; 16],
                lv: [8; 16],
            };
            let device = SimDevice::new(3000, sector)?;
            let mut p = if historical {
                SecurePool::format(device, [3; 16], &a)?
            } else {
                SecurePool::format_p9(device, [3; 16], &a, HostEntropy)?
            };
            let root = p.create([2; 16], ids, 256, Compression::Lz4, Some((b"ancien", &k)))?;
            p.create_file(root, "/f")?;
            p.write_file(root, "/f", 0, &[123; 32768])?;
            if !historical {
                p.set_atime_policy(root, Atime::No)?;
                p.set_times(root, "/f", None, Some(Timestamp::utc(-1, 123)?))?;
            }
            let s = p.snapshot(root)?;
            let c = p.clone_snapshot(s, [6; 16])?;
            let old = p.domain(s)?;
            let captured = p.times(s, "/f")?;
            p.write_file(c, "/f", 169, b"P9")?;
            let mut snapshot = Vec::new();
            let mut clone = Vec::new();
            p.export(s, sector, &backup, &mut snapshot)?;
            p.export(c, sector, &backup, &mut clone)?;
            drop(p);
            secure_export::check_export(snapshot.as_slice(), &backup)?;
            let mut target =
                SecurePool::format_p9(SimDevice::new(3000, sector)?, [4; 16], &a, HostEntropy)?;
            let t = target.restore(
                snapshot.as_slice(),
                &backup,
                [3; 16],
                b"nouvelle",
                &secret,
                Compression::Lz4,
            )?;
            let domain = target.domain(t)?;
            assert_ne!(old, domain);
            assert_eq!(target.times(t, "/f")?, captured);
            let t2 = target.restore(
                clone.as_slice(),
                &backup,
                [8; 16],
                b"nouvelle",
                &secret,
                Compression::Lz4,
            )?;
            assert_ne!(t, t2);
            assert_eq!(target.domain(t2)?, domain);
            assert_eq!(target.inspect_file(t2, "/f", 169, 2)?, b"P9");
            assert_eq!(target.inspect_file(t, "/f", 169, 2)?, [123, 123]);
            let target = target.into_device();
            assert!(target.blocks() > 0);
            let mut restored = SecurePool::open(target, [4; 16], &a)?;
            restored.unlock(domain, Credential::Recovery(&secret))?;
            restored.check()?;
            if historical {
                assert_eq!(restored.parent(t2)?, ViewParent::HistoricalUnknown);
                assert_eq!(captured, Times::UNKNOWN);
            } else {
                assert_eq!(restored.parent(t2)?, ViewParent::External([3; 16], s));
            }
        }
    }
    Ok(())
}
