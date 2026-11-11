// SPDX-License-Identifier: MPL-2.0
use novafs::crypto::{Credential, Entropy, HostEntropy, Secret};
use novafs::device::{Event, Persistence, SimDevice, SimFault, SimSnapshot};
use novafs::secure_pool::{
    SecurePool, inspect_header,
    migration::{Enrollment, incomplete},
    scrub::{Authority, Mode, Status, scrub},
};
use novafs::{Compression, Device, Error, Ids, Result};
use std::collections::{BTreeMap, BTreeSet};
static KDF: std::sync::Mutex<()> = std::sync::Mutex::new(());
const SRC: [u8; 16] = [41; 16];

fn source(
    sector: u32,
    admin: &Secret,
    recovery: &Secret,
) -> Result<(SimSnapshot, [u8; 16], u64, u64)> {
    let mut p = SecurePool::format(SimDevice::new(1800, sector)?, SRC, admin)?;
    let v = p.create(
        [1; 16],
        Ids {
            fs: [6; 16],
            lv: [7; 16],
        },
        256,
        Compression::Lz4,
        Some((b"source", recovery)),
    )?;
    let domain = p.domain(v)?;
    p.create_file(v, "/f")?;
    p.write_file(v, "/f", 0, b"source persistante")?;
    let snap = p.snapshot(v)?;
    let clone = p.clone_snapshot(snap, [3; 16])?;
    p.write_file(clone, "/f", 0, b"copie")?;
    Ok((p.into_device().control().snapshot(), domain, snap, clone))
}
fn nonces(dev: &impl Device, domain: [u8; 16]) -> Result<(usize, BTreeSet<(bool, [u8; 24])>)> {
    let mut seen = BTreeSet::new();
    let mut count = 0;
    for b in (46..dev.blocks()).step_by(2) {
        let h = dev.read(b)?;
        if &h[..8] == b"NP7UNIT1" && h[10] == 1 && h[32..48] == domain {
            count += 1;
            assert!(seen.insert((h[11] == 2, h[88..112].try_into().unwrap())));
        }
    }
    Ok((count, seen))
}

#[test]
fn coupures_migration_rechiffrement_recu_et_scrub() -> Result<()> {
    let _kdf = KDF.lock().unwrap();
    for sector in [512, 4096] {
        let admin = Secret::generate(&mut HostEntropy)?;
        let new_admin = Secret::generate(&mut HostEntropy)?;
        let recovery = Secret::generate(&mut HostEntropy)?;
        let new_recovery = Secret::generate(&mut HostEntropy)?;
        let (image, domain, snap, clone) = source(sector, &admin, &recovery)?;
        let before = image.bytes()?;
        let legacy_nonces = nonces(&image.device(), domain)?.1;
        let mut src = SecurePool::open(image.device(), SRC, &admin)?;
        src.unlock(domain, Credential::Recovery(&recovery))?;
        let enroll = [Enrollment {
            old_domain: domain,
            phrase: b"nouveau",
            recovery: &new_recovery,
        }];
        let d = SimDevice::new(1800, sector)?;
        let ctl = d.control();
        ctl.trace(None, None);
        let (mut good, r) = src.migrate_p9(d, &new_admin, &enroll, HostEntropy)?;
        let (_, _, new) = r.lineages().next().unwrap();
        let (units, seen) = nonces(&good, new)?;
        assert!(units > 0 && seen.is_disjoint(&legacy_nonces));
        let (_, data, meta) = r.seals().next().unwrap();
        assert_eq!(data + meta, units as u64);
        assert_eq!(r.units(), units as u64);
        let auth = Authority {
            pool: r.destination(),
            admin: &new_admin,
            domains: &[(new, Credential::Recovery(&new_recovery))],
        };
        let report = scrub(&mut good, Mode::Full, Some(&auth), |_| true)?;
        assert_eq!(report.status, Status::Healthy, "{:?}", report.issues);
        let certificate = 2 + 2 * (r.destination_generation() % 4);
        for missing in [1, 2] {
            let mut copy = ctl.snapshot().device();
            let damaged = copy.control();
            for index in 0..missing {
                damaged.replace_stable(certificate + index, [0; 4096])?;
            }
            let before = damaged.snapshot().bytes()?;
            let repair = scrub(&mut copy, Mode::Repair, Some(&auth), |_| true)?;
            if missing == 1 {
                assert_eq!(repair.status, Status::Repaired);
                assert_eq!(repair.writes, 1);
                assert_eq!(
                    scrub(&mut copy, Mode::Full, Some(&auth), |_| true)?.status,
                    Status::Healthy
                );
            } else {
                assert_eq!(repair.writes, 0);
                assert_ne!(repair.status, Status::Healthy);
                assert_eq!(damaged.snapshot().bytes()?, before);
                assert!(SecurePool::open(copy, r.destination(), &new_admin).is_err());
            }
        }
        let trace = ctl.events();
        let mut relevant = BTreeSet::new();
        for (i, e) in trace.iter().enumerate() {
            match e {
                Event::FlushSubmitted(_)
                | Event::FlushEffect(_)
                | Event::FlushCompleted(_)
                | Event::WriteSubmitted(0 | 1) => {
                    relevant.insert(i);
                }
                _ => {}
            }
        }
        relevant.insert(
            trace
                .iter()
                .position(|e| matches!(e,Event::WriteSubmitted(b) if *b>=46))
                .unwrap(),
        );
        for i in &relevant {
            let d = SimDevice::new(1800, sector)?;
            let ctl = d.control();
            ctl.trace(Some(*i), None);
            assert!(
                src.migrate_p9(d, &new_admin, &enroll, HostEntropy).is_err(),
                "{i}"
            );
            ctl.power_cut(Persistence::Newest(1))?;
            let recovered = ctl.snapshot().device();
            if let Ok(id) = inspect_header(&recovered) {
                let mut pool = SecurePool::open(recovered, id, &new_admin)?;
                let domain = pool.domain(clone)?;
                pool.unlock(domain, Credential::Recovery(&new_recovery))?;
                pool.check()?;
                assert_eq!(pool.inspect_file(snap, "/f", 0, 19)?, b"source persistante");
                assert_eq!(
                    pool.inspect_file(clone, "/f", 0, 19)?,
                    b"copiee persistante"
                );
                src.verify_migration_source(&pool.migration_receipt()?.unwrap())?;
            } else {
                assert!(
                    incomplete(&recovered)?
                        || recovered.read(0)? == [0; 4096]
                        || recovered.read(1)? == [0; 4096]
                );
            }
        }
        let flushes = trace
            .iter()
            .filter(|e| matches!(e, Event::FlushSubmitted(_)))
            .count();
        for ordinal in 1..=flushes as u32 {
            let d = SimDevice::new(1800, sector)?;
            let ctl = d.control();
            ctl.inject(SimFault::Flush {
                ordinal,
                persist_newest: 1,
            })?;
            let e = src
                .migrate_p9(d, &new_admin, &enroll, HostEntropy)
                .err()
                .expect("FLUSH perdu");
            if ordinal == flushes as u32 {
                assert!(matches!(e, Error::Publication { .. }));
            }
        }
        let d = SimDevice::new(1800, sector)?;
        d.control().inject(SimFault::Write {
            block: 46,
            prefix: 511,
        })?;
        assert!(src.migrate_p9(d, &new_admin, &enroll, HostEntropy).is_err());
        assert_eq!(before, image.bytes()?);
        eprintln!(
            "P9 coupures secteur={sector} frontières={} flushs={flushes} unités={units}",
            relevant.len()
        );
    }
    Ok(())
}

struct Duplicate {
    count: usize,
}
impl Entropy for Duplicate {
    fn fill(&mut self, out: &mut [u8]) -> Result<()> {
        HostEntropy.fill(out)?;
        if out.len() == 24 {
            self.count += 1;
            if self.count >= 4 {
                out.fill(0x5a);
            }
        }
        Ok(())
    }
}

#[test]
fn source_alteree_nonce_rejete_et_destination_incomplete() -> Result<()> {
    let _kdf = KDF.lock().unwrap();
    for sector in [512, 4096] {
        let a = Secret::generate(&mut HostEntropy)?;
        let k = Secret::generate(&mut HostEntropy)?;
        let (base, id, _, _) = source(sector, &a, &k)?;
        let mut src = SecurePool::open(base.device(), SRC, &a)?;
        src.unlock(id, Credential::Recovery(&k))?;
        let enroll = [Enrollment {
            old_domain: id,
            phrase: b"p9",
            recovery: &k,
        }];
        let dest = SimDevice::new(1800, sector)?;
        let ctl = dest.control();
        assert!(matches!(
            src.migrate_p9(dest, &a, &enroll, Duplicate { count: 0 }),
            Err(Error::Unavailable)
        ));
        assert!(inspect_header(&ctl.snapshot().device()).is_err());
        assert!(incomplete(&ctl.snapshot().device())?);
        let d = base.device();
        let ctl = d.control();
        let mut roles = BTreeMap::new();
        for i in (46..d.blocks()).step_by(2) {
            let h = d.read(i)?;
            if &h[..8] == b"NP7UNIT1" {
                roles.insert(i, h);
            }
        }
        // Altérer toutes les unités chiffrées de données, dont la racine active.
        for (i, h) in roles {
            if h[11] == 1 {
                let mut p = d.read(i + 1)?;
                p[0] ^= 1;
                ctl.replace_stable(i + 1, p)?;
            }
        }
        let mut damaged = SecurePool::open(d, SRC, &a)?;
        damaged.unlock(id, Credential::Recovery(&k))?;
        let target = SimDevice::new(1800, sector)?;
        let ctl = target.control();
        ctl.trace(None, None);
        assert!(
            damaged
                .migrate_p9(target, &a, &enroll, HostEntropy)
                .is_err()
        );
        assert!(ctl.events().is_empty());
    }
    Ok(())
}

#[derive(Clone, Copy, Debug)]
enum Mutation {
    Rename,
    Date,
    Snapshot,
    Clone,
}

struct FixedClock(novafs::time::Timestamp);
impl novafs::time::Clock for FixedClock {
    fn now(&self) -> novafs::time::Timestamp {
        self.0
    }
}

fn mutate(
    pool: &mut SecurePool<SimDevice, HostEntropy>,
    mutation: Mutation,
    origin: u64,
    snapshot: u64,
) -> Result<()> {
    match mutation {
        Mutation::Rename => pool.rename(origin, "/f", "/renomme"),
        Mutation::Date => pool.set_times(
            origin,
            "/f",
            None,
            Some(novafs::time::Timestamp::utc(-150, 74)?),
        ),
        Mutation::Snapshot => pool.snapshot(origin).map(drop),
        Mutation::Clone => pool.clone_snapshot(snapshot, [94; 16]).map(drop),
    }
}

#[test]
fn transaction_p9_interrompue_dates_renommage_snapshot_et_clone() -> Result<()> {
    use novafs::secure_pool::ViewParent;
    use novafs::time::Timestamp;
    for sector in [512, 4096] {
        let a = Secret::generate(&mut HostEntropy)?;
        let pool_id = [70; 16];
        let mut initial =
            SecurePool::format_p9(SimDevice::new(1800, sector)?, pool_id, &a, HostEntropy)?;
        let historic = Timestamp::utc(100, 210)?;
        initial.set_clock(std::sync::Arc::new(FixedClock(historic)));
        let origin = initial.create(
            [1; 16],
            Ids {
                fs: [8; 16],
                lv: [9; 16],
            },
            256,
            Compression::None,
            None,
        )?;
        initial.create_file(origin, "/f")?;
        initial.write_file(origin, "/f", 0, b"octets confirmes")?;
        let snapshot = initial.snapshot(origin)?;
        let generation = initial.generation();
        let base = initial.into_device().control().snapshot();
        let new_time = Timestamp::utc(200, 19)?;
        let mut cases = 0;
        for op in [
            Mutation::Rename,
            Mutation::Date,
            Mutation::Snapshot,
            Mutation::Clone,
        ] {
            let fresh = || -> Result<_> {
                let image = base.device();
                let control = image.control();
                let mut pool = SecurePool::open(image, pool_id, &a)?;
                pool.set_clock(std::sync::Arc::new(FixedClock(new_time)));
                Ok((pool, control))
            };
            let (mut reference, c) = fresh()?;
            c.trace(None, None);
            mutate(&mut reference, op, origin, snapshot)?;
            let trace = c.events();
            // Le premier écrivain après remontage draine le cache avant F1 et F2.
            assert_eq!(
                trace
                    .iter()
                    .filter(|e| matches!(e, Event::FlushCompleted(_)))
                    .count(),
                3
            );
            let first_publication = trace
                .iter()
                .position(|e| *e == Event::BeforeWrite(2 + 2 * ((generation + 1) % 4)))
                .unwrap();
            let durable = trace
                .iter()
                .position(|e| *e == Event::FlushEffect(3))
                .unwrap();
            for i in 0..trace.len() {
                for outcome in [Persistence::None, Persistence::Newest(1)] {
                    let (mut p, c) = fresh()?;
                    c.trace(Some(i), None);
                    assert!(mutate(&mut p, op, origin, snapshot).is_err());
                    c.power_cut(outcome)?;
                    let mut recovered = SecurePool::open(c.snapshot().device(), pool_id, &a)?;
                    recovered.check()?;
                    let committed = match recovered.generation() {
                        g if g == generation => false,
                        g if g == generation + 1 => true,
                        g => panic!("génération {g}"),
                    };
                    if i <= first_publication {
                        assert!(!committed);
                    }
                    if i >= durable {
                        assert!(committed);
                    }
                    assert_eq!(
                        recovered.inspect_file(snapshot, "/f", 0, 32)?,
                        b"octets confirmes"
                    );
                    assert_eq!(recovered.times(snapshot, "/f")?.mtime, historic);
                    let path = if committed && matches!(op, Mutation::Rename) {
                        "/renomme"
                    } else {
                        "/f"
                    };
                    assert_eq!(
                        recovered.inspect_file(origin, path, 0, 32)?,
                        b"octets confirmes"
                    );
                    let t = recovered.times(origin, path)?;
                    assert_eq!(t.btime, historic);
                    assert_eq!(
                        t.mtime,
                        if committed && matches!(op, Mutation::Date) {
                            Timestamp::utc(-150, 74)?
                        } else {
                            historic
                        }
                    );
                    assert_eq!(
                        t.ctime,
                        if committed && matches!(op, Mutation::Rename | Mutation::Date) {
                            new_time
                        } else {
                            historic
                        }
                    );
                    if matches!(op, Mutation::Rename) {
                        assert!(matches!(
                            recovered.times(origin, if committed { "/f" } else { "/renomme" }),
                            Err(Error::NotFound)
                        ));
                    }
                    let views = recovered.list()?;
                    assert_eq!(
                        views.len(),
                        2 + usize::from(
                            committed && matches!(op, Mutation::Snapshot | Mutation::Clone)
                        )
                    );
                    if views.len() == 3 {
                        assert_eq!(
                            recovered.parent(views[2].id)?,
                            ViewParent::Exact(if matches!(op, Mutation::Clone) {
                                snapshot
                            } else {
                                origin
                            })
                        );
                    }
                    cases += 1;
                }
            }
        }
        eprintln!("P9 transactions secteur={sector} coupures_et_ordres={cases}");
    }
    Ok(())
}

// Rejoue uniquement les octets publics des nonces. Les clés et les identifiants
// viennent toujours du CSPRNG hôte, y compris lors du second essai.
#[derive(Default)]
struct NoncesRejoues(u64);
impl Entropy for NoncesRejoues {
    fn fill(&mut self, out: &mut [u8]) -> Result<()> {
        if out.len() == 24 {
            self.0 += 1;
            out.fill(0);
            out[..8].copy_from_slice(&self.0.to_le_bytes());
            Ok(())
        } else {
            HostEntropy.fill(out)
        }
    }
}
#[test]
fn nouvelle_cle_apres_un_scellement_interrompu_et_integrite_du_recu() -> Result<()> {
    use novafs::secure_unit::context;
    let _kdf = KDF.lock().unwrap();
    for sector in [512, 4096] {
        let a = Secret::generate(&mut HostEntropy)?;
        let key = Secret::generate(&mut HostEntropy)?;
        let next = Secret::generate(&mut HostEntropy)?;
        let (src, old, _, _) = source(sector, &a, &key)?;
        let mut source = SecurePool::open(src.device(), SRC, &a)?;
        let enroll = [Enrollment {
            old_domain: old,
            phrase: b"nouvelle",
            recovery: &next,
        }];
        let blank = SimDevice::new(1800, sector)?;
        let untouched = blank.control();
        untouched.trace(None, None);
        assert!(matches!(
            source.migrate_p9(blank, &a, &enroll, HostEntropy),
            Err(Error::ReadOnly)
        ));
        assert!(untouched.events().is_empty());
        source.unlock(old, Credential::Recovery(&key))?;
        let small = SimDevice::new(1000, sector)?;
        let ctl = small.control();
        ctl.trace(None, None);
        assert!(matches!(
            source.migrate_p9(small, &a, &enroll, HostEntropy),
            Err(Error::NoSpace)
        ));
        assert!(ctl.events().is_empty());

        let dest = SimDevice::new(1800, sector)?;
        let good = dest.control();
        good.trace(None, None);
        let (_, receipt) = source.migrate_p9(dest, &a, &enroll, NoncesRejoues::default())?;
        let trace = good.events();
        let cut = trace
            .iter()
            .position(|e| matches!(e,Event::WriteCompleted(b) if *b>=46))
            .unwrap();
        let Event::WriteCompleted(addr) = trace[cut] else {
            unreachable!()
        };
        let d = SimDevice::new(1800, sector)?;
        let fault = d.control();
        fault.trace(Some(cut), None);
        assert!(
            source
                .migrate_p9(d, &a, &enroll, NoncesRejoues::default())
                .is_err()
        );
        fault.power_cut(Persistence::Newest(2))?;
        let aborted = fault.snapshot().device();
        assert!(incomplete(&aborted)?);
        let abandoned = aborted.read(addr)?;
        let valid = good.snapshot().device().read(addr)?;
        assert_eq!(abandoned[88..112], valid[88..112]);
        assert_ne!(context(&abandoned)?.domain, context(&valid)?.domain);
        assert_ne!(abandoned[112..128], valid[112..128]);
        assert!(
            source
                .migrate_p9(aborted, &a, &enroll, HostEntropy)
                .is_err()
        );
        for remove in [false, true] {
            let media = good.snapshot().device();
            let ctl = media.control();
            for header in [0, 1] {
                let mut bytes = media.read(header)?;
                if remove {
                    bytes[12] = 0;
                    bytes[112..].fill(0);
                } else {
                    bytes[114 + 160] ^= 1;
                }
                ctl.replace_stable(header, bytes)?;
            }
            ctl.trace(None, None);
            assert!(SecurePool::open(media, receipt.destination(), &a).is_err());
            assert!(ctl.events().is_empty());
        }
    }
    Ok(())
}
