//! Oracles différentiels de contenu et images malformées à sommes valides.
use super::*;
use crate::{Ids, Mode, device::SimDevice};

const IDS: Ids = Ids {
    fs: [1; 16],
    lv: [2; 16],
};
fn laboratory() -> Engine<SimDevice> {
    Engine::format(SimDevice::new(2048, 512).unwrap(), IDS).unwrap()
}

#[test]
fn n01_replacement_collects_and_reuses_verified_blocks() {
    let mut engine = laboratory();
    engine.create("/x").unwrap();
    // Un lot complet contigu puis une retouche au milieu du run ; les deux
    // fragments extérieurs conservent leurs anciennes empreintes/naissances.
    let mut model = (0..MAX_IO)
        .map(|n| (n.wrapping_mul(37) >> 5) as u8)
        .collect::<Vec<_>>();
    engine.write("/x", 0, &model).unwrap();
    assert!(engine.extent_count("/x").unwrap() < 16);
    let off = 4096 * 6 - 3;
    engine.write("/x", off as u64, &[0xca; 7]).unwrap();
    model[off..off + 7].fill(0xca);
    assert_eq!(engine.read("/x", 0, MAX_IO).unwrap(), model);
    let end = model.len() + 8193;
    engine.write("/x", end as u64, &[0xcd]).unwrap();
    model.resize(end + 1, 0);
    model[end] = 0xcd;
    // Écritures dont le dernier dépasse le bloc final, lacunes et fin de fichier.
    for i in [0, off - 5, off + 2, 65530, end - 10, end, end + 1] {
        let expected = model.get(i..).unwrap_or(&[]);
        assert_eq!(
            engine.read("/x", i as u64, 48).unwrap(),
            &expected[..expected.len().min(48)]
        );
    }
    engine.verify().unwrap();
    let id = engine.stat("/x").unwrap().id;
    let old = data::covering(&engine, id, 0)
        .unwrap()
        .unwrap()
        .1
        .reference(0);
    engine.unlink("/x").unwrap();
    assert!(matches!(engine.stat("/x"), Err(Error::NotFound)));
    // La collecte continue à partir du curseur persistant après remontage.
    assert!(engine.collect().unwrap());
    let mut engine = Engine::mount(engine.unmount().unwrap(), IDS, Mode::ReadWrite).unwrap();
    while engine.collect().unwrap() {}
    engine.create("/reuse").unwrap();
    let reuse_oid = engine.stat("/reuse").unwrap().id;
    assert!(reuse_oid > id);
    let mut reused = false;
    for i in 0..16 {
        engine.write("/reuse", i * 65536, &[0x55; 65536]).unwrap();
        let mut cursor = 0;
        while let Some((logical, run)) = data::first_extent(&engine, reuse_oid, cursor).unwrap() {
            reused |= (run.first..run.first + run.hashes.len() as u64).contains(&old.block);
            cursor = run.end(logical).unwrap();
        }
        if reused {
            break;
        }
    }
    assert!(reused, "le cadre témoin doit être réattribué");
    engine.verify().unwrap();
}

#[test]
fn n01_names_split_delete_and_contract() {
    let mut engine = laboratory();
    engine.mkdir("/d").unwrap();
    let suffix = "x".repeat(190);
    for i in 0..150 {
        engine.create(&format!("/d/{i:03}{suffix}")).unwrap();
    }
    engine.verify().unwrap();
    let mut observed = Vec::new();
    loop {
        let page = engine
            .list("/d", observed.last().map(String::as_str), 19)
            .unwrap();
        if page.is_empty() {
            break;
        }
        observed.extend(page);
    }
    assert_eq!(
        observed,
        (0..150)
            .map(|i| format!("{i:03}{suffix}"))
            .collect::<Vec<_>>()
    );
    for name in observed.into_iter().rev() {
        engine.unlink(&format!("/d/{name}")).unwrap();
    }
    assert!(engine.list("/d", None, 1).unwrap().is_empty());
    engine.unlink("/d").unwrap();
    engine.verify().unwrap();
}

#[test]
fn n04_reject_semantic_corruption_even_with_valid_digests() {
    for defect in 0..5 {
        let mut engine = laboratory();
        engine.create("/a").unwrap();
        engine.create("/b").unwrap();
        engine.create("/hole").unwrap();
        let deleted = engine.stat("/hole").unwrap().id;
        engine.unlink("/hole").unwrap();
        let aid = engine.stat("/a").unwrap().id;
        let bid = engine.stat("/b").unwrap().id;
        engine.write("/a", 0, &[0x9c; 8192]).unwrap();
        // L'encodeur de production régénère toutes les sommes et réalise le commit.
        engine
            .transact(false, |tx| {
                match defect {
                    0 => {
                        let mut obj = live(tx, bid)?;
                        obj.name = "autre".into();
                        obj.stat.revision = generation(tx)?;
                        save_object(tx, &obj)?;
                    }
                    1 => {
                        tx.put(Family::Garbage, bid.to_le_bytes().to_vec(), gc_value(123))?;
                    }
                    2 => {
                        // L'extent change d'objet entre les générations, avec un seul propriétaire par vue.
                        let key = extent_key(aid, 0);
                        let val = get(tx, Family::Extents, &key)?.unwrap();
                        tx.remove(Family::Extents, &key)?;
                        tx.put(Family::Extents, extent_key(bid, 0).into(), val)?;
                        let mut obj = live(tx, bid)?;
                        obj.stat.size = 8192;
                        obj.stat.revision = generation(tx)?;
                        save_object(tx, &obj)?;
                    }
                    3 => {
                        // Réutilisation d’un numéro dans un trou antérieur au compteur.
                        save_object(
                            tx,
                            &Object::new(deleted, Kind::File, ROOT, "recycled", generation(tx)?),
                        )?;
                        tx.put(
                            Family::Names,
                            name_key(ROOT, "recycled"),
                            deleted.to_le_bytes().to_vec(),
                        )?;
                    }
                    _ => {
                        let (start, run) = data::first_extent(tx, aid, 0)?.unwrap();
                        let mut second = run.slice(0..1);
                        second.hashes[0] = [0u8; 32];
                        tx.put(
                            Family::Extents,
                            extent_key(aid, start + 1).to_vec(),
                            second.encode(),
                        )?;
                    }
                }
                Ok(())
            })
            .unwrap();
        let image = engine.into_device();
        let stable = image.control().snapshot().bytes().unwrap();
        assert!(
            matches!(
                Engine::mount(image, IDS, Mode::ReadWrite),
                Err(Error::Corrupt(_))
            ),
            "défaut {defect}"
        );
        // Un second montage RO des mêmes octets doit faire le même diagnostic.
        assert!(matches!(
            Engine::mount(
                SimDevice::from_bytes(&stable, 4096).unwrap(),
                IDS,
                Mode::ReadOnly
            ),
            Err(Error::Corrupt(_))
        ));
    }
}

#[test]
fn n04_checksum_fault_freezes_writer_and_hides_partial_read() {
    let dev = SimDevice::new(1024, 4096).unwrap();
    let control = dev.control();
    let mut engine = Engine::format(dev, IDS).unwrap();
    engine.create("/x").unwrap();
    engine.write("/x", 0, &[3; 8192]).unwrap();
    let id = engine.stat("/x").unwrap().id;
    let (start, run) = data::covering(&engine, id, 1).unwrap().unwrap();
    let r = run.reference((1 - start) as usize);
    let mut damaged = engine.raw(r.block).unwrap();
    damaged[0] ^= 1;
    control.replace_stable(r.block, damaged).unwrap();
    assert!(matches!(engine.read("/x", 4093, 8), Err(Error::Corrupt(_))));
    assert!(matches!(
        engine.write("/x", 0, &[7]),
        Err(Error::Unavailable)
    ));
    assert!(matches!(engine.unmount(), Err(Error::Unavailable)));
}

#[test]
fn p2_extent_ne_apres_la_revision_du_fichier() -> Result<()> {
    let mut engine = laboratory();
    engine.create("/x")?;
    engine.write("/x", 0, &[5; 4096])?;
    let id = engine.stat("/x")?.id;
    engine.transact(false, |tx| {
        let (_, old) = data::covering(tx, id, 0)?.unwrap();
        tx.retire_block(old.first)?;
        let replacement = tx.alloc_data(&[6; 4096])?;
        // Toutes les sommes et l'allocation sont justes. Seule la révision
        // de l'objet n'a pas été mise à jour avec l'extent de remplacement.
        let stale = Run {
            first: replacement.block,
            birth: replacement.birth,
            hashes: vec![replacement.hash],
        };
        tx.put(Family::Extents, extent_key(id, 0).into(), stale.encode())
    })?;
    let newest = engine.generation() as usize % 2;
    let image = engine.into_device();
    let report = crate::check_image(&image, IDS);
    assert!(report.slots[1 - newest].issue.is_none());
    assert!(matches!(
        report.slots[newest].issue.as_ref().map(|i| &i.error),
        Some(Error::Corrupt("extent postérieur à la révision du fichier"))
    ));
    assert!(matches!(report.pair, crate::PairStatus::Unavailable));
    assert!(matches!(
        Engine::mount(image, IDS, Mode::ReadOnly),
        Err(Error::Corrupt(_))
    ));
    Ok(())
}
