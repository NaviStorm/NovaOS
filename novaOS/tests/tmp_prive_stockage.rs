//! Exercices du code du service sur des images P9. Le backend simulé rend
//! observables les frontières des commandes et des barrières de persistance.
extern crate alloc;
#[path = "../user/novafs/files/tmp_store.rs"]
mod tmp_store;
use novafs::{
    Device, Engine, Error, Ids, Kind, Mode, Result, VolumePolicy,
    device::{Event, FileImage, Persistence, SimDevice, SimFault, SimSnapshot},
    time::Atime,
};
use tmp_store::{ROOT_NAME, Store};

const IDS: Ids = Ids {
    fs: [3; 16],
    lv: [4; 16],
};
const FIRST: [u8; 16] = [0x11; 16];
const NEXT: [u8; 16] = [0x22; 16];

fn owner(boot: [u8; 16], nr: u64) -> [u8; 32] {
    let mut out = [0; 32];
    out[..16].copy_from_slice(&boot);
    out[16..24].copy_from_slice(&((1u64 << 63) + nr).to_le_bytes());
    out[24..].copy_from_slice(&1u64.to_le_bytes());
    out
}

fn base<D: Device>(dev: D) -> Result<Engine<D>> {
    let mut fs = Engine::format_p9(dev, IDS, Atime::No, None)?;
    fs.create("/exterieur")?;
    fs.write("/exterieur", 0, b"donnee-exterieure")?;
    Ok(fs)
}
fn outside<D: Device>(fs: &Engine<D>) -> Result<()> {
    assert_eq!(
        fs.inspect_id(fs.stat("/exterieur")?.id, 0, 32)?,
        b"donnee-exterieure"
    );
    fs.verify()
}

fn real(sector: u32) -> Result<()> {
    let mut fs = base(FileImage::create(256, sector)?)?;
    let store = Store::mount(&mut fs)?;
    let a = owner(FIRST, 1);
    let b = owner(FIRST, 2);
    let a_dir = store.create(&mut fs, a)?;
    let b_dir = store.create(&mut fs, b)?;
    assert_ne!(a_dir, b_dir);
    assert!(
        fs.inspect_dir(a_dir, None, 2)?.is_empty() && fs.inspect_dir(b_dir, None, 2)?.is_empty()
    );
    let inner = fs.create_at(a_dir, "niveau", Kind::Directory)?;
    let file = fs.create_at(inner.id, "lien", Kind::File)?;
    fs.rename_at(inner.id, "lien", inner.id, "cible")?;
    assert!(matches!(
        fs.lookup_at(inner.id, "lien"),
        Err(Error::NotFound)
    ));
    assert_eq!(fs.lookup_at(inner.id, "cible")?.id, file.id);
    // P9 ne possède ni lien symbolique ni lien dur. Une cible écrite dans
    // un fichier ordinaire reste une donnée. La suppression ne la résout pas.
    fs.prepare_write_id(file.id, 0, b"/exterieur")?.commit()?;
    assert!(matches!(
        fs.lookup_at(b_dir, "niveau/cible"),
        Err(Error::NotFound)
    ));
    assert!(matches!(
        fs.lookup_at(a_dir, "../exterieur"),
        Err(Error::Invalid(_))
    ));
    assert!(matches!(
        fs.lookup_at(a_dir, "/exterieur"),
        Err(Error::Invalid(_))
    ));
    assert!(matches!(store.create(&mut fs, a), Err(Error::Exists)));
    assert!(store.purge(&mut fs, a, b_dir).is_err());
    assert!(matches!(
        store.create(&mut fs, [0; 32]),
        Err(Error::Invalid(_))
    ));
    outside(&fs)?;
    // Perte du service dans la même époque : les deux dossiers sont isolés.
    let recovered = store.recover(&mut fs, FIRST)?;
    assert_eq!((recovered.removed, recovered.quarantined), (0, 2));
    assert_eq!(fs.inspect_id(file.id, 0, 40)?, b"/exterieur");
    store.purge(&mut fs, a, a_dir)?;
    store.purge(&mut fs, a, a_dir)?;
    assert!(matches!(fs.stat_id(file.id), Err(Error::NotFound)));
    assert_eq!(fs.stat_id(b_dir)?.kind, Kind::Directory);
    let mut fs = Engine::mount_current(fs.unmount()?, IDS, Mode::ReadWrite)?;
    let recovered = Store::mount(&mut fs)?.recover(&mut fs, NEXT)?;
    assert_eq!((recovered.removed, recovered.quarantined), (1, 0));
    assert!(!fs.collection_pending()?);
    let fresh = Store::mount(&mut fs)?.create(&mut fs, owner(NEXT, 2))?;
    assert_ne!(fresh, b_dir);
    assert!(fs.inspect_dir(fresh, None, 1)?.is_empty());
    outside(&fs)?;
    let mut fs = Engine::mount_current(fs.unmount()?, IDS, Mode::ReadOnly)?;
    assert!(matches!(
        Store::mount(&mut fs)?.create(&mut fs, owner(NEXT, 3)),
        Err(Error::ReadOnly)
    ));
    outside(&fs)?;
    if let Some(target) = std::env::var_os("NOVAOS_TMP_PROOFS") {
        use std::{
            fs::{File, OpenOptions},
            io::{Seek, copy},
            path::PathBuf,
        };
        let path =
            PathBuf::from(target).join(format!("image-{}-{sector}.raw", std::env::consts::ARCH));
        let mut source = fs.unmount()?.into_file();
        source.rewind()?;
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        assert_eq!(copy(&mut source, &mut file)?, source.metadata()?.len());
        file.sync_all()?;
        drop(file);
        let verify = FileImage::from_file(File::open(&path)?, 256, sector)?;
        let read = Engine::mount_current(verify, IDS, Mode::ReadOnly)?;
        outside(&read)?;
        assert!(read.inspect_dir(fresh, None, 1)?.is_empty());
        assert!(matches!(read.stat_id(b_dir), Err(Error::NotFound)));
        println!(
            "image_archivee={} remontee_independamment=OK",
            path.file_name().unwrap().to_string_lossy()
        );
    }
    println!(
        "secteur={sector} image_fichier=OK collision=OK époque_rejouée=quarantaine traversal=refusé GC=vide"
    );
    Ok(())
}

fn quarantine(sector: u32) -> Result<()> {
    let mut fs = base(SimDevice::new(384, sector)?)?;
    fs.mkdir(&format!("/{ROOT_NAME}"))?;
    let foreign = fs.create_at(
        fs.stat(&format!("/{ROOT_NAME}"))?.id,
        "etranger",
        Kind::File,
    )?;
    fs.prepare_write_id(foreign.id, 0, b"etranger")?.commit()?;
    let before = fs.receipt();
    assert!(Store::mount(&mut fs).is_err());
    assert_eq!(fs.receipt(), before);
    assert_eq!(fs.inspect_id(foreign.id, 0, 30)?, b"etranger");
    outside(&fs)?;

    let mut fs = base(SimDevice::new(384, sector)?)?;
    let s = Store::mount(&mut fs)?;
    let oa = owner(FIRST, 1);
    let ob = owner(FIRST, 2);
    let a = s.create(&mut fs, oa)?;
    let b = s.create(&mut fs, ob)?;
    let sentinel = fs.create_at(b, "temoin", Kind::File)?;
    fs.prepare_write_id(sentinel.id, 0, b"B")?.commit()?;
    let manager = fs.lookup_at(fs.root_id(), ROOT_NAME)?.id;
    let names = fs.inspect_dir(manager, None, 16)?;
    let name_for = |p: u8, id: &[u8; 32]| {
        format!(
            "{}{suffix}",
            char::from(p),
            suffix = id.iter().map(|b| format!("{b:02x}")).collect::<String>()
        )
    };
    let name_a = name_for(b'r', &oa);
    let name_b = name_for(b'r', &ob);
    assert!(names.contains(&name_a) && names.contains(&name_b));
    let fa = fs.lookup_at(manager, &name_a)?;
    let fb = fs.lookup_at(manager, &name_b)?;
    let bytes = fs.inspect_id(fa.id, 0, 104)?;
    fs.prepare_write_id(fb.id, 0, &bytes)?.commit()?; // substitution entre propriétaires
    let unknown = fs.create_at(manager, "objet-inconnu", Kind::File)?;
    let report = s.recover(&mut fs, NEXT)?;
    assert_eq!((report.removed, report.quarantined), (1, 2));
    assert!(matches!(fs.stat_id(a), Err(Error::NotFound)));
    assert!(fs.stat_id(unknown.id).is_ok());
    assert_eq!(fs.inspect_id(sentinel.id, 0, 1)?, b"B");
    assert!(matches!(s.create(&mut fs, ob), Err(Error::Exists)));
    let before = fs.receipt();
    assert!(matches!(
        s.recover(&mut fs, [0; 16]),
        Err(Error::Unavailable)
    ));
    assert_eq!(fs.receipt(), before);
    outside(&fs)?;
    println!(
        "secteur={sector} volume_préoccupé=préservé substitution_propriétaire=quarantaine refus_sans_bootid=OK"
    );
    Ok(())
}

fn full_and_io(sector: u32) -> Result<()> {
    let dev = SimDevice::new(256, sector)?;
    let ctl = dev.control();
    let mut fs = base(dev)?;
    let s = Store::mount(&mut fs)?;
    fs.create("/remplissage")?;
    let active = s.create(&mut fs, owner(FIRST, 3))?;
    let leaf = fs.create_at(active, "f", Kind::File)?;
    fs.prepare_write_id(leaf.id, 0, b"A")?.commit()?;
    // Amener l'espace disponible pour les transactions ordinaires à zéro en
    // conservant tous les blocs libres pour la maintenance P9.
    let stats = fs.accounting()?;
    fs.set_volume_policy(VolumePolicy {
        maintenance_blocks: stats.available_blocks + stats.maintenance_reserved,
        logical_quota_bytes: None,
    })?;
    assert_eq!(fs.accounting()?.available_blocks, 0);
    assert!(matches!(
        s.create(&mut fs, owner(FIRST, 4)),
        Err(Error::NoSpace)
    ));
    assert_eq!(fs.inspect_id(leaf.id, 0, 1)?, b"A");
    fs.set_volume_policy(VolumePolicy::default())?;
    // Avec des blocs disponibles, saturer aussi le quota logique à l'écriture du registre.
    fs.set_volume_policy(VolumePolicy {
        maintenance_blocks: 64,
        logical_quota_bytes: Some(fs.accounting()?.logical_bytes as u64),
    })?;
    let before = fs.receipt();
    assert!(matches!(
        s.create(&mut fs, owner(FIRST, 5)),
        Err(Error::QuotaExceeded)
    ));
    assert_eq!(fs.receipt(), before);
    fs.set_volume_policy(VolumePolicy::default())?;
    ctl.inject(SimFault::Flush {
        ordinal: 1,
        persist_newest: 0,
    })?;
    assert!(s.purge(&mut fs, owner(FIRST, 3), active).is_err());
    drop(fs);
    ctl.power_cut(Persistence::None)?;
    let mut fs = Engine::mount_current(ctl.snapshot().device(), IDS, Mode::ReadWrite)?;
    let s = Store::mount(&mut fs)?;
    let recovered = s.recover(&mut fs, NEXT)?;
    assert_eq!((recovered.removed, recovered.quarantined), (1, 0));
    assert!(matches!(fs.stat_id(leaf.id), Err(Error::NotFound)));
    outside(&fs)?;
    println!(
        "secteur={sector} espace_disponible=0 quota_plein=OK erreur_flush=propagée reprise=OK"
    );
    Ok(())
}

fn bounded_quarantine(sector: u32) -> Result<()> {
    let dev = SimDevice::new(512, sector)?;
    let control = dev.control();
    let fs = base(dev)?;
    drop(fs);
    let mut current = control.snapshot();
    // Interrompre deux démarrages successifs exactement à la publication
    // durable du répertoire préparatoire, avant la création de son sceau.
    for _ in 0..2 {
        let dev = current.device();
        let ctl = dev.control();
        let mut trial = Engine::mount_current(dev, IDS, Mode::ReadWrite)?;
        ctl.trace(None, None);
        Store::mount(&mut trial)?;
        let after_mkdir = ctl
            .events()
            .iter()
            .position(|e| *e == Event::FlushCompleted(2))
            .unwrap();
        let dev = current.device();
        let ctl = dev.control();
        let mut interrupted = Engine::mount_current(dev, IDS, Mode::ReadWrite)?;
        ctl.trace(Some(after_mkdir), None);
        assert!(Store::mount(&mut interrupted).is_err());
        drop(interrupted);
        ctl.power_cut(Persistence::None)?;
        current = ctl.snapshot();
    }
    let mut fs = Engine::mount_current(current.device(), IDS, Mode::ReadWrite)?;
    let stages = fs.inspect_dir(fs.root_id(), None, 8)?;
    assert_eq!(
        stages
            .iter()
            .filter(|n| tmp_store::hidden_root_name(n))
            .count(),
        2
    );
    let before = fs.receipt();
    for _ in 0..3 {
        assert!(matches!(Store::mount(&mut fs), Err(Error::Limit(_))));
    }
    assert_eq!(fs.receipt(), before);
    assert_eq!(fs.inspect_dir(fs.root_id(), None, 8)?, stages);
    outside(&fs)?;

    let mut fs = base(SimDevice::new(512, sector)?)?;
    let store = Store::mount(&mut fs)?;
    let manager = fs.lookup_at(fs.root_id(), ROOT_NAME)?.id;
    // Une coupure entre CREATE du registre et sa première écriture publiée
    // laisse exactement ce fichier vide. Les injections de cuts(Create)
    // exercent ce chemin réel ; remplir ici le registre de ces restes.
    for n in 1..=63 {
        let id = owner(FIRST, n);
        let encoded: String = id.iter().map(|b| format!("{b:02x}")).collect();
        fs.create_at(manager, &format!("r{encoded}"), Kind::File)?;
    }
    let before = fs.receipt();
    let a = store.recover(&mut fs, NEXT)?;
    assert_eq!((a.quarantined, a.removed), (63, 0));
    for n in 1..=3 {
        assert!(matches!(
            store.create(&mut fs, owner(NEXT, n)),
            Err(Error::Limit(_))
        ));
    }
    assert_eq!(fs.receipt(), before);
    for name in ["inconnu-a", "inconnu-b"] {
        fs.create_at(manager, name, Kind::File)?;
    }
    let before = fs.receipt();
    assert!(matches!(store.recover(&mut fs, NEXT), Err(Error::Limit(_))));
    assert_eq!(fs.receipt(), before);
    outside(&fs)?;
    println!(
        "secteur={sector} préparations_interrompues=2 registres_incomplets=63 refus_sans_mutation=OK registre_importé_surdimensionné=refusé"
    );
    Ok(())
}

#[derive(Debug, Clone, Copy)]
enum Step {
    Init,
    Create,
    Purge,
}
fn act(fs: &mut Engine<SimDevice>, step: Step, target: u64) -> Result<()> {
    let store = Store::mount(fs)?;
    match step {
        Step::Init => Ok(()),
        Step::Create => store.create(fs, owner(FIRST, 1)).map(|_| ()),
        Step::Purge => store.purge(fs, owner(FIRST, 1), target),
    }
}
fn cuts(sector: u32, step: Step) -> Result<()> {
    let dev = SimDevice::new(256, sector)?;
    let ctl = dev.control();
    let mut fs = base(dev)?;
    let mut target = 0;
    if !matches!(step, Step::Init) {
        let s = Store::mount(&mut fs)?;
        if matches!(step, Step::Purge) {
            target = s.create(&mut fs, owner(FIRST, 1))?;
            let dir = fs.create_at(target, "niveau", Kind::Directory)?;
            let f = fs.create_at(dir.id, "lien", Kind::File)?;
            fs.prepare_write_id(f.id, 0, b"/exterieur")?.commit()?;
        }
    }
    let start: SimSnapshot = ctl.snapshot();
    ctl.trace(None, None);
    act(&mut fs, step, target)?;
    let events = ctl.events();
    let mut count = 0;
    for (i, e) in events.iter().enumerate() {
        if !matches!(
            e,
            Event::FlushSubmitted(_) | Event::FlushEffect(_) | Event::FlushCompleted(_)
        ) {
            continue;
        }
        let dev = start.device();
        let ctl = dev.control();
        let mut fs = Engine::mount_current(dev, IDS, Mode::ReadWrite)?;
        ctl.trace(Some(i), None);
        assert!(
            act(&mut fs, step, target).is_err(),
            "injection {i} attendue"
        );
        drop(fs);
        ctl.power_cut(Persistence::None)?;
        let mut fs = Engine::mount_current(ctl.snapshot().device(), IDS, Mode::ReadWrite)?;
        let store = Store::mount(&mut fs)?;
        let result = store.recover(&mut fs, NEXT)?;
        if matches!(step, Step::Create) {
            assert!(result.quarantined <= 1); // seul le registre vide peut rester isolé
        } else {
            assert_eq!(result.quarantined, 0);
        }
        if matches!(step, Step::Purge) {
            assert!(matches!(fs.stat_id(target), Err(Error::NotFound)));
        }
        let again = store.recover(&mut fs, NEXT)?;
        assert_eq!((again.removed, again.quarantined), (0, result.quarantined));
        let root = store.create(&mut fs, owner(NEXT, 1))?;
        assert!(fs.inspect_dir(root, None, 1)?.is_empty());
        outside(&fs)?;
        count += 1;
    }
    assert!(count >= 6);
    println!("secteur={sector} opération={step:?} injections_F1_F2={count} reprise+sentinelle=OK");
    Ok(())
}
fn main() -> Result<()> {
    println!("cible={} Darwin, NovaFS v0.9.13 P9", std::env::consts::ARCH);
    assert!(tmp_store::hidden_root_name(ROOT_NAME));
    assert!(tmp_store::hidden_root_name(
        "\x1fNovaOS-tmp-v1.new-0000000000000010"
    ));
    assert!(!tmp_store::hidden_root_name("tmp"));
    for sector in [512, 4096] {
        real(sector)?;
        quarantine(sector)?;
        full_and_io(sector)?;
        bounded_quarantine(sector)?;
        for step in [Step::Init, Step::Create, Step::Purge] {
            cuts(sector, step)?;
        }
    }
    Ok(())
}
