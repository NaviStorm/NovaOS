//! Lecteur indépendant du client invité. Ouvre l'image existante en lecture seule.
use novafs::{Device, Engine, Mode, codec::Anchor, device::FileImage};
use std::{env, error::Error, fs::File};

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = env::args().collect();
    let sector = args[2].parse()?;
    let file = File::open(&args[1])?;
    let len = file.metadata()?.len();
    assert_eq!(len % 4096, 0);
    let disk = FileImage::from_file(file, len / 4096, sector)?;
    let ids = Anchor::decode(&disk.read(0)?, 0)?.ids;
    let fs = Engine::mount(disk, ids, Mode::ReadOnly)?;
    fs.verify()?;
    let mut parent = fs.root_id();
    let start = if args[3] == "ancien" {
        "a".to_owned()
    } else {
        "z".repeat(255)
    };
    for name in [start, "b".repeat(250), "c".repeat(250), "d".repeat(250)] {
        parent = fs.lookup_at(parent, &name)?.id;
    }
    let id = fs.lookup_at(parent, &"e".repeat(64))?.id;
    assert_eq!(fs.read_id(id, 0, 5)?, b"Nova");
    println!("{} {id}", fs.generation());
    Ok(())
}
