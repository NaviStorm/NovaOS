//! P01/P02 exercent les publications du catalogue et du FS sur le même
//! périphérique via le gestionnaire de volumes du prototype hôte.
use novafs::device::{Event, Persistence, SimDevice, SimSnapshot};
use novafs::pool::{GRAIN, Pool, Type, Uid};
use novafs::{Device, Engine, Error, Ids, Mode, Result};

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
        assert!(matches!(p.create(NEW, None, 1), Err(Error::PoolFull)));
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
        assert!(matches!(p.create(NEW, None, 1), Err(Error::PoolFull)));
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
            Err(Error::QuotaExceeded)
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
    let initial = p.generation();
    let old_entries = entry_ids(&p);
    h.trace(None, None);
    op(&mut p)?;
    let new_entries = entry_ids(&p);
    let events = h.events();
    let new = p.generation();
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
            if restored.generation() == initial {
                assert!(i < stable);
                assert_eq!(entry_ids(&restored), old_entries);
            } else {
                assert!(i >= submitted);
                assert_eq!(restored.generation(), new);
                assert_eq!(entry_ids(&restored), new_entries);
            }
            tested += 1;
        }
    }
    println!("P02 {label}: {tested} coupures");
    Ok(())
}
fn entry_ids(p: &Pool<SimDevice>) -> Vec<(Uid, Uid, Type, u64)> {
    p.volumes()
        .map(|e| (e.id, e.fs, e.kind, e.guaranteed_blocks / GRAIN))
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
    assert_eq!(reopening.volumes().next().unwrap().kind, Type::Preparing);
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
    let old = p.generation();
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
    let newer = p.generation();
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
    assert_eq!(p.generation(), newer);
    h.power_cut(Persistence::None)?;
    let p = Pool::import(h.snapshot().device())?;
    assert_eq!(p.generation(), old);
    assert!(p.volumes().next().is_none());
    Ok(())
}

#[test]
fn p2_check_pool_json_et_image_identique() -> Result<()> {
    use serde_json::Value;
    use std::{fs, process::Command};

    const PD_BLOCKS: u64 = 5 * GRAIN + 7;
    for sector in [512, 4096] {
        let disk = SimDevice::new(PD_BLOCKS, sector)?;
        let ctl = disk.control();
        let mut p = Pool::format(disk)?;
        p.create(LV, Some(FS), 1)?;
        let mut fs = Engine::format(p.view(LV, Type::Preparing, FS)?, IDS)?;
        fs.create("/preuve")?;
        fs.write("/preuve", 0, &[0x27; 4096])?;
        fs.unmount()?;
        p.mark_ready(LV, FS)?;
        p.create(RAW, None, 1)?;
        p.create(NEW, Some([0xfa; 16]), 1)?;
        let base = ctl.snapshot().bytes()?;
        let marker: Vec<_> = base
            .chunks_exact(4096)
            .enumerate()
            .filter(|(_, b)| **b == [0x27; 4096])
            .map(|(i, _)| i)
            .collect();
        assert_eq!(
            marker.len(),
            1,
            "seul le grain du LV prêt contient le marqueur"
        );
        for case in 0..4 {
            let mut bytes = base.clone();
            if case == 1 {
                bytes[marker[0] * 4096 + 821] ^= 0x40;
            }
            if case == 2 {
                bytes[82] ^= 0x80;
            }
            let file = tempfile::NamedTempFile::new()?;
            fs::write(file.path(), &bytes)?;
            // Le cas 3 présente un descripteur d'écriture seule ; la vraie
            // lecture fera EBADF. Le diagnostic doit être incomplet, code 3.
            let fd = if case == 3 {
                fs::OpenOptions::new().write(true).open(file.path())?
            } else {
                fs::File::open(file.path())?
            };
            let out = Command::new(env!("CARGO_BIN_EXE_novafs"))
                .args(["check-pool", &PD_BLOCKS.to_string(), &sector.to_string()])
                .stdin(fd)
                .output()?;
            assert_eq!(out.status.code(), Some([0, 1, 1, 3][case]), "{:?}", out);
            let report: Value = serde_json::from_slice(&out.stdout).expect("rapport JSON du CLI");
            assert_eq!(report["domaine"], "pool");
            let volumes = report["volumes"].as_array().unwrap();
            if case < 2 {
                assert_eq!(report["paire"]["etat"], "verifiee");
                assert_eq!(volumes.len(), 3);
                assert_eq!(
                    volumes[0]["contenu"]["etat"],
                    if case == 0 { "verifie" } else { "echec" }
                );
                let nested = &volumes[0]["contenu"]["diagnostic"];
                assert_eq!(nested["slots"][0]["vue_verifiee"], true); // G2, avant l’écriture du fichier.
                assert_eq!(nested["slots"][1]["vue_verifiee"], case == 0);
                assert_eq!(
                    nested["paire"]["etat"],
                    if case == 0 {
                        "verifiee"
                    } else {
                        "indisponible"
                    }
                );
                assert_eq!(volumes[1]["contenu"]["etat"], "donnees_sans_checksum_fs");
                assert_eq!(volumes[2]["contenu"]["etat"], "non_publie");
            } else {
                assert_eq!(report["paire"]["etat"], "indisponible");
                assert_eq!(report["slots"][0]["catalogue_verifie"], false);
                assert_eq!(report["slots"][1]["catalogue_verifie"], case == 2);
                assert!(volumes.is_empty());
            }
            assert_eq!(fs::read(file.path())?, bytes);
        }
    }
    Ok(())
}

/// Image de mesure extérieure au processus mesuré. Les 16 LV prêts occupent
/// chacun un grain et contiennent leur propre fichier non vide.
#[test]
#[ignore = "crée un nouveau fichier jetable indiqué par NOVAFS_POOL_FIXTURE"]
fn p2_pool_image_de_mesure() -> Result<()> {
    use std::{fs::OpenOptions, io::Write};
    let path = std::env::var_os("NOVAFS_POOL_FIXTURE").ok_or(Error::Invalid(
        "destination jetable NOVAFS_POOL_FIXTURE requise",
    ))?;
    let disk = SimDevice::new(17 * GRAIN, 4096)?;
    let ctl = disk.control();
    let mut pool = Pool::format(disk)?;
    for i in 1..=16 {
        let ids = Ids {
            lv: [i; 16],
            fs: [i + 16; 16],
        };
        pool.create_for([i + 32; 16], ids.lv, Some(ids.fs), 1)?;
        let view = pool.view(ids.lv, Type::Preparing, ids.fs)?;
        let mut fs = Engine::format(view, ids)?;
        fs.create("/contenu")?;
        fs.write("/contenu", 0, &[i; 4096])?;
        fs.unmount()?;
        pool.mark_ready(ids.lv, ids.fs)?;
    }
    let source = ctl.snapshot().device();
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    for b in 0..source.blocks() {
        file.write_all(&source.read(b)?)?;
    }
    Ok(())
}
