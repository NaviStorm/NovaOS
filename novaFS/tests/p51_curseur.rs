//! Un seul test dans ce binaire pour imputer les allocations de la lecture chaude.
#[path = "../lab/heap_meter.rs"]
#[allow(dead_code)]
mod heap_meter;
use novafs::{Device, Engine, Error, Ids, Mode, Result, device::SimDevice, shared::SharedPool};
use std::sync::atomic::Ordering;

const IDS: Ids = Ids {
    fs: [11; 16],
    lv: [12; 16],
};

fn verifier_chemin_et_racine(secteur: u32) -> Result<()> {
    let mut pool = SharedPool::format(SimDevice::new(4096, secteur)?, [3; 16])?;
    let o = pool.create([4; 16], IDS, 256)?;
    pool.mkdir(o, "/d")?;
    pool.create_file(o, "/d/f")?;
    pool.write_file(o, "/d/f", 0, &[0x49; 16384])?;
    pool.create_file(o, "/d/autre")?;
    pool.write_file(o, "/d/autre", 0, &[0x82; 4096])?;
    let s = pool.snapshot(o)?;
    let c = pool.clone_snapshot(s, [5; 16])?;

    pool.reclaim_cache()?;
    assert_eq!(pool.read_file(s, "/d/f", 4000, 128)?, [0x49; 128]);
    let before = heap_meter::ALLOCATIONS.load(Ordering::Relaxed);
    let data = pool.read_file(s, "/d/f", 3999, 129)?;
    let after = heap_meter::ALLOCATIONS.load(Ordering::Relaxed);
    assert_eq!(data, [0x49; 129]);
    assert_eq!(after - before, 1, "seul le Vec de sortie doit être alloué");
    // Même racine immuable et chemin différent : le numéro d'objet du précédent
    // fichier et ses références ne sont jamais réutilisés.
    assert_eq!(pool.read_file(c, "/d/autre", 4000, 128)?, [0x82; 96]);
    assert!(matches!(
        pool.read_file(c, "/d/absent", 0, 1),
        Err(Error::NotFound)
    ));
    assert!(matches!(
        pool.read_file(c, "/d", 0, 1),
        Err(Error::IsDirectory)
    ));
    assert!(matches!(
        pool.read_file(c, "/d//f", 0, 1),
        Err(Error::Invalid(_))
    ));
    assert!(matches!(
        pool.read_file(c, "/d/f", u64::MAX, 1),
        Err(Error::Invalid(_))
    ));

    pool.write_file(c, "/d/f", 7 * 4096 + 2, b"Z")?;
    assert_eq!(pool.read_file(c, "/d/f", 7 * 4096, 4)?, [0, 0, b'Z']);
    assert_eq!(pool.read_file(c, "/d/f", 16383, 8194)?[0], 0x49);
    assert!(
        pool.read_file(c, "/d/f", 16384, 8192)?
            .iter()
            .all(|&b| b == 0)
    );
    assert!(pool.read_file(s, "/d/f", 16384, 8192)?.is_empty());
    pool.rename(o, "/d/f", "/d/ancien")?;
    assert!(matches!(
        pool.read_file(o, "/d/f", 0, 1),
        Err(Error::NotFound)
    ));
    pool.create_file(o, "/d/f")?;
    pool.write_file(o, "/d/f", 0, b"N")?;
    assert_eq!(pool.read_file(o, "/d/f", 0, 16)?, b"N");
    assert_eq!(pool.read_file(s, "/d/f", 0, 16)?, [0x49; 16]);
    let autre = pool.create(
        [6; 16],
        Ids {
            fs: [8; 16],
            lv: [9; 16],
        },
        512,
    )?;
    pool.mkdir(autre, "/d")?;
    pool.create_file(autre, "/d/f")?;
    pool.write_file(autre, "/d/f", 0, b"autre volume")?;
    assert_eq!(pool.read_file(s, "/d/f", 0, 16)?, [0x49; 16]);
    assert_eq!(pool.read_file(autre, "/d/f", 0, 16)?, b"autre volume");
    assert_eq!(pool.read_file(o, "/d/f", 0, 16)?, b"N");
    pool.remove(o)?;
    pool.advance()?;
    let p = SharedPool::open(pool.into_device())?;
    assert_eq!(p.read_file(c, "/d/f", 7 * 4096 + 2, 1)?, b"Z");
    assert_eq!(p.read_file(s, "/d/f", 16383, 2)?, [0x49]);
    Ok(())
}

fn verifie_les_donnees_du_meme_extent(secteur: u32) -> Result<()> {
    let disque = SimDevice::new(1024, secteur)?;
    let ctl = disque.control();
    let mut pool = SharedPool::format(disque, [33; 16])?;
    let o = pool.create([4; 16], IDS, 256)?;
    pool.create_file(o, "/f")?;
    let mut payload = [0x91; 8192];
    payload[4096..].fill(0x92);
    pool.write_file(o, "/f", 0, &payload)?;
    let fs = Engine::mount(pool.read_device(o)?, IDS, Mode::ReadOnly)?;
    assert_eq!(fs.extent_count("/f")?, 1);
    drop(fs);
    pool.reclaim_cache()?;
    // Seule la première page de cet extent entre dans le cache de données.
    assert_eq!(pool.read_file(o, "/f", 0, 16)?, [0x91; 16]);
    let stable = ctl.snapshot().device();
    let mut adresse = None;
    for b in 0..stable.blocks() {
        if stable.read(b)? == [0x92; 4096] {
            assert!(adresse.replace(b).is_none());
        }
    }
    ctl.replace_stable(adresse.expect("page témoin"), [0xff; 4096])?;
    assert!(matches!(
        pool.read_file(o, "/f", 4096, 1),
        Err(Error::Corrupt(_))
    ));
    assert!(matches!(
        pool.read_file(o, "/f", 0, 1),
        Err(Error::Unavailable)
    ));
    assert!(SharedPool::open(pool.into_device()).is_err());
    Ok(())
}

#[test]
fn curseur_du_montage_cow() -> Result<()> {
    for secteur in [512, 4096] {
        verifier_chemin_et_racine(secteur)?;
        verifie_les_donnees_du_meme_extent(secteur)?;
    }
    Ok(())
}
