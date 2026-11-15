//! Le même backend P9 que l'ELF NovaFS. L'autorité du courtier est testée
//! dans l'invité, indépendamment de ces contrôles de la couche persistante.
extern crate alloc;
#[path = "../user/novafs/files/cache_store.rs"]
mod cache_store;
use cache_store::{Key, Policy, ROOT, Store};
use novafs::{
    Device, Engine, Error, Ids, Mode, Result, VolumePolicy,
    device::{Event, FileImage, Persistence, SimDevice, SimFault},
    time::Atime,
};
const IDS: Ids = Ids {
    fs: [23; 16],
    lv: [77; 16],
};
fn key(u: u8, i: u8) -> Key {
    let mut k = [i; 32];
    k[..16].fill(u);
    k
}
fn policy() -> Policy {
    Policy {
        cache: 500_000,
        user: 600_000,
        global: 900_000,
        reserve: 64,
    }
}
fn base<D: Device>(device: D) -> Result<Engine<D>> {
    let mut f = Engine::format_p9(device, IDS, Atime::No, None)?;
    f.create("/document")?;
    f.write("/document", 0, b"conserver")?;
    Ok(f)
}
fn document<D: Device>(f: &Engine<D>) -> Result<()> {
    assert_eq!(f.inspect_id(f.stat("/document")?.id, 0, 20)?, b"conserver");
    f.verify()
}
fn real(sector: u32) -> Result<()> {
    let mut f = base(FileImage::create(512, sector)?)?;
    assert!(cache_store::hidden(ROOT) && !cache_store::hidden("cache"));
    let derived = Policy::for_volume(&f)?.validate()?;
    assert_eq!(derived.reserve, 64);
    let (a, b, c) = (key(1, 1), key(1, 2), key(2, 1));
    let s = Store::mount(&mut f, policy())?;
    let (ga, gb, gc) = (
        s.open(&mut f, &a)?,
        s.open(&mut f, &b)?,
        s.open(&mut f, &c)?,
    );
    assert_eq!(s.open(&mut f, &a)?, ga);
    s.write(&mut f, &a, ga, "temoin", 0, b"42")?;
    s.write(&mut f, &b, gb, "temoin", 0, b"73")?;
    s.write(&mut f, &c, gc, "temoin", 0, b"91")?;
    let r = f.receipt();
    for n in [
        "/document",
        "../document",
        "a/b",
        ".",
        "..",
        "",
        "\x1ffaux",
        "a\\b",
    ] {
        assert!(s.write(&mut f, &a, ga, n, 0, b"x").is_err());
    }
    assert!(s.write(&mut f, &a, gb, "temoin", 0, b"x").is_err());
    assert!(s.purge(&mut f, &c, ga).is_err());
    assert!(s.open(&mut f, &[0; 32]).is_err());
    assert_eq!(f.receipt(), r);
    // P9 n'a aucun type lien. Ces octets ne sont jamais résolus comme chemin.
    s.write(&mut f, &a, ga, "pseudo-lien", 0, b"/document")?;
    let mut f = Engine::mount_current(f.unmount()?, IDS, Mode::ReadWrite)?;
    let s = Store::mount(&mut f, policy())?;
    let report = s.recover(&mut f)?;
    assert_eq!((report.purged, report.quarantined), (0, 0));
    assert_eq!(s.read(&f, &a, ga, "temoin", 0, 64)?, b"42");
    s.begin_purge(&mut f, &a, ga)?;
    let receipt = f.receipt();
    assert!(s.write(&mut f, &a, ga, "obsolete", 0, b"NON").is_err());
    assert!(s.open(&mut f, &a).is_err());
    assert_eq!(f.receipt(), receipt);
    let mut f = Engine::mount_current(f.unmount()?, IDS, Mode::ReadWrite)?;
    let s = Store::mount(&mut f, policy())?;
    let report = s.recover(&mut f)?;
    assert_eq!((report.purged, report.quarantined), (1, 0));
    let fresh = s.open(&mut f, &a)?;
    assert_ne!(ga, fresh);
    assert!(s.write(&mut f, &a, ga, "obsolete", 0, b"NON").is_err());
    assert!(matches!(
        s.read(&f, &a, fresh, "temoin", 0, 64),
        Err(Error::NotFound)
    ));
    assert_eq!(s.read(&f, &b, gb, "temoin", 0, 64)?, b"73");
    assert_eq!(s.read(&f, &c, gc, "temoin", 0, 64)?, b"91");
    assert_eq!(s.purge_scope(&mut f, Some([1; 16]))?, 2);
    assert_eq!(s.read(&f, &c, gc, "temoin", 0, 64)?, b"91");
    assert_eq!(s.purge_scope(&mut f, None)?, 1);
    assert!(!f.collection_pending()?);
    document(&f)?;
    if let Some(target) = std::env::var_os("NOVAOS_CACHE_PROOFS") {
        use std::{
            fs::{File, OpenOptions},
            io::{Seek, copy},
            path::PathBuf,
        };
        let path =
            PathBuf::from(target).join(format!("p9-{}-{sector}.raw", std::env::consts::ARCH));
        let mut src = f.unmount()?.into_file();
        src.rewind()?;
        let mut dst = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        assert_eq!(copy(&mut src, &mut dst)?, src.metadata()?.len());
        dst.sync_all()?;
        drop(dst);
        let f = Engine::mount_current(
            FileImage::from_file(File::open(path)?, 512, sector)?,
            IDS,
            Mode::ReadOnly,
        )?;
        document(&f)?;
    }
    println!(
        "secteur={sector} P9-fichier remontage purge-marquee reprise ciblage génération partages données-extérieures=OK"
    );
    Ok(())
}
fn quotas(sector: u32) -> Result<()> {
    let mut f = base(FileImage::create(512, sector)?)?;
    let (a, b, c) = (key(1, 1), key(1, 2), key(2, 1));
    let mut s = Store::mount(&mut f, policy())?;
    let (ga, gb, gc) = (
        s.open(&mut f, &a)?,
        s.open(&mut f, &b)?,
        s.open(&mut f, &c)?,
    );
    for (k, g) in [(a, ga), (b, gb), (c, gc)] {
        s.write(&mut f, &k, g, "x", 0, b"1")?;
    }
    let r = f.receipt();
    for p in [
        Policy {
            cache: 8193,
            user: 100_000,
            global: 100_000,
            ..policy()
        },
        Policy {
            cache: 16000,
            user: 16386,
            global: 100_000,
            ..policy()
        },
        Policy {
            cache: 16000,
            user: 23000,
            global: 24579,
            ..policy()
        },
    ] {
        s = Store::mount(&mut f, p)?;
        assert!(matches!(
            s.write(&mut f, &a, ga, "x", 1, b"2"),
            Err(Error::QuotaExceeded)
        ));
        assert_eq!(f.receipt(), r);
    }
    s = Store::mount(&mut f, policy())?;
    assert!(s.pressure(&f)?.is_none());
    // Charge physique réelle distincte du quota logique du cache. Des données
    // dans le document public franchissent le seuil sans réduire les quotas.
    f.create("/charge-systeme")?;
    let blocks = vec![0x75; 16 * 4096];
    let mut off = 0;
    while s.pressure(&f)?.is_none() {
        f.write("/charge-systeme", off, &blocks)?;
        off += blocks.len() as u64;
        assert!(off <= 512 * 4096);
    }
    let avail = f.accounting()?.available_blocks;
    assert!(avail < 128);
    assert_eq!(s.pressure(&f)?, Some((a, ga)));
    let before = f.receipt();
    assert!(!s.auto_purge(&mut f, &b, gb)?);
    assert!(matches!(s.open(&mut f, &key(1, 5)), Err(Error::NoSpace)));
    assert_eq!(f.receipt(), before);
    assert!(s.auto_purge(&mut f, &a, ga)?);
    assert_eq!(s.read(&f, &b, gb, "x", 0, 4)?, b"1");
    assert_eq!(s.read(&f, &c, gc, "x", 0, 4)?, b"1");
    let settings = f.accounting()?;
    f.set_volume_policy(VolumePolicy {
        maintenance_blocks: settings.available_blocks + settings.maintenance_reserved,
        logical_quota_bytes: None,
    })?;
    assert_eq!(f.accounting()?.available_blocks, 0);
    assert!(matches!(s.open(&mut f, &key(3, 4)), Err(Error::NoSpace)));
    document(&f)?;
    println!(
        "secteur={sector} quotas cache-utilisateur-global=OK pression_réelle disponible_blocs={avail} réserve=64 victime_exacte=OK saturation=OK"
    );
    Ok(())
}
fn corrupt(sector: u32) -> Result<()> {
    let mut f = base(FileImage::create(512, sector)?)?;
    let s = Store::mount(&mut f, policy())?;
    let a = key(1, 2);
    let g = s.open(&mut f, &a)?;
    s.write(&mut f, &a, g, "secret", 0, b"sans-attribution")?;
    let dir = f.lookup_at(f.root_id(), ROOT)?;
    let m = f.lookup_at(dir.id, &format!("r{g:016x}"))?;
    f.prepare_write_id(m.id, 56, &[9])?.commit()?;
    let before = f.receipt();
    let report = s.recover(&mut f)?;
    assert!(report.quarantined > 0);
    assert!(s.read(&f, &a, g, "secret", 0, 64).is_err());
    assert!(s.open(&mut f, &a).is_err());
    assert!(s.purge(&mut f, &a, g).is_err());
    assert_eq!(f.receipt(), before);
    assert_eq!(
        f.inspect_id(f.lookup_at(g, "secret")?.id, 0, 64)?,
        b"sans-attribution"
    );
    document(&f)?;
    let mut p8 = Engine::format(FileImage::create(256, sector)?, IDS)?;
    let r = p8.receipt();
    assert!(Store::mount(&mut p8, policy()).is_err());
    assert_eq!(r, p8.receipt());
    println!("secteur={sector} corruption-filiation=quarantaine immuable P8=refus-sans-écriture");
    Ok(())
}
fn corrupt_scope(sector: u32) -> Result<()> {
    let mut f = base(FileImage::create(512, sector)?)?;
    let (a, b, c) = (key(1, 1), key(2, 1), key(3, 1));
    let s = Store::mount(&mut f, policy())?;
    let (ga, gb, gc) = (
        s.open(&mut f, &a)?,
        s.open(&mut f, &b)?,
        s.open(&mut f, &c)?,
    );
    for (k, g) in [(a, ga), (b, gb), (c, gc)] {
        s.write(&mut f, &k, g, "x", 0, b"origine")?;
    }
    let root = f.lookup_at(f.root_id(), ROOT)?.id;
    let ma = f.lookup_at(root, &format!("r{ga:016x}"))?.id;
    let mb = f.lookup_at(root, &format!("r{gb:016x}"))?.id;
    f.prepare_write_id(mb, 56, &[0x82])?.commit()?;
    let receipt = f.receipt();
    for scope in [Some([1; 16]), None] {
        assert!(matches!(
            s.purge_scope(&mut f, scope),
            Err(Error::Invalid(cache_store::UNVERIFIED | cache_store::STALE))
        ));
        assert_eq!(f.receipt(), receipt);
    }
    for (k, g) in [(a, ga), (c, gc)] {
        assert_eq!(s.read(&f, &k, g, "x", 0, 16)?, b"origine");
    }
    // Deuxième image logique du banc : cette fois la décision précède le défaut.
    f.prepare_write_id(mb, 56, &[2])?.commit()?;
    s.begin_scope(&mut f, Some([1; 16]))?;
    f.prepare_write_id(ma, 56, &[0x81])?.commit()?;
    let mut f = Engine::mount_current(f.unmount()?, IDS, Mode::ReadWrite)?;
    let s = Store::mount(&mut f, policy())?;
    let receipt = f.receipt();
    let audit = s.recover(&mut f)?;
    assert!(audit.quarantined != 0);
    assert_eq!(audit.purged, 0);
    assert_eq!(f.receipt(), receipt);
    assert!(matches!(s.open(&mut f, &a), Err(Error::Busy)));
    assert_eq!(f.inspect_id(f.lookup_at(ga, "x")?.id, 0, 16)?, b"origine");
    for (k, g) in [(b, gb), (c, gc)] {
        assert_eq!(s.open(&mut f, &k)?, g);
        assert_eq!(s.read(&f, &k, g, "x", 0, 16)?, b"origine");
    }
    assert_eq!(f.receipt(), receipt);
    document(&f)?;
    println!(
        "secteur={sector} purge-portée avant-intention=refus-sans-écriture après-intention=quarantaine-durable caches-indépendants-lisibles=OK"
    );
    Ok(())
}
fn cuts(sector: u32) -> Result<()> {
    let dev = SimDevice::new(320, sector)?;
    let ctl = dev.control();
    let mut f = base(dev)?;
    let (a, b) = (key(1, 1), key(2, 1));
    let s = Store::mount(&mut f, policy())?;
    let ga = s.open(&mut f, &a)?;
    let gb = s.open(&mut f, &b)?;
    for n in ["x", "y"] {
        s.write(&mut f, &a, ga, n, 0, b"ancien")?
    }
    s.write(&mut f, &b, gb, "public-prive", 0, b"autre")?;
    drop(f);
    let baseline = ctl.snapshot();
    let run = baseline.device();
    let ctl = run.control();
    let mut f = Engine::mount_current(run, IDS, Mode::ReadWrite)?;
    let s = Store::mount(&mut f, policy())?;
    ctl.trace(None, None);
    s.purge(&mut f, &a, ga)?;
    let events = ctl.events();
    let indexes: Vec<_> = events
        .iter()
        .enumerate()
        .filter_map(|(i, e)| matches!(e, Event::FlushCompleted(_)).then_some(i))
        .collect();
    assert!(indexes.len() >= 8);
    for i in &indexes {
        let dev = baseline.device();
        let ctl = dev.control();
        let mut f = Engine::mount_current(dev, IDS, Mode::ReadWrite)?;
        let s = Store::mount(&mut f, policy())?;
        ctl.trace(Some(*i), None);
        let _ = s.purge(&mut f, &a, ga);
        drop(f);
        ctl.power_cut(Persistence::None)?;
        let mut f = Engine::mount_current(ctl.snapshot().device(), IDS, Mode::ReadWrite)?;
        let s = Store::mount(&mut f, policy())?;
        let r = s.recover(&mut f)?;
        assert_eq!(r.quarantined, 0);
        let now = s.open(&mut f, &a)?;
        if now == ga {
            for n in ["x", "y"] {
                assert_eq!(s.read(&f, &a, now, n, 0, 10)?, b"ancien");
            }
        } else {
            for n in ["x", "y"] {
                assert!(matches!(
                    s.read(&f, &a, now, n, 0, 10),
                    Err(Error::NotFound)
                ));
            }
        }
        assert_eq!(s.read(&f, &b, gb, "public-prive", 0, 10)?, b"autre");
        document(&f)?;
    }
    println!(
        "secteur={sector} coupures_après_FLUSH={} aucune_adoption_partielle=OK",
        indexes.len()
    );
    Ok(())
}
fn more_cuts(sector: u32, purge: bool) -> Result<()> {
    let dev = SimDevice::new(384, sector)?;
    let ctl = dev.control();
    let mut f = base(dev)?;
    let (a, b, c) = (key(1, 4), key(1, 5), key(2, 6));
    let s = Store::mount(&mut f, policy())?;
    let (ga, gb, gc) = (
        s.open(&mut f, &a)?,
        s.open(&mut f, &b)?,
        s.open(&mut f, &c)?,
    );
    for (k, g) in [(a, ga), (b, gb), (c, gc)] {
        s.write(&mut f, &k, g, "x", 0, b"original")?;
    }
    drop(f);
    let baseline = ctl.snapshot();
    let operation = |s: &Store, f: &mut Engine<SimDevice>| -> Result<()> {
        if purge {
            s.purge_scope(f, Some([1; 16])).map(|_| ())
        } else {
            s.write(f, &a, ga, "nouveau", 0, b"complet")
        }
    };
    let dev = baseline.device();
    let ctl = dev.control();
    let mut f = Engine::mount_current(dev, IDS, Mode::ReadWrite)?;
    let s = Store::mount(&mut f, policy())?;
    ctl.trace(None, None);
    operation(&s, &mut f)?;
    let indexes: Vec<_> = ctl
        .events()
        .iter()
        .enumerate()
        .filter_map(|(i, e)| matches!(e, Event::FlushCompleted(_)).then_some(i))
        .collect();
    assert!(indexes.len() >= 6);
    for i in &indexes {
        let dev = baseline.device();
        let ctl = dev.control();
        let mut f = Engine::mount_current(dev, IDS, Mode::ReadWrite)?;
        let s = Store::mount(&mut f, policy())?;
        ctl.trace(Some(*i), None);
        let _ = operation(&s, &mut f);
        drop(f);
        ctl.power_cut(Persistence::None)?;
        let mut f = Engine::mount_current(ctl.snapshot().device(), IDS, Mode::ReadWrite)?;
        let s = Store::mount(&mut f, policy())?;
        assert_eq!(s.recover(&mut f)?.quarantined, 0);
        if purge {
            let (a_new, b_new) = (s.open(&mut f, &a)?, s.open(&mut f, &b)?);
            assert_eq!(ga == a_new, gb == b_new);
            for (k, old, g) in [(a, ga, a_new), (b, gb, b_new)] {
                if old == g {
                    assert_eq!(s.read(&f, &k, g, "x", 0, 32)?, b"original");
                } else {
                    assert!(matches!(
                        s.read(&f, &k, g, "x", 0, 32),
                        Err(Error::NotFound)
                    ));
                }
            }
        } else {
            match s.read(&f, &a, ga, "nouveau", 0, 32) {
                Err(Error::NotFound) => (),
                Ok(bytes) => assert_eq!(bytes, b"complet"),
                Err(e) => return Err(e),
            }
            assert!(
                f.inspect_dir(ga, None, 10)?
                    .iter()
                    .all(|n| n.as_bytes().first() != Some(&31))
            );
        }
        assert_eq!(s.read(&f, &c, gc, "x", 0, 16)?, b"original");
        document(&f)?;
    }
    println!(
        "secteur={sector} opération={} frontières_FLUSH={} témoin_isolé=OK",
        if purge {
            "purge-utilisateur"
        } else {
            "premier-WRITE-nom-absent-ou-complet"
        },
        indexes.len()
    );
    Ok(())
}
fn flush_status(sector: u32) -> Result<()> {
    let dev = SimDevice::new(320, sector)?;
    let ctl = dev.control();
    let mut f = base(dev)?;
    let s = Store::mount(&mut f, policy())?;
    let k = key(7, 3);
    let g = s.open(&mut f, &k)?;
    s.write(&mut f, &k, g, "existant", 0, b"avant")?;
    drop(f);
    let baseline = ctl.snapshot();
    for fresh in [false, true] {
        let name = if fresh { "nouveau" } else { "existant" };
        let barriers = if fresh { 6 } else { 2 };
        for ordinal in 1..=barriers {
            for persistent in [0, 1] {
                let dev = baseline.device();
                let ctl = dev.control();
                let mut f = Engine::mount_current(dev, IDS, Mode::ReadWrite)?;
                let s = Store::mount(&mut f, policy())?;
                ctl.trace(None, None);
                ctl.inject(SimFault::Flush {
                    ordinal,
                    persist_newest: persistent,
                })?;
                let err = s.write(&mut f, &k, g, name, 0, b"apres").unwrap_err();
                let uncertain = matches!(err, Error::Publication { .. } | Error::UnknownOutcome);
                assert_eq!(
                    uncertain,
                    ordinal == barriers,
                    "publication visible {name} FLUSH {ordinal} {err}"
                );
                assert!(f.accounting().is_err());
                let events = ctl.events();
                assert!(matches!(
                    s.write(&mut f, &k, g, "suite", 0, b"refus"),
                    Err(Error::Unavailable | Error::Io(_))
                ));
                assert_eq!(
                    ctl.events(),
                    events,
                    "la requête suivante a touché le disque"
                );
                drop(f);
                ctl.power_cut(Persistence::None)?;
                let mut f = Engine::mount_current(ctl.snapshot().device(), IDS, Mode::ReadWrite)?;
                let s = Store::mount(&mut f, policy())?;
                assert_eq!(s.recover(&mut f)?.quarantined, 0);
                let seen = s.read(&f, &k, g, name, 0, 10);
                if uncertain && persistent == 1 {
                    assert_eq!(seen?, b"apres");
                } else if fresh {
                    assert!(matches!(seen, Err(Error::NotFound)));
                } else {
                    assert_eq!(seen?, b"avant");
                }
                assert!(matches!(
                    s.read(&f, &k, g, "suite", 0, 10),
                    Err(Error::NotFound)
                ));
                document(&f)?;
            }
        }
    }
    println!(
        "secteur={sector} 16 erreurs_FLUSH première-écriture + écrasement : statut certain avant publication, F2 final incertain, requête ultérieure sans E/S, remontage vérifié"
    );
    Ok(())
}
fn main() -> Result<()> {
    for s in [512, 4096] {
        real(s)?;
        quotas(s)?;
        corrupt(s)?;
        corrupt_scope(s)?;
        cuts(s)?;
        more_cuts(s, false)?;
        more_cuts(s, true)?;
        flush_status(s)?;
    }
    println!("CACHE-P9 STOCKAGE OK {}", std::env::consts::ARCH);
    Ok(())
}
