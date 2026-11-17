//! Provisionneur hors ligne de la recette. Exclusivement sur une copie P9/autorité.
use novafs::{Engine, Ids, Kind, Mode, device::FileImage};
use novaos_identity::{
    Authority, Registry, created_user,
    distribution::{Action, Machine, public_key},
    lab::Ledger,
    policy::{Origin, READ},
};
use std::{
    env,
    fs::{self, OpenOptions},
    path::Path,
};
fn inspect(root: &Path, sector: u32, group: &str) {
    let file = std::fs::File::open(root.join("identites.p9")).unwrap();
    let dev = FileImage::from_file(file, 512, sector).unwrap();
    let fs = Engine::mount_current(
        dev,
        Ids {
            fs: [8; 16],
            lv: [9; 16],
        },
        Mode::ReadOnly,
    )
    .unwrap();
    let tsv = b"A\tB\tC\nD\tE\tF\n";
    let out = b"A\tC\nD\tF\n";
    let mut files: Vec<(&str, &[u8])> = vec![("donnees.tsv", tsv)];
    match group {
        "redirect" => files.extend([("sortie", &out[..]), ("erreurs", &[][..])]),
        "redirpipe" => files.extend([
            ("paire", &out[..]),
            ("err-paire", &[][..]),
            ("diagnostic", "cut: accès refusé\n".as_bytes()),
        ]),
        "droits" => files.extend([("course", &b"unique"[..]), ("revoque", &b"avant"[..])]),
        "binaire" => files.extend([("octets", &b"\0\0\x03\0a b\x02\0-x"[..])]),
        "syntaxe" => assert!(matches!(
            fs.stat("/travail/interdit"),
            Err(novafs::Error::NotFound)
        )),
        _ => (),
    }
    let directory = fs.lookup_at(fs.root_id(), "travail").unwrap();
    for (name, expected) in files {
        let stat = fs.lookup_at(directory.id, name).unwrap();
        assert_eq!(stat.kind, Kind::File);
        assert_eq!(stat.size, expected.len() as u64, "{name}");
        assert_eq!(
            fs.inspect_id(stat.id, 0, expected.len()).unwrap(),
            expected,
            "{name}"
        );
        println!(
            "P9 relu /travail/{name} {} octets {:02x?}",
            expected.len(),
            expected
        );
    }
    assert!(matches!(
        fs.stat("/system/interdit"),
        Err(novafs::Error::NotFound)
    ));
}
fn main() {
    let argv: Vec<_> = env::args().collect();
    let root = Path::new(&argv[1]);
    let sector = argv[2].parse().unwrap();
    if argv[3] == "--inspect" {
        return inspect(root, sector, &argv[4]);
    }
    let machine = match argv[3].as_str() {
        "aarch64" => Machine::Aarch64,
        "x86_64" => Machine::X86_64,
        _ => panic!("architecture"),
    };
    let image = |name, n| {
        FileImage::from_file(
            OpenOptions::new()
                .read(true)
                .write(true)
                .open(root.join(name))
                .unwrap(),
            n,
            sector,
        )
        .unwrap()
    };
    let mut ledger = Ledger::new(image("autorite.raw", 12));
    let mut fs = Engine::mount_current(
        image("identites.p9", 512),
        Ids {
            fs: [8; 16],
            lv: [9; 16],
        },
        Mode::ReadWrite,
    )
    .unwrap();
    if argv.get(4).is_some_and(|op| op == "--pipe") {
        use novaos_identity::policy::{InstallChange, installed};
        use sha2::{Digest, Sha256};
        let executable = fs::read(&argv[5]).unwrap();
        let digest: [u8; 32] = Sha256::digest(&executable).into();
        let root_id = fs.root_id();
        let directory = fs
            .create_at(root_id, "travail", Kind::Directory)
            .unwrap()
            .id;
        let data = fs.create_at(directory, "donnees.tsv", Kind::File).unwrap();
        fs.prepare_write_id(data.id, 0, b"A\tB\tC\nD\tE\tF\n")
            .unwrap()
            .commit()
            .unwrap();
        let head = ledger.current().unwrap();
        let reg = Registry::open(&mut fs, &mut ledger, [7; 16]).unwrap();
        let owner = created_user(&reg.outcome([1; 16]).unwrap().unwrap());
        drop(reg);
        let mut seal = Vec::from(*b"NXScope1");
        seal.extend_from_slice(&owner);
        seal.extend_from_slice(&directory.to_le_bytes());
        seal.extend_from_slice(&data.id.to_le_bytes());
        let hidden = fs.create_at(root_id, "\u{1f}NXScope", Kind::File).unwrap();
        fs.prepare_write_id(hidden.id, 0, &seal)
            .unwrap()
            .commit()
            .unwrap();
        let mut reg = Registry::open(&mut fs, &mut ledger, [7; 16]).unwrap();
        let head = reg
            .bind_folder(head, owner, directory, &seal, [159; 16])
            .unwrap();
        let registered = reg
            .provision_install(head, [160; 16], owner, InstallChange::New(digest))
            .unwrap();
        let installation = installed(&registered);
        reg.set_permission(
            reg.grant_attested(owner).unwrap(),
            owner,
            directory,
            installation,
            Origin::Direct,
            READ,
            [161; 16],
        )
        .unwrap();
        drop(reg);
        let mut marker = Vec::from(installation);
        marker.extend_from_slice(&digest);
        let object = fs
            .create_at(root_id, "\u{1f}NXPipeImages", Kind::File)
            .unwrap();
        fs.prepare_write_id(object.id, 0, &marker)
            .unwrap()
            .commit()
            .unwrap();
        let file = fs.create_at(directory, "lignes.tsv", Kind::File).unwrap();
        let mut text = Vec::from(*b"A\tB\tC\n");
        text.resize(65535, b'x');
        text.push(b'\n');
        fs.prepare_write_id(file.id, 0, &text)
            .unwrap()
            .commit()
            .unwrap();
        fs.unmount().unwrap();
        println!("installation={installation:02x?} sha256={digest:02x?}");
        return;
    }
    let root_id = fs.root_id();
    let directory = fs
        .create_at(root_id, "travail", Kind::Directory)
        .unwrap()
        .id;
    let file = fs
        .create_at(directory, "donnees.tsv", Kind::File)
        .unwrap()
        .id;
    fs.prepare_write_id(file, 0, b"A\tB\tC\nD\tE\tF\n")
        .unwrap()
        .commit()
        .unwrap();
    let system = match fs.lookup_at(root_id, "system") {
        Ok(s) => s,
        Err(novafs::Error::NotFound) => fs.create_at(root_id, "system", Kind::Directory).unwrap(),
        Err(e) => panic!("{e:?}"),
    };
    if let Err(novafs::Error::NotFound) = fs.lookup_at(system.id, "bin") {
        fs.create_at(system.id, "bin", Kind::Directory).unwrap();
    }
    let script = fs
        .create_at(directory, "arguments.nsh", Kind::File)
        .unwrap()
        .id;
    fs.prepare_write_id(script, 0, b"#!/bin/nsh\ntemoin '' 'a b' -x\n")
        .unwrap()
        .commit()
        .unwrap();
    let hidden = fs
        .create_at(root_id, "\u{1f}NXScope", Kind::File)
        .unwrap()
        .id;
    let mut reg = Registry::open(&mut fs, &mut ledger, [7; 16]).unwrap();
    let owner = created_user(&reg.outcome([1; 16]).unwrap().unwrap());
    let key = public_key(&fs::read(&argv[4]).unwrap().try_into().unwrap()).unwrap();
    let cat = fs::read(&argv[5]).unwrap();
    reg.publish_package(
        reg.grant_attested(owner).unwrap(),
        [151; 16],
        Action::Install,
        &key,
        machine,
        &cat,
    )
    .unwrap();
    let mut seal = Vec::from(*b"NXScope1");
    seal.extend_from_slice(&owner);
    seal.extend_from_slice(&directory.to_le_bytes());
    seal.extend_from_slice(&file.to_le_bytes());
    drop(reg);
    fs.prepare_write_id(hidden, 0, &seal)
        .unwrap()
        .commit()
        .unwrap();
    let head = ledger.current().unwrap();
    let mut reg = Registry::open(&mut fs, &mut ledger, [7; 16]).unwrap();
    reg.bind_folder(head, owner, directory, &seal, [152; 16])
        .unwrap();
    for (id, name) in [(153, "cut"), (154, "cat")] {
        let grant = reg.grant_attested(owner).unwrap();
        let app = reg
            .load_package(grant, &key, machine, name)
            .unwrap()
            .principal
            .unwrap();
        reg.set_permission(grant, owner, directory, app, Origin::Direct, READ, [id; 16])
            .unwrap();
    }
    drop(reg);
    fs.unmount().unwrap();
    println!("dossier={directory} fichier={file} lecture directe cat/cut, témoin absent");
}
