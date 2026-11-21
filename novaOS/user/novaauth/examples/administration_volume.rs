//! Fixture R1 : propriétaire authentifiable, portée explicite, aucun droit de fichier.
//! Ne constitue ni une authentification ni une preuve de lancement dans l'invité.
use novafs::{Ids, Kind, device::FileImage};
use novaos_identity::{
    distribution::{Action, Machine},
    lab::Ledger,
    policy::Image,
    recuperation::Triplet,
    *,
};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
};
fn save(path: &Path, data: &[u8]) {
    let mut f = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .unwrap();
    f.write_all(data).unwrap();
    f.sync_all().unwrap();
}
fn main() {
    let a: Vec<_> = std::env::args().collect();
    let root = Path::new(&a[1]);
    fs::create_dir(root).unwrap();
    let sector = a[2].parse().unwrap();
    let machine = match a[3].as_str() {
        "aarch64" => Machine::Aarch64,
        "x86_64" => Machine::X86_64,
        _ => panic!("ISA"),
    };
    let public = fs::read(&a[4]).unwrap();
    let public = if public.len() == 32 {
        public
    } else {
        let h = std::str::from_utf8(&public).unwrap().trim();
        assert_eq!(h.len(), 64);
        (0..32)
            .map(|i| u8::from_str_radix(&h[i * 2..i * 2 + 2], 16).unwrap())
            .collect()
    };
    let reference: Id = Sha256::digest(
        [
            b"NovaAuth/Ed25519/credential/v1".as_slice(),
            &[7; 16],
            &public,
        ]
        .concat(),
    )[..16]
        .try_into()
        .unwrap();
    let image = |name: &str, blocks: u64| {
        let f = OpenOptions::new()
            .create_new(true)
            .read(true)
            .write(true)
            .open(root.join(name))
            .unwrap();
        f.set_len(blocks * 4096).unwrap();
        FileImage::from_file(f, blocks, sector).unwrap()
    };
    let mut ledger = Ledger::new(image("autorite.raw", 12));
    let domain = [7; 16];
    let mut fs = provision(
        image("identites.p9", 512),
        Ids {
            fs: [8; 16],
            lv: [9; 16],
        },
        domain,
        [4; 16],
        &mut ledger,
    )
    .unwrap();
    let permit = Enrollment::from_provisioner(&mut ledger).unwrap();
    let mut reg = Registry::open(&mut fs, &mut ledger, domain).unwrap();
    let scope_bits: u8 = a.get(7).map(|s| s.parse().unwrap()).unwrap_or(1);
    let enrolled = reg
        .enroll(&permit, [1; 16], "alice", reference, "base")
        .unwrap();
    let owner = created_user(&enrolled);
    let group = created_group(&enrolled);
    let grant = reg.grant_attested(owner).unwrap();
    reg.mutate(
        grant,
        [3; 16],
        Change::Create {
            name: "bob",
            credential: [191; 16],
            group,
        },
    )
    .unwrap();
    drop(reg);
    let root_id = fs.root_id();
    let work = fs
        .create_at(root_id, "travail", Kind::Directory)
        .unwrap()
        .id;
    let file = fs.create_at(work, "donnees.tsv", Kind::File).unwrap().id;
    fs.prepare_write_id(file, 0, b"A\tB\tC\nD\tE\tF\n")
        .unwrap()
        .commit()
        .unwrap();
    let system = fs.lookup_at(root_id, "system").unwrap().id;
    fs.create_at(system, "bin", Kind::Directory).unwrap();
    let mut seal = Vec::from(*b"NXScope1");
    seal.extend_from_slice(&owner);
    seal.extend_from_slice(&work.to_le_bytes());
    seal.extend_from_slice(&file.to_le_bytes());
    let scope = fs
        .create_at(root_id, "\u{1f}NXScope", Kind::File)
        .unwrap()
        .id;
    fs.prepare_write_id(scope, 0, &seal)
        .unwrap()
        .commit()
        .unwrap();
    let head = ledger.current().unwrap();
    let mut reg = Registry::open(&mut fs, &mut ledger, domain).unwrap();
    reg.bind_folder(head, owner, work, &seal, [152; 16])
        .unwrap();
    let key = distribution::public_key(&fs::read(&a[5]).unwrap().try_into().unwrap()).unwrap();
    let grant = reg.grant_attested(owner).unwrap();
    let head = reg
        .publish_package(
            grant,
            [150; 16],
            Action::Install,
            &key,
            machine,
            &fs::read(&a[6]).unwrap(),
        )
        .unwrap();
    let grant = reg.grant_attested(owner).unwrap();
    let app = reg
        .load_package(grant, &key, machine, "admin-sonde")
        .unwrap()
        .principal
        .unwrap();
    reg.provision_administrative_image(
        head,
        [151; 16],
        owner,
        Image {
            installation: app,
            digest: reg
                .load_package(grant, &key, machine, "admin-sonde")
                .unwrap()
                .digest,
        },
        Some(administration::Scope::parse(scope_bits).unwrap()),
    )
    .unwrap();
    drop(reg);
    let mut oracle=[0;64];oracle[8..24].copy_from_slice(&owner);oracle[24]=5;
    oracle[25..30].copy_from_slice(b"alice");oracle[57]=1;oracle[58]=1;
    save(&root.join("oracle-self.bin"),&oracle);
    let triplet = Triplet::capture(&fs, &ledger, domain).unwrap();
    save(&root.join("triplet.bin"), &triplet.encode());
    fs.unmount().unwrap();
    println!(
        "fixture hors ligne : installation R1 attestée sans capacité de fichier, génération={}, secteur={sector}",
        triplet.head().generation
    );
}
