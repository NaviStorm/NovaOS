//! N06. Vérifier la génération, les crédits et le contenu après chaque refus.
use novafs::{Engine, Error, Ids, Mode, SpaceStats, device::SimDevice};
const IDS: Ids = Ids {
    fs: [7; 16],
    lv: [8; 16],
};
fn counts(s: SpaceStats) -> (u64, u64, u64, usize) {
    (s.free, s.deferred, s.largest_free, s.intervals)
}

#[test]
fn n06_staging_credits_are_returned_at_every_failure() {
    let dev = SimDevice::new(512, 4096).unwrap();
    let control = dev.control();
    let mut fs = Engine::format(dev, IDS).unwrap();
    fs.create("/victime").unwrap();
    fs.write("/victime", 0, b"temoin").unwrap();
    let original = fs.receipt();
    let before = counts(fs.space_stats());
    let replacement = [0x47; 65536];
    let mut success = None;
    for pages in 0..=64 {
        fs.set_staging_budget(pages * 4096);
        control.trace(None, None);
        match fs.write("/victime", 1, &replacement) {
            Err(Error::NoMemory) => {
                assert_eq!(fs.receipt(), original);
                assert_eq!(counts(fs.space_stats()), before);
                assert_eq!(fs.read("/victime", 0, 32).unwrap(), b"temoin");
                assert!(control.events().is_empty());
                fs.verify().unwrap();
            }
            Ok(()) => {
                success = Some(pages);
                break;
            }
            Err(e) => panic!("crédit {pages}, erreur inattendue {e}"),
        }
    }
    let pages = success.expect("la transaction tient dans le plafond P0");
    assert!(pages > 17);
    assert_eq!(fs.read("/victime", 1, 65536).unwrap(), replacement);
    assert_eq!(fs.generation(), original.birth + 1);
    println!(
        "N06 staging crédits refusés=0..{} premier succès={pages} pages",
        pages - 1
    );
}

#[test]
fn n06_full_device_keeps_the_emergency_reserve_and_recovers_after_collection() {
    let dev = SimDevice::new(224, 512).unwrap();
    let control = dev.control();
    let mut fs = Engine::format(dev, IDS).unwrap();
    fs.create("/plein").unwrap();
    let mut pages = 0u64;
    loop {
        let before = counts(fs.space_stats());
        let receipt = fs.receipt();
        control.trace(None, None);
        match fs.write("/plein", pages * 4096, &[0xd3; 4096]) {
            Ok(()) => {
                pages += 1;
                assert!(pages < 224);
            }
            Err(Error::NoSpace) => {
                assert_eq!(before, counts(fs.space_stats()));
                assert_eq!(receipt, fs.receipt());
                assert!(control.events().is_empty());
                break;
            }
            Err(e) => panic!("{e}"),
        }
    }
    assert!(pages > 0 && fs.space_stats().free >= 128);
    let file = fs.stat("/plein").unwrap();
    assert_eq!(file.size, pages * 4096);
    assert_eq!(
        fs.read("/plein", (pages - 1) * 4096, 4096).unwrap(),
        [0xd3; 4096]
    );
    let mut fs = Engine::mount(fs.unmount().unwrap(), IDS, Mode::ReadWrite).unwrap();
    fs.unlink("/plein").unwrap();
    let mut batches = 0;
    while fs.collect().unwrap() {
        batches += 1;
    }
    assert!(batches > 0);
    fs.create("/reprise").unwrap();
    fs.write("/reprise", 0, &[0xea; 65536]).unwrap();
    fs.verify().unwrap();
    println!(
        "N06 224 blocs, {pages} pages utilisateur avant PLEIN, {batches} lots de collecte, reprise 64 Kio"
    );
}
