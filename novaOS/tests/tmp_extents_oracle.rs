//! Oracle indépendant. Les instantanés P9 restent ouverts en lecture seule.
use novafs::{
    Device, Engine, Mode, View,
    codec::Anchor,
    device::FileImage,
    tree::{self, Family},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    env,
    error::Error,
    fs::File,
};

fn mount(path: &str, sector: u32) -> Result<Engine<FileImage>, Box<dyn Error>> {
    let f = File::open(path)?;
    let blocks = f.metadata()?.len() / 4096;
    let disk = FileImage::from_file(f, blocks, sector)?;
    let ids = Anchor::decode(&disk.read(0)?, 0)?.ids;
    let fs = Engine::mount_current(disk, ids, Mode::ReadOnly)?;
    fs.verify()?;
    Ok(fs)
}
fn integer(bytes: &[u8]) -> u64 {
    u64::from_le_bytes(bytes.try_into().unwrap())
}
fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = env::args().collect();
    let sec = args[1].parse()?;
    let before = mount(&args[2], sec)?;
    let mut files = BTreeSet::new();
    let root = before.lookup_at(before.root_id(), "\x1fNovaOS-tmp-v1")?.id;
    for n in before.inspect_dir(root, None, 64)? {
        if n.starts_with('t') {
            let dir = before.lookup_at(root, &n)?.id;
            if let Ok(file) = before.lookup_at(dir, "commun") {
                if file.size == 32768 {
                    let bytes = before.inspect_id(file.id, 0, 32768)?;
                    assert!(
                        bytes
                            .iter()
                            .enumerate()
                            .all(|(i, b)| *b == (i as u8 ^ 0xa5))
                    );
                    files.insert(file.id);
                }
            }
        }
    }
    assert_eq!(files.len(), 1);
    let file = *files.first().unwrap();
    let mut physical = BTreeSet::new();
    let f = Family::Extents;
    tree::walk(
        &before,
        before.manifest().roots[f.index()],
        f,
        |_| Ok(()),
        |k, v| {
            if integer(&k[..8]) == file {
                let addr = integer(&v[..8]);
                let n = u16::from_le_bytes(v[8..10].try_into().unwrap());
                physical.extend(addr..addr + u64::from(n));
            }
            Ok(())
        },
    )?;
    assert_eq!(physical.len(), 8);
    println!("objet={file} LBA={physical:?}");
    let mut drainage = None;
    let mut restitues = BTreeSet::new();
    for path in &args[3..] {
        let fs = mount(path, sec)?;
        assert_eq!(before.ids().fs, fs.ids().fs);
        assert!(matches!(fs.stat_id(file), Err(novafs::Error::NotFound)));
        let limite = *drainage.get_or_insert(fs.generation());
        let mut ownership: BTreeMap<u64, String> = BTreeMap::new();
        for f in [
            Family::Space,
            Family::Names,
            Family::Objects,
            Family::Extents,
            Family::Garbage,
        ] {
            // Conserver séparément les visites de noeuds et d'entrées.
            let mut nodes = Vec::new();
            tree::walk(
                &fs,
                fs.manifest().roots[f.index()],
                f,
                |r| {
                    nodes.push(r.block);
                    Ok(())
                },
                |k, v| {
                    if f == Family::Space {
                        let addr = integer(k);
                        for &lba in physical.range(addr..addr + integer(&v[..8])) {
                            let age = integer(&v[9..]);
                            let etat = if age < fs.manifest().previous.max(1) {
                                restitues.insert(lba);
                                "libre"
                            } else if age > limite && age == fs.manifest().previous {
                                // Le validateur P9 exige que ce LBA daté du prédécesseur
                                // y soit possédé. Le fichier initial était déjà collecté.
                                restitues.insert(lba);
                                "realloue_puis_retire"
                            } else {
                                "protege_par_generation"
                            };
                            ownership.insert(lba, format!("{etat}(age={age})"));
                        }
                    } else if f == Family::Extents {
                        let id = integer(&k[..8]);
                        assert_ne!(id, file);
                        let addr = integer(&v[..8]);
                        let n = u16::from_le_bytes(v[8..10].try_into().unwrap());
                        for &lba in physical.range(addr..addr + u64::from(n)) {
                            ownership.insert(lba, format!("reaffecte(objet={id})"));
                        }
                    }
                    Ok(())
                },
            )?;
            for lba in nodes.into_iter().filter(|n| physical.contains(n)) {
                ownership.insert(lba, format!("reaffecte(noeud={f:?})"));
            }
        }
        println!("generation={} etat={ownership:?}", fs.generation());
        assert_eq!(ownership.len(), 8);
        restitues.extend(
            ownership
                .iter()
                .filter(|(_, v)| v.starts_with("reaffecte"))
                .map(|(lba, _)| *lba),
        );
        if path == args.last().unwrap() {
            assert_eq!(restitues, physical);
        }
    }
    Ok(())
}
