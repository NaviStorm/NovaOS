// SPDX-License-Identifier: MPL-2.0
//! Mesure des opérations P9 sur fichiers temporaires Darwin, clés éphémères.
#[path = "../lab/heap_meter.rs"]
mod heap_meter;
use novafs::crypto::{Credential, HostEntropy, Secret};
use novafs::device::FileImage;
use novafs::secure_pool::{SecurePool, migration::Enrollment};
use novafs::time::{Atime, Times};
use novafs::{Compression, Device, Error, Ids, Result};
use serde_json::{Value, json};
use std::sync::atomic::Ordering::Relaxed;
use std::time::Instant;

fn measure<T>(f: impl FnOnce() -> Result<T>) -> Result<(T, u64)> {
    let at = Instant::now();
    let result = f()?;
    Ok((result, at.elapsed().as_nanos() as u64))
}
fn distribution(mut samples: Vec<u64>) -> Value {
    samples.sort_unstable();
    let n = samples.len();
    let q = |p: usize| samples[(p * n).div_ceil(100) - 1];
    json!({"n":n,"p50_ns":q(50),"p95_ns":q(95),"p99_ns":q(99),"max_ns":q(100)})
}

fn crypto_throughput() -> Result<(u64, u64)> {
    use novafs::secure_unit::{Context, Keys, Kind, open, seal};
    use zeroize::Zeroizing;
    let source = Context {
        pool: [11; 16],
        domain: [12; 16],
        physical: 70,
        logical: 0,
        generation: 1,
        kind: Kind::Data,
    };
    let target = Context {
        pool: [21; 16],
        domain: [22; 16],
        ..source
    };
    let master = Secret::generate(&mut HostEntropy)?;
    let next = Secret::generate(&mut HostEntropy)?;
    let before = Keys::derive(&master, &source.domain)?;
    let after = Keys::derive(&next, &target.domain)?;
    let sample = [0xa9; 4096];
    let (h, p, d) = seal(source, Some(&before), &mut HostEntropy, &sample)?;
    let pages = 256u64;
    let (_, ns) = measure(|| {
        for i in 0..pages {
            let plain = Zeroizing::new(open(source, Some(&before), &h, &p, &d)?);
            let ctx = Context {
                physical: target.physical + 2 * i,
                ..target
            };
            let (nh, np, nd) = seal(ctx, Some(&after), &mut HostEntropy, &plain)?;
            let decoded = Zeroizing::new(open(ctx, Some(&after), &nh, &np, &nd)?);
            if *decoded != sample {
                return Err(Error::Corrupt("relecture du banc de réchiffrement"));
            }
        }
        Ok(())
    })?;
    Ok((pages * 4096, ns))
}

fn run(sector: u32, count: usize) -> Result<Value> {
    let a = Secret::generate(&mut HostEntropy)?;
    let admin = Secret::generate(&mut HostEntropy)?;
    let recovery = Secret::generate(&mut HostEntropy)?;
    let next = Secret::generate(&mut HostEntropy)?;
    let mut p = SecurePool::format(FileImage::create(3000, sector)?, [46; 16], &a)?;
    let o = p.create(
        [2; 16],
        Ids {
            fs: [4; 16],
            lv: [3; 16],
        },
        256,
        Compression::Lz4,
        Some((b"source labo", &recovery)),
    )?;
    let old = p.domain(o)?;
    p.create_file(o, "/fixe")?;
    let pattern = (0..65536).map(|i| (i % 127) as u8).collect::<Vec<_>>();
    for i in 0..4 {
        p.write_file(o, "/fixe", i * 65536, &pattern)?;
    }
    let s = p.snapshot(o)?;
    let c = p.clone_snapshot(s, [7; 16])?;
    p.write_file(c, "/fixe", 4097, b"coW")?;
    let per = [Enrollment {
        old_domain: old,
        phrase: b"destination labo",
        recovery: &next,
    }];
    let before = heap_meter::LIVE.load(Relaxed);
    heap_meter::PHASE_PEAK.store(before, Relaxed);
    let allocations = heap_meter::ALLOCATIONS.load(Relaxed);
    let mut migrations = Vec::new();
    let mut last = None;
    for _ in 0..count {
        let d = FileImage::create(3000, sector)?;
        let (result, ns) = measure(|| p.migrate_p9(d, &admin, &per, HostEntropy))?;
        migrations.push(ns);
        last = Some(result);
    }
    let peak = heap_meter::PHASE_PEAK.load(Relaxed);
    let allocations = heap_meter::ALLOCATIONS.load(Relaxed) - allocations;
    let (target, receipt) = last.ok_or(Error::Invalid("nombre de campagnes"))?;
    p.verify_migration_source(&receipt)?;
    let source_usage = p.view_usage(s)?;
    let mut p8_create = Vec::new();
    let mut p8_write = Vec::new();
    let mut p8_rename = Vec::new();
    // Comparaison sur la même lignée de référence, avant de libérer sa copie privée.
    let old_medium = p.into_device();
    let source_disk = old_medium.allocated_bytes()?;
    let mut comparison = FileImage::create(old_medium.blocks(), sector)?;
    for index in 0..old_medium.blocks() {
        let page = old_medium.read(index)?;
        if page != [0; 4096] {
            comparison.write(index, &page)?;
        }
    }
    comparison.flush()?;
    drop(old_medium);
    let mut p = SecurePool::open(comparison, [46; 16], &a)?;
    p.unlock(old, Credential::Recovery(&recovery))?;
    for i in 0..count {
        let path = format!("/temp{i}");
        let renamed = format!("/nom{i}");
        p8_create.push(measure(|| p.create_file(c, &path))?.1);
        p8_write.push(measure(|| p.write_file(c, &path, 0, b"donnees"))?.1);
        p8_rename.push(measure(|| p.rename(c, &path, &renamed))?.1);
        p.unlink(c, &renamed)?;
    }
    drop(p);
    let allocated_after = target.allocated_bytes()?;
    let id = receipt.destination();
    let (mut dest, mount_ns) = measure(|| SecurePool::open(target, id, &admin))?;
    let d = dest.domain(c)?;
    dest.unlock(d, Credential::Recovery(&next))?;
    let target_usage = dest.view_usage(s)?;
    assert_eq!(source_usage.shared, target_usage.shared);
    assert_eq!(source_usage.exclusive, target_usage.exclusive);
    let (crypto_bytes, crypto_ns) = crypto_throughput()?;
    let mut ss = Vec::new();
    let mut create = Vec::new();
    let mut writes = Vec::new();
    let mut renames = Vec::new();
    let mut reads_noatime = Vec::new();
    let mut atimes = Vec::new();
    let mut receipts = Vec::new();
    let mut restored = Vec::new();
    let mut domain = Vec::new();
    let mut clear_domain = Vec::new();
    let mut checks = Vec::new();
    let backup = Secret::generate(&mut HostEntropy)?;
    let mut archive = Vec::new();
    dest.export(c, sector, &backup, &mut archive)?;
    let source_count = receipt.units();
    for i in 0..count {
        let (v, ns) = measure(|| dest.snapshot(c))?;
        ss.push(ns);
        dest.remove(v)?;
        let path = format!("/temp{i}");
        let renamed = format!("/nom{i}");
        create.push(measure(|| dest.create_file(c, &path))?.1);
        writes.push(measure(|| dest.write_file(c, &path, 0, b"donnees"))?.1);
        renames.push(measure(|| dest.rename(c, &path, &renamed))?.1);
        dest.set_atime_policy(c, Atime::No)?;
        reads_noatime.push(measure(|| dest.read_file(c, &renamed, 0, 16))?.1);
        dest.set_atime_policy(c, Atime::Strict)?;
        atimes.push(measure(|| dest.read_file(c, &renamed, 0, 16))?.1);
        dest.unlink(c, &renamed)?;
        let (signed, ns) = measure(|| dest.migration_receipt())?;
        assert_eq!(signed.as_ref(), Some(&receipt));
        receipts.push(ns);
        let mut target = SecurePool::format_p9(
            FileImage::create(3000, sector)?,
            [95; 16],
            &admin,
            HostEntropy,
        )?;
        restored.push(
            measure(|| {
                target.restore(
                    archive.as_slice(),
                    &backup,
                    [8; 16],
                    b"restauration labo",
                    &next,
                    Compression::Lz4,
                )
            })?
            .1,
        );
        for encrypted in [false, true] {
            let mut isolated = SecurePool::format_p9(
                FileImage::create(3000, sector)?,
                [81; 16],
                &admin,
                HostEntropy,
            )?;
            let (_, duration) = measure(|| {
                isolated.create(
                    [3; 16],
                    Ids {
                        fs: [4; 16],
                        lv: [5; 16],
                    },
                    256,
                    Compression::Lz4,
                    encrypted.then_some((b"enrolement labo".as_slice(), &next)),
                )
            })?;
            if encrypted {
                domain.push(duration);
            } else {
                clear_domain.push(duration);
            }
        }
        checks.push(measure(|| dest.check())?.1);
    }
    let times = dest.times(s, "/fixe")?;
    assert_eq!(times, Times::UNKNOWN);
    let target_disk = dest.into_device().allocated_bytes()?;
    let sum_ns = create
        .iter()
        .chain(&writes)
        .chain(&renames)
        .copied()
        .sum::<u64>();
    let debit_tx = 3.0 * count as f64 * 1e9 / sum_ns as f64;
    Ok(json!({
        "secteur": sector,
        "taille_image": 3000 * 4096,
        "occupation_source": source_disk,
        "occupation_destination_apres_migration": allocated_after,
        "occupation_destination_apres_charge": target_disk,
        "partage_snapshot_avant": source_usage.shared,
        "partage_snapshot_apres": target_usage.shared,
        "exclusifs_snapshot_avant": source_usage.exclusive,
        "exclusifs_snapshot_apres": target_usage.exclusive,
        "rechiffrement_octets": crypto_bytes,
        "rechiffrement_ns": crypto_ns,
        "unites_immuables": source_count,
        "lignes_recu": receipt.views().count(),
        "octets_recu": receipt.as_bytes().len(),
        "surcout_objet_octets": Times::UNKNOWN.encode().len(),
        "tas_initial": before,
        "tas_pic_migration": peak,
        "allocations_migrations": allocations,
        "pile_reservee": heap_meter::current_stack_size(),
        "migration": distribution(migrations),
        "montage_ns": mount_ns,
        "snapshot": distribution(ss),
        "restauration_clone": distribution(restored),
        "transactions_par_seconde": debit_tx,
        "p8_creation": distribution(p8_create),
        "p8_ecriture": distribution(p8_write),
        "p8_renommage": distribution(p8_rename),
        "enrolement_volume": distribution(domain),
        "creation": distribution(create),
        "ecriture": distribution(writes),
        "renommage": distribution(renames),
        "creation_volume_clair": distribution(clear_domain),
        "lecture_noatime": distribution(reads_noatime),
        "lecture_strictatime": distribution(atimes),
        "verification_recu": distribution(receipts),
        "verification": distribution(checks),
    }))
}
fn main() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.len() != 2 {
        return Err(Error::Invalid("secteur répétitions"));
    }
    let n: usize = args[1].parse().map_err(|_| Error::Invalid("répétitions"))?;
    if !(1..=1000).contains(&n) {
        return Err(Error::Invalid("répétitions"));
    }
    {
        use std::io::Write;
        std::io::stdout().lock().flush()?;
    }
    let before = heap_meter::LIVE.load(Relaxed);
    let data = run(args[0].parse().map_err(|_| Error::Invalid("secteur"))?, n)?;
    println!("{data}");
    drop(data);
    eprintln!(
        "tas_avant={before} tas_apres={} pic_processus={}",
        heap_meter::LIVE.load(Relaxed),
        heap_meter::PEAK.load(Relaxed)
    );
    Ok(())
}
