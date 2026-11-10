#[path = "../lab/p6_donnees.rs"]
mod donnees;
use donnees::{donnees, suivant};
use novafs::device::{Event, Persistence, SimDevice, SimFault};
use novafs::shared::SharedPool;
use novafs::shared_export::{export_view, restore_with_compression};
use novafs::{Compression, Error, Ids, Result};

const IDS: Ids = Ids {
    fs: [11; 16],
    lv: [12; 16],
};
const K: usize = 4096;
type Pool = SharedPool<SimDevice>;

fn creer(n: u64, sector: u32) -> Result<(Pool, u64)> {
    let mut p = SharedPool::format_p6(SimDevice::new(n, sector)?, [19; 16])?;
    let o = p.create([1; 16], IDS, 256)?;
    p.set_compression(o, Compression::Lz4)?;
    p.create_file(o, "/f")?;
    Ok((p, o))
}

fn verifier(p: &Pool, o: u64, expected: &[u8]) -> Result<()> {
    for (i, block) in expected.chunks(16 * K).enumerate() {
        assert_eq!(
            p.read_file(o, "/f", (i * 16 * K) as u64, block.len())?,
            block
        );
    }
    assert!(p.read_file(o, "/f", expected.len() as u64, 1)?.is_empty());
    Ok(())
}

#[test]
fn partage_partiel_et_restitution_autonome() -> Result<()> {
    for sector in [512, 4096] {
        let (mut p, o) = creer(4096, sector)?;
        let original = donnees(32 * K, "repetitif", 17);
        for (i, piece) in original.chunks(16 * K).enumerate() {
            p.write_file(o, "/f", (i * 16 * K) as u64, piece)?;
        }
        let before = p.compression_usage(o)?;
        assert!(before.compressed_units > 0 && before.saved_bytes > 0);
        let s = p.snapshot(o)?;
        let a = p.clone_snapshot(s, [2; 16])?;
        let b = p.clone_snapshot(s, [3; 16])?;
        assert!(p.usage(a)?.shared > 0);
        let mut expected = original.clone();
        for (offset, count) in [
            (7 * K + 19, 1),
            (15 * K + 37, K),
            (12 * K + 1, 4 * K),
            (16 * K, 1),
        ] {
            let delta = donnees(count, "aleatoire", offset as u64);
            p.write_file(a, "/f", offset as u64, &delta)?;
            expected[offset..offset + count].copy_from_slice(&delta);
            p = SharedPool::open(p.into_device())?;
            verifier(&p, a, &expected)?;
            verifier(&p, b, &original)?;
            verifier(&p, s, &original)?;
        }
        assert!(matches!(
            p.set_compression(s, Compression::None),
            Err(Error::ReadOnly)
        ));
        let encodage = p.compression_usage(a)?;
        p.set_compression(a, Compression::None)?;
        p = SharedPool::open(p.into_device())?;
        assert_eq!(p.info(a)?.policy, Compression::None);
        assert_eq!(p.info(b)?.policy, Compression::Lz4);
        assert_eq!(p.compression_usage(a)?, encodage);
        // Basculer la politique n'effectue aucune recompression des pages historiques.
        let saved = p.compression_usage(s)?.saved_bytes;
        assert_eq!(saved, before.saved_bytes);
        p.remove(o)?;
        p.advance()?;
        p = SharedPool::open(p.into_device())?;
        verifier(&p, a, &expected)?;
        verifier(&p, s, &original)?;
        let mut archive = Vec::new();
        export_view(&p, s, sector, &mut archive)?;
        p.remove(a)?;
        p.remove(b)?;
        p.remove(s)?;
        p.advance()?;
        assert_eq!(p.pool_usage()?.0, 7);
        drop(p);
        for policy in [Compression::None, Compression::Lz4] {
            let mut dest = SharedPool::format_p6(SimDevice::new(4096, sector)?, [99; 16])?;
            let id = restore_with_compression(&mut dest, &archive[..], [9; 16], policy)?;
            let imported = SharedPool::open(dest.into_device())?;
            verifier(&imported, id, &original)?;
            assert_eq!(
                imported.compression_usage(id)?.saved_bytes > 0,
                policy == Compression::Lz4
            );
        }
    }
    Ok(())
}

#[test]
fn donnees_incompressibles_quota_et_refus_de_divergence() -> Result<()> {
    for sector in [512, 4096] {
        let (mut p, o) = creer(1024, sector)?;
        let bytes = donnees(16 * K, "aleatoire", 0xb45d);
        p.write_file(o, "/f", 0, &bytes)?;
        assert_eq!(p.compression_usage(o)?.compressed_units, 0);
        let reserve = p.pool_usage()?.2;
        let compressible = donnees(16 * K, "repetitif", 0x94);
        p.write_file(o, "/f", 0, &compressible)?;
        assert_eq!(p.pool_usage()?.2, reserve);
        assert!(p.compression_usage(o)?.saved_bytes > 0);
        let s = p.snapshot(o)?;
        let mut count = 0;
        loop {
            match p.clone_snapshot(s, [count + 20; 16]) {
                Ok(_) => count += 1,
                Err(Error::PoolFull) => break,
                Err(e) => return Err(e),
            }
        }
        let g = p.generation();
        assert!(matches!(
            p.write_file(o, "/f", 256 * K as u64, &[42]),
            Err(Error::QuotaExceeded)
        ));
        assert_eq!(g, p.generation());
        let p = SharedPool::open(p.into_device())?;
        verifier(&p, o, &compressible)?;
        verifier(&p, s, &compressible)?;
    }
    Ok(())
}

#[test]
fn campagnes_aleatoires_compression_et_coupures() -> Result<()> {
    for sector in [512, 4096] {
        for seed in [0x2911a3, 0x6c28e1] {
            let mut rng = seed;
            let (mut p, o) = creer(4096, sector)?;
            let mut current = donnees(16 * K, "mixte", seed);
            p.write_file(o, "/f", 0, &current)?;
            let mut coupures = 0;
            let mut eio = 0;
            for step in 0..24 {
                let s = p.snapshot(o)?;
                let c = p.clone_snapshot(s, [29; 16])?;
                let (count, offset) = match step % 4 {
                    0 => (1, (suivant(&mut rng) as usize) % current.len()),
                    1 => (K, (suivant(&mut rng) as usize) % (15 * K)),
                    2 => (4 * K, (suivant(&mut rng) as usize) % (12 * K)),
                    _ => (16 * K, 0),
                };
                let payload = donnees(
                    count,
                    ["aleatoire", "mixte", "repetitif"][step % 3],
                    suivant(&mut rng),
                );
                let base = p.into_device().control().snapshot();
                let disk = base.device();
                let ctl = disk.control();
                let mut refpool = SharedPool::open(disk)?;
                ctl.trace(None, None);
                refpool.write_file(c, "/f", offset as u64, &payload)?;
                let events = ctl.events();
                let publish = events
                    .iter()
                    .position(|e| matches!(e, Event::WriteSubmitted(2)))
                    .unwrap();
                let f2 = events
                    .iter()
                    .position(|e| matches!(e, Event::FlushEffect(3)))
                    .unwrap();
                assert!(publish < f2);
                let mut oracle = current.clone();
                oracle[offset..offset + count].copy_from_slice(&payload);
                let disk = base.device();
                let ctl = disk.control();
                let mut mutating = SharedPool::open(disk)?;
                let new = if step % 2 == 0 {
                    let at = (suivant(&mut rng) as usize) % events.len();
                    let keep = if step % 4 == 0 { 0 } else { usize::MAX };
                    ctl.trace(Some(at), None);
                    assert!(
                        mutating
                            .write_file(c, "/f", offset as u64, &payload)
                            .is_err()
                    );
                    ctl.power_cut(Persistence::Newest(keep))?;
                    coupures += 1;
                    at >= f2 || (at >= publish && keep > 0)
                } else {
                    let ordinal = 1 + (step % 3) as u32;
                    let keep = if step % 4 == 1 { 0 } else { usize::MAX };
                    ctl.inject(SimFault::Flush {
                        ordinal,
                        persist_newest: keep,
                    })?;
                    assert!(
                        mutating
                            .write_file(c, "/f", offset as u64, &payload)
                            .is_err()
                    );
                    ctl.power_cut(Persistence::None)?;
                    eio += 1;
                    ordinal == 3 && keep > 0
                };
                drop(mutating);
                p = SharedPool::open(ctl.snapshot().device())?;
                verifier(&p, c, if new { &oracle } else { &current })?;
                verifier(&p, s, &current)?;
                verifier(&p, o, &current)?;
                p.remove(c)?;
                p.remove(s)?;
                p.advance()?;
                p.set_compression(
                    o,
                    if step % 3 == 0 {
                        Compression::None
                    } else {
                        Compression::Lz4
                    },
                )?;
                p.write_file(o, "/f", offset as u64, &payload)?;
                current = oracle;
            }
            println!(
                "P6_ALEA secteur={sector} graine={seed:x} actions=24 coupures={coupures} EIO={eio}"
            );
        }
    }
    Ok(())
}

#[test]
fn mutation_compressee_a_chaque_frontiere_de_publication() -> Result<()> {
    for sector in [512, 4096] {
        let (mut p, o) = creer(2048, sector)?;
        let old = donnees(16 * K, "repetitif", 5);
        p.write_file(o, "/f", 0, &old)?;
        let s = p.snapshot(o)?;
        let c = p.clone_snapshot(s, [3; 16])?;
        let base = p.into_device().control().snapshot();
        let mut new = old.clone();
        let offset = 2 * K + 31;
        let payload = donnees(4 * K, "repetitif", 51);
        new[offset..offset + payload.len()].copy_from_slice(&payload);
        let disk = base.device();
        let ctl = disk.control();
        let mut p = SharedPool::open(disk)?;
        ctl.trace(None, None);
        p.write_file(c, "/f", offset as u64, &payload)?;
        let events = ctl.events();
        let publish = events
            .iter()
            .position(|e| matches!(e, Event::WriteSubmitted(2)))
            .unwrap();
        let f2 = events
            .iter()
            .position(|e| matches!(e, Event::FlushEffect(3)))
            .unwrap();
        assert!(publish < f2);
        let validate = |disk: SimDevice, published: bool| -> Result<()> {
            let mut p = SharedPool::open(disk)?;
            verifier(&p, c, if published { &new } else { &old })?;
            verifier(&p, s, &old)?;
            p.advance()?;
            verifier(
                &SharedPool::open(p.into_device())?,
                c,
                if published { &new } else { &old },
            )
        };
        for n in 0..events.len() {
            for retain in [0, 1, usize::MAX] {
                let disk = base.device();
                let ctl = disk.control();
                let mut p = SharedPool::open(disk)?;
                ctl.trace(Some(n), None);
                assert!(p.write_file(c, "/f", offset as u64, &payload).is_err());
                ctl.power_cut(Persistence::Newest(retain))?;
                validate(
                    ctl.snapshot().device(),
                    n >= f2 || (n >= publish && retain > 0),
                )?;
            }
        }
        let mut errors = 0;
        for pos in events.iter().filter_map(|e| {
            if let Event::BeforeWrite(b) = e {
                Some(*b)
            } else {
                None
            }
        }) {
            for bytes in [0, 4096] {
                let disk = base.device();
                let ctl = disk.control();
                let mut p = SharedPool::open(disk)?;
                ctl.inject(SimFault::Write {
                    block: pos,
                    prefix: bytes,
                })?;
                assert!(p.write_file(c, "/f", offset as u64, &payload).is_err());
                ctl.power_cut(Persistence::Newest(usize::MAX))?;
                validate(ctl.snapshot().device(), pos == 2 && bytes == 4096)?;
                errors += 1;
            }
        }
        println!(
            "P6_PANNE secteur={sector} frontieres={} coupures={} EIO_ecriture={errors}",
            events.len(),
            3 * events.len()
        );
    }
    Ok(())
}
