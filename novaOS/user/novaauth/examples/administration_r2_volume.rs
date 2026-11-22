//! Fixture R2 : Alice administratrice et Bob indépendant, deux facteurs attestés.
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
    if a.get(1).map(String::as_str)==Some("--capture-published") {
        // Analyse hors ligne uniquement : la tête est relevée dans la preuve
        // d'autorité indépendante. Ce témoin ne confère aucune fraîcheur invité.
        struct Observed(Head);
        impl Authority for Observed {
            fn current(&self)->Result<Head> {Ok(self.0)}
            fn outcome(&self,_:Id)->Result<Option<Head>> {Err(Fault::Denied)}
            fn decide(&mut self,_:Head,_:Head)->Result<()> {Err(Fault::Denied)}
        }
        let sector=a[4].parse().unwrap();
        let file=fs::File::open(&a[2]).unwrap();
        let bytes=file.metadata().unwrap().len();assert_eq!(bytes%4096,0);
        let device=FileImage::from_file(file,bytes/4096,sector).unwrap();
        let head=recuperation::decode_head(&fs::read(&a[3]).unwrap()).unwrap();
        let engine=novafs::Engine::mount_current(device,Ids {fs:head.volume[0],lv:head.volume[1]},novafs::Mode::ReadOnly).unwrap();
        let actual=Triplet::capture(&engine,&Observed(head),head.domain).unwrap();
        let work=engine.lookup_at(engine.root_id(),"travail").unwrap().id;
        let object=engine.lookup_at(work,"donnees.tsv").unwrap().id;
        assert_eq!(engine.read_id(object,0,64).unwrap(),b"A\tB\tC\nD\tE\tF\n");
        save(Path::new(&a[5]),&actual.encode());
        println!("{{\"empreinte_triplet_verifiee\":true,\"donnees_privees_inchangees\":true,\"generation\":{}}}",head.generation);
        return;
    }
    if a.get(1).map(String::as_str)==Some("--verify") {
        // Vérification hors ligne des effets persistants du banc, jamais une
        // décision de fraîcheur pour l'invité. Tous les descripteurs sont RO.
        let sector=a[4].parse().unwrap();
        let open=|name:&str| {
            let f=fs::File::open(name).unwrap();
            let bytes=f.metadata().unwrap().len();
            assert_eq!(bytes%4096,0);
            FileImage::from_file(f,bytes/4096,sector).unwrap()
        };
        let ledger=Ledger::new(open(&a[3]));
        let fs=novafs::Engine::mount_current(open(&a[2]),Ids {fs:[8;16],lv:[9;16]},novafs::Mode::ReadOnly).unwrap();
        let actual=Triplet::capture(&fs,&ledger,[7;16]).unwrap();
        assert_eq!(actual.encode(),fs::read(&a[5]).unwrap());
        let work=fs.lookup_at(fs.root_id(),"travail").unwrap().id;
        let file=fs.lookup_at(work,"donnees.tsv").unwrap().id;
        assert_eq!(fs.read_id(file,0,64).unwrap(),b"A\tB\tC\nD\tE\tF\n");
        println!("{{\"triplet_identique\":true,\"donnees_privees_inchangees\":true,\"generation\":{}}}",actual.head().generation);
        return;
    }
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
    // Le journal hors ligne contient une décision par bloc. Les six commandes
    // pour deux titulaires ajoutent exactement 2 * (1 paquet + 6 portées)
    // décisions aux dix états (genèse incluse) de la fixture avec alias.
    // Ce disque sert à fabriquer/vérifier la fixture ; l'invité consulte
    // exclusivement l'autorité indépendante, pas ce journal local.
    let mut ledger = Ledger::new(image("autorite.raw", if a.get(10).is_some() {25} else if a.get(9).is_some() {24} else {12}));
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
    let scope_bits: u8 = 15;
    let bob_public: [u8;32] = fs::read(&a[7]).unwrap().try_into().unwrap();
    let bob_reference:Id=Sha256::digest([b"NovaAuth/Ed25519/credential/v1".as_slice(), &domain, &bob_public].concat())[..16].try_into().unwrap();
    let enrolled = reg
        .enroll(&permit, [1; 16], "alice", reference, "base")
        .unwrap();
    let owner = created_user(&enrolled);
    let group = created_group(&enrolled);
    let grant = reg.grant_attested(owner).unwrap();
    let bob_outcome=reg.mutate(
        grant,
        [3; 16],
        Change::Create {
            name: "bob",
            credential: bob_reference,
            group,
        },
    )
    .unwrap();
    let bob=created_user(&bob_outcome);
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
    let grant=reg.grant_attested(bob).unwrap();
    let package=fs::read(&a[6]).unwrap();
    assert!(matches!(reg.publish_package(grant,[153;16],Action::Install,&key,machine,&package),Err(Fault::Denied)));
    assert!(reg.outcome([153;16]).unwrap().is_none());
    let administrator=reg.grant_attested(owner).unwrap();
    let head=reg.provision_package_for_user(administrator,bob,[153;16],&key,machine,&package).unwrap();
    let grant=reg.grant_attested(bob).unwrap();
    let admitted=reg.load_package(grant,&key,machine,"admin-sonde").unwrap();
    let image=Image {installation:admitted.principal.unwrap(),digest:admitted.digest};
    reg.provision_administrative_image(head,[154;16],bob,image,Some(administration::Scope::parse(1).unwrap())).unwrap();
    // Témoin distinct : ce paquet signé n'est jamais publié pour Bob.
    // Argument facultatif pour conserver les fixtures historiques inchangées.
    if let Some(path)=a.get(8) {
        let alice_package=fs::read(path).unwrap();
        let alice=reg.grant_attested(owner).unwrap();
        let head=reg.publish_package(alice,[155;16],Action::Install,&key,machine,&alice_package).unwrap();
        let alice=reg.grant_attested(owner).unwrap();
        let loaded=reg.load_package(alice,&key,machine,"alice-seulement").unwrap();
        let image=Image {installation:loaded.principal.unwrap(),digest:loaded.digest};
        reg.provision_administrative_image(head,[156;16],owner,image,Some(administration::Scope::parse(1).unwrap())).unwrap();
        let bob_grant=reg.grant_attested(bob).unwrap();
        assert!(matches!(reg.load_package(bob_grant,&key,machine,"alice-seulement"),Err(Fault::Denied)));
    }
    // Paquet facultatif des six commandes : la fixture historique sans cet
    // argument reste identique. Seul le provisionneur accorde les portées.
    if let Some(path)=a.get(9) {
        let package=fs::read(path).unwrap();
        for (slot,recipient,bits) in [(0,owner,15),(1,bob,1)] {
            let administrator=reg.grant_attested(owner).unwrap();
            let mut head=reg.provision_package_for_user(administrator,recipient,[160+slot*8;16],&key,machine,&package).unwrap();
            for (i,name) in ["whoami","id","groups","useradd","usermod","groupadd"].iter().enumerate() {
                let grant=reg.grant_attested(recipient).unwrap();
                let loaded=reg.load_package(grant,&key,machine,name).unwrap();
                let image=Image {installation:loaded.principal.unwrap(),digest:loaded.digest};
                head=reg.provision_administrative_image(head,[161+slot*8+i as u8;16],recipient,image,Some(administration::Scope::parse(bits).unwrap())).unwrap();
            }
        }
    }
    if let Some(name)=a.get(10) {
        assert_eq!(name,"equipe-r2");
        let grant=reg.grant_attested(owner).unwrap();
        reg.mutate(grant,[190;16],Change::AddGroup(name)).unwrap();
    }
    drop(reg);
    let mut identities=Vec::new();identities.extend_from_slice(&owner);identities.extend_from_slice(&bob);identities.extend_from_slice(&group);
    save(&root.join("identites-temoins.bin"),&identities);
    let mut bob_oracle=[0;64];bob_oracle[8..24].copy_from_slice(&bob);bob_oracle[24]=3;bob_oracle[25..28].copy_from_slice(b"bob");bob_oracle[57]=1;
    save(&root.join("oracle-bob.bin"),&bob_oracle);
    let mut oracle=[0;64];oracle[8..24].copy_from_slice(&owner);oracle[24]=5;
    oracle[25..30].copy_from_slice(b"alice");oracle[57]=1;oracle[58]=1;
    save(&root.join("oracle-self.bin"),&oracle);
    let triplet = Triplet::capture(&fs, &ledger, domain).unwrap();
    save(&root.join("triplet.bin"), &triplet.encode());
    fs.unmount().unwrap();
    println!(
        "fixture hors ligne : deux installations R2 attestées, permissions distinctes, génération={}, secteur={sector}",
        triplet.head().generation
    );
}
