use novafs::{
    Device, Engine, Error, Ids, Mode,
    device::{Event, SimDevice},
};
const IDS: Ids = Ids {
    fs: [21; 16],
    lv: [41; 16],
};

#[test]
fn pas_de_reformatage_repli_ou_reparation_implicite() {
    let mut dev = SimDevice::new(512, 512).unwrap();
    let ctrl = dev.control();
    dev.write(300, &[0x54; 4096]).unwrap();
    dev.flush().unwrap();
    ctrl.trace(None, None);
    assert!(Engine::format(&mut dev, IDS).is_err());
    assert!(ctrl.events().is_empty());
    assert!(Engine::mount(&mut dev, IDS, Mode::ReadOnly).is_err());
    assert_eq!(dev.read(300).unwrap(), [0x54; 4096]);
}
#[test]
fn retour_image_coherente_pendant_vie_du_processus() {
    let dev = SimDevice::new(256, 512).unwrap();
    let c = dev.control();
    let mut e = Engine::format(dev, IDS).unwrap();
    e.create("/a").unwrap();
    let old = c.snapshot().device();
    e.write("/a", 0, b"recu").unwrap();
    for b in 0..old.blocks() {
        c.replace_stable(b, old.read(b).unwrap()).unwrap();
    }
    assert!(matches!(e.verify(), Err(Error::Corrupt(_))));
    assert!(matches!(e.create("/b"), Err(Error::Unavailable)));
    // Une nouvelle instance privée du reçu récent voit une paire cohérente.
    let old_mount = Engine::mount(e.into_device(), IDS, Mode::ReadOnly).unwrap();
    assert!(old_mount.read("/a", 0, 7).unwrap().is_empty());
    old_mount.verify().unwrap();
}
#[test]
fn distinguer_erreur_avant_ancre_et_issue_inconnue() {
    let d = SimDevice::new(256, 512).unwrap();
    let c = d.control();
    let mut e = Engine::format(d, IDS).unwrap();
    let seed = c.snapshot();
    c.trace(None, None);
    e.create("/x").unwrap();
    let events = c.events();
    let slot = e.generation() % 2;
    let first = events
        .iter()
        .position(|e| matches!(e, Event::BeforeWrite(_)))
        .unwrap();
    let anchor = events
        .iter()
        .position(|&e| e == Event::BeforeWrite(slot))
        .unwrap();
    for (idx, publishing) in [(first, false), (anchor, true)] {
        let mut d = seed.device();
        let c = d.control();
        let mut app = Engine::mount(&mut d, IDS, Mode::ReadWrite).unwrap();
        c.trace(Some(idx), None);
        let fail = app.create("/x").unwrap_err();
        if publishing {
            assert!(matches!(fail, Error::Publication { .. }));
            assert!(matches!(app.create("/y"), Err(Error::UnknownOutcome)));
        } else {
            assert!(matches!(fail, Error::Io(_)));
            assert!(matches!(app.create("/y"), Err(Error::Unavailable)));
        }
    }
}

#[test]
fn cache_propre_restitue_et_verification_independante() {
    let dev = SimDevice::new(1024, 4096).unwrap();
    let control = dev.control();
    let mut e = Engine::format(dev, IDS).unwrap();
    e.create("/data").unwrap();
    for group in 0..20u64 {
        e.write("/data", group * 65536, &vec![group as u8 + 1; 65536])
            .unwrap();
    }
    for group in 0..20u64 {
        assert_eq!(
            e.read("/data", group * 65536, 65536).unwrap(),
            vec![group as u8 + 1; 65536]
        );
    }
    let stats = e.cache_stats();
    assert_eq!(stats.pages, 256);
    assert!(stats.evictions > 0);
    assert_eq!(e.reclaim_cache(), 256);
    assert_eq!(e.cache_stats().pages, 0);
    let data = e.read("/data", 0, 4096).unwrap();
    assert_eq!(data, [1; 4096]);
    let hits = e.cache_stats().hits;
    assert_eq!(e.read("/data", 0, 4096).unwrap(), data);
    assert!(e.cache_stats().hits > hits);
    let disk = control.snapshot().device();
    let physical = (2..disk.blocks())
        .find(|&b| disk.read(b).unwrap() == [1; 4096])
        .unwrap();
    control.replace_stable(physical, [0; 4096]).unwrap();
    // Les octets validés de la vue en cache restent intacts. L'audit relit le support.
    assert_eq!(e.read("/data", 0, 4096).unwrap(), data);
    assert!(matches!(e.verify(), Err(Error::Corrupt(_))));
    assert_eq!(e.cache_stats().pages, 0);
    assert!(matches!(e.write("/data", 0, b"!"), Err(Error::Unavailable)));
}
