//! P01/P02 exercent les publications du catalogue et du FS sur le même
//! périphérique. Ce gestionnaire de volumes de test n’est pas un service NovaOS.
#[path = "support/pool.rs"]
mod pool;
use novafs::device::{Event, Persistence, SimDevice, SimSnapshot};
use novafs::{Device, Engine, Error, Ids, Mode, Result};
use pool::{GRAIN, Pool, Type, Uid};

const LV: Uid = [0x11; 16];
const FS: Uid = [0x21; 16];
const RAW: Uid = [0x31; 16];
const NEW: Uid = [0x41; 16];
const IDS: Ids = Ids { fs: FS, lv: LV };

#[test]
fn p01_typed_exports_geometry_and_zero_on_reassignment() -> Result<()> {
    for sector in [512, 4096] {
        let d = SimDevice::new(3 * GRAIN + 11, sector)?;
        let ctl = d.control();
        let mut p = Pool::format(d)?;
        p.create(LV, Some(FS), 1)?;
        p.create_for([0x81; 16], RAW, None, 1)?;
        assert!(matches!(p.create(RAW, None, 1), Err(Error::Exists)));
        assert!(matches!(p.create(NEW, None, 1), Err(Error::NoSpace)));
        assert!(p.view(LV, Type::Raw, [0; 16]).is_err());
        assert!(p.view(RAW, Type::Ready, FS).is_err());
        assert!(p.mark_ready(LV, FS).is_err());
        let v = p.view(LV, Type::Preparing, FS)?;
        let mut fs = Engine::format(v, IDS)?;
        fs.create("/preuve")?;
        fs.write("/preuve", 0, "identité et plage".as_bytes())?;
        fs.unmount()?;
        p.mark_ready(LV, FS)?;
        assert!(p.raw_for([0x82; 16], RAW).is_err());
        let mut raw = p.raw_for([0x81; 16], RAW)?;
        raw.write(0, &[0xbb; 4096])?;
        raw.write(GRAIN - 1, &[0xab; 4096])?;
        assert!(raw.write(GRAIN, &[0x42; 4096]).is_err());
        assert!(raw.read(u64::MAX).is_err());
        raw.flush()?;
        drop(raw);
        assert_eq!(ctl.snapshot().device().read(2 * GRAIN)?, [0xbb; 4096]);
        assert_eq!(ctl.snapshot().device().read(3 * GRAIN)?, [0; 4096]);
        let fs = Engine::mount(p.view(LV, Type::Ready, FS)?, IDS, Mode::ReadOnly)?;
        assert_eq!(fs.read("/preuve", 0, 128)?, "identité et plage".as_bytes());
        fs.unmount()?;
        p.remove(RAW)?;
        // Le catalogue précédent possède encore les blocs de RAW.
        assert!(matches!(p.create(NEW, None, 1), Err(Error::NoSpace)));
        p.advance()?;
        p.create_for([0x82; 16], NEW, None, 1)?;
        assert!(p.raw_for([0x81; 16], NEW).is_err());
        let raw = p.raw_for([0x82; 16], NEW)?;
        for b in 0..GRAIN {
            assert_eq!(raw.read(b)?, [0; 4096]);
        }
        drop(raw);
        drop(p);
        let mut remount = Pool::import(ctl.snapshot().device())?;
        assert!(remount.view(RAW, Type::Raw, [0; 16]).is_err());
        assert!(remount.raw_for([0x81; 16], NEW).is_err());
        assert!(remount.raw_for([0x82; 16], NEW).is_ok());
        let mut q = Pool::format(SimDevice::new(8 * GRAIN, sector)?)?;
        q.create_for([0x81; 16], RAW, None, 3)?;
        assert!(matches!(
            q.create_for([0x81; 16], NEW, None, 2),
            Err(Error::Limit(_))
        ));
        q.create_for([0x82; 16], NEW, None, 2)?;
    }
    Ok(())
}

fn pool_cut_matrix(
    label: &str,
    base: &SimSnapshot,
    op: impl Fn(&mut Pool<SimDevice>) -> Result<()>,
) -> Result<()> {
    let d = base.device();
    let h = d.control();
    let mut p = Pool::import(d)?;
    let initial = p.head.generation;
    let old_entries = entry_ids(&p);
    h.trace(None, None);
    op(&mut p)?;
    let new_entries = entry_ids(&p);
    let events = h.events();
    let new = p.head.generation;
    let slot = new % 2;
    let submitted = events
        .iter()
        .position(|&e| e == Event::WriteSubmitted(slot))
        .unwrap();
    let stable = events
        .iter()
        .position(|&e| e == Event::FlushEffect(2))
        .unwrap();
    // Inclure chaque écriture de remise à zéro du grain de 256 blocs.
    let mut tested = 0;
    for i in 0..events.len() {
        for policy in [Persistence::None, Persistence::Newest(1)] {
            let d = base.device();
            let c = d.control();
            let mut p = Pool::import(d)?;
            c.trace(Some(i), None);
            assert!(op(&mut p).is_err(), "{label} frontière {i}");
            c.power_cut(policy)?;
            let restored = Pool::import(c.snapshot().device())?;
            if restored.head.generation == initial {
                assert!(i < stable);
                assert_eq!(entry_ids(&restored), old_entries);
            } else {
                assert!(i >= submitted);
                assert_eq!(restored.head.generation, new);
                assert_eq!(entry_ids(&restored), new_entries);
            }
            tested += 1;
        }
    }
    println!("P02 {label}: {tested} coupures");
    Ok(())
}
fn entry_ids(p: &Pool<SimDevice>) -> Vec<(Uid, Uid, Type, u64)> {
    p.head
        .entries
        .iter()
        .map(|e| (e.id, e.fs, e.kind, e.grains))
        .collect()
}

#[test]
fn p02_independent_catalogue_fs_commits_and_stale_identity() -> Result<()> {
    let disk = SimDevice::new(4 * GRAIN, 512)?;
    let host = disk.control();
    let mut p = Pool::format(disk)?;
    pool_cut_matrix("préparation", &host.snapshot(), |p| {
        p.create(LV, Some(FS), 1)
    })?;
    p.create(LV, Some(FS), 1)?;
    let reserved = host.snapshot();
    let reopening = Pool::import(reserved.device())?;
    assert_eq!(reopening.head.entries[0].kind, Type::Preparing);
    assert!(p.view(LV, Type::Ready, FS).is_err());
    let fs = Engine::format(p.view(LV, Type::Preparing, FS)?, IDS)?;
    drop(fs);
    let initialized = host.snapshot();
    // Interrompre le formatage interne sans permettre une publication « prêt ».
    let d = reserved.device();
    let c = d.control();
    let mut q = Pool::import(d)?;
    c.trace(None, None);
    Engine::format(q.view(LV, Type::Preparing, FS)?, IDS)?;
    let events = c.events();
    for i in 0..events.len() {
        let disk = reserved.device();
        let ctl = disk.control();
        let mut pool = Pool::import(disk)?;
        ctl.trace(Some(i), None);
        assert!(Engine::format(pool.view(LV, Type::Preparing, FS)?, IDS).is_err());
        ctl.power_cut(Persistence::None)?;
        let mut after = Pool::import(ctl.snapshot().device())?;
        assert!(after.view(LV, Type::Ready, FS).is_err());
        let result = after.mark_ready(LV, FS);
        let survived = i
            >= events
                .iter()
                .position(|&e| e == Event::FlushEffect(2))
                .unwrap();
        assert_eq!(result.is_ok(), survived);
    }
    println!("P02 formatage du LV: {} coupures", events.len());
    pool_cut_matrix("passage à prêt", &initialized, |p| p.mark_ready(LV, FS))?;
    p.mark_ready(LV, FS)?;
    let mut fs = Engine::mount(p.view(LV, Type::Ready, FS)?, IDS, Mode::ReadWrite)?;
    fs.create("/ancien")?;
    let object = fs.stat("/ancien")?.id;
    fs.unmount()?;
    pool_cut_matrix("suppression", &host.snapshot(), |p| p.remove(LV))?;
    p.remove(LV)?;
    p.advance()?;
    let newfs = [0x91; 16];
    pool_cut_matrix("réattribution", &host.snapshot(), |p| {
        p.create(LV, Some(newfs), 1)
    })?;
    p.create(LV, Some(newfs), 1)?;
    let new_ids = Ids { fs: newfs, lv: LV };
    Engine::format(p.view(LV, Type::Preparing, newfs)?, new_ids)?;
    p.mark_ready(LV, newfs)?;
    let mut fs = Engine::mount(p.view(LV, Type::Ready, newfs)?, new_ids, Mode::ReadWrite)?;
    fs.create("/nouveau")?;
    assert_eq!(fs.stat("/nouveau")?.id, object);
    fs.unmount()?;
    assert!(p.view(LV, Type::Ready, FS).is_err());
    assert!(Engine::mount(p.view(LV, Type::Ready, newfs)?, IDS, Mode::ReadWrite).is_err());
    Ok(())
}

#[test]
fn p02_pool_barrier_negative_controls() -> Result<()> {
    let d = SimDevice::new(4 * GRAIN, 512)?;
    let h = d.control();
    let p = Pool::format(d)?;
    let old = p.head.generation;
    let base = h.snapshot();
    let d = base.device();
    let h = d.control();
    let mut p = Pool::import(d)?;
    h.trace(None, None);
    p.create(RAW, None, 1)?;
    let on_anchor = h
        .events()
        .iter()
        .position(|e| *e == Event::FlushSubmitted(2))
        .unwrap();
    let newer = p.head.generation;
    // F1 omise, l'ancre seule nomme une bande encore volatile.
    let d = base.device();
    let h = d.control();
    let mut p = Pool::import(d)?;
    h.trace(Some(on_anchor), Some(1));
    assert!(p.create(RAW, None, 1).is_err());
    h.power_cut(Persistence::Newest(1))?;
    assert!(Pool::import(h.snapshot().device()).is_err());
    // F2 omise, un succès de création est perdu sous le contrôle de l'oracle.
    let d = base.device();
    let h = d.control();
    let mut p = Pool::import(d)?;
    h.trace(None, Some(2));
    p.create(RAW, None, 1)?;
    assert_eq!(p.head.generation, newer);
    h.power_cut(Persistence::None)?;
    let p = Pool::import(h.snapshot().device())?;
    assert_eq!(p.head.generation, old);
    assert!(p.head.entries.is_empty());
    Ok(())
}
