//! Oracle indépendant, en lecture seule, du volume jetable après arrêt de QEMU.
use novafs::{Engine, Ids, Kind, Mode, device::FileImage};
use novaos_identity::{Authority, Head, Registry, Status, created_user, lab::Ledger};
use std::{fs::File, path::Path};

fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    assert_eq!(args.len(), 2, "répertoire secteur");
    let dir = Path::new(&args[0]);
    let sector = args[1].parse().unwrap();
    let image = |name: &str, pages| {
        FileImage::from_file(File::open(dir.join(name)).unwrap(), pages, sector).unwrap()
    };
    let ids = Ids {
        fs: [8; 16],
        lv: [9; 16],
    };
    let mut fs = Engine::mount_current(image("identites.p9", 512), ids, Mode::ReadOnly).unwrap();
    let folder = fs.lookup_at(fs.root_id(), "\x1fNovaAuth-D").unwrap();
    let file = fs.lookup_at(folder.id, "registre").unwrap();
    let protected = fs.lookup_at(folder.id, "prive").unwrap();
    let witness = fs.lookup_at(protected.id, "temoin").unwrap();
    let policy = fs.lookup_at(folder.id, "politique").unwrap();
    assert_eq!(folder.kind, Kind::Directory);
    assert_eq!(protected.kind, Kind::Directory);
    assert_eq!(file.kind, Kind::File);
    assert_eq!(policy.size, 104);
    let seal = fs.inspect_id(policy.id, 0, 104).unwrap();
    assert_eq!(&seal[..8], b"NADIR001");
    assert_eq!(&seal[24..40], ids.fs);
    assert_eq!(&seal[40..56], ids.lv);
    for (word, id) in
        seal[56..96]
            .chunks_exact(8)
            .zip([folder.id, file.id, protected.id, witness.id, policy.id])
    {
        assert_eq!(word, id.to_le_bytes());
    }
    assert_eq!(&seal[96..], &[3, 1, 0, 0, 0, 0, 0, 0]);
    assert_eq!(file.size, 16);
    assert_eq!(
        fs.inspect_id(file.id, 0, 8).unwrap(),
        0x4132303236u64.to_le_bytes()
    );
    assert_eq!(
        fs.inspect_id(file.id, 8, 8).unwrap(),
        0x4132303237u64.to_le_bytes()
    );
    let raw = std::fs::read(dir.join("autorite.raw")).unwrap();
    let entry = &raw[2 * 4096..3 * 4096];
    let size = u32::from_le_bytes(entry[40..44].try_into().unwrap()) as usize;
    let first: Head = serde_json::from_slice(&entry[44..44 + size]).unwrap();
    let mut ledger = Ledger::new(image("autorite.raw", 12));
    assert_eq!(ledger.outcome(first.request).unwrap(), Some(first));
    let old = created_user(&first);
    assert_eq!(&seal[8..24], old);
    let head = ledger.current().unwrap();
    assert_eq!(head.generation, 7);
    let reg = Registry::open(&mut fs, &mut ledger, [7; 16]).unwrap();
    let admin = reg
        .grant_attested(created_user(&reg.outcome([1; 16]).unwrap().unwrap()))
        .unwrap();
    assert_eq!(reg.inspect(admin, old).unwrap().status, Status::Deleted);
    assert_ne!(old, created_user(&head));
    println!(
        "P9/{sector} D={} ACL ancienne identité, sous-dossier protégé, deux décisions écrites exactement une fois, opération annulée absente, génération identités 7",
        folder.id
    );
}
