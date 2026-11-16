//! Relire la décision autorisée sans ouvrir les deux supports en écriture.
use novafs::{Engine, Ids, Mode, device::FileImage};
use novaos_identity::{Authority, Registry, lab::Ledger};
use std::{fs::File, path::Path};

fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    assert_eq!(args.len(), 2, "répertoire secteur");
    let root = Path::new(&args[0]);
    let sector = args[1].parse().unwrap();
    let image = |name: &str, pages| {
        FileImage::from_file(File::open(root.join(name)).unwrap(), pages, sector).unwrap()
    };
    let ids = Ids {
        fs: [8; 16],
        lv: [9; 16],
    };
    let mut fs = Engine::mount_current(image("identites.p9", 512), ids, Mode::ReadOnly).unwrap();
    fs.verify().unwrap();
    let mut ledger = Ledger::new(image("autorite.raw", 12));
    let head = ledger.current().unwrap();
    let registry = Registry::open(&mut fs, &mut ledger, [7; 16]).unwrap();
    assert_eq!(registry.outcome(head.request).unwrap(), Some(head));
    println!(
        "P9/{sector} intègre, état autorisé relu, génération={}",
        head.generation
    );
}
