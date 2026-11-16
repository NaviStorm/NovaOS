//! Provisionnement hors ligne d'une copie neuve et oracle de relecture après QEMU.
use novafs::{Engine, Ids, Kind, Mode, device::FileImage};
use novaos_identity::{lab::Ledger, policy::*, *};
use sha2::{Digest, Sha256};
use std::{env, fs::OpenOptions, path::Path};
fn decode<const N: usize>(s: &str) -> [u8; N] {
    assert_eq!(s.len(), 2 * N);
    core::array::from_fn(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap())
}
fn main() {
    let arg: Vec<_> = env::args().collect();
    let init = matches!(arg[1].as_str(), "init" | "fixture");
    let p = Path::new(&arg[2]);
    let sector = arg[3].parse().unwrap();
    let image = |name, n| {
        FileImage::from_file(
            OpenOptions::new()
                .read(true)
                .write(init)
                .open(p.join(name))
                .unwrap(),
            n,
            sector,
        )
        .unwrap()
    };
    let ids = Ids {
        fs: [8; 16],
        lv: [9; 16],
    };
    let dom = [7; 16];
    let mut ledger = Ledger::new(image("autorite.raw", 12));
    let mut fs = Engine::mount_current(
        image("identites.p9", 512),
        ids,
        if init {
            Mode::ReadWrite
        } else {
            Mode::ReadOnly
        },
    )
    .unwrap();
    if arg[1] == "fixture" {
        let dir = fs.lookup_at(fs.root_id(), "\u{1f}NovaAuth-D").unwrap().id;
        match arg[4].as_str() {
            "avant" | "apres" => {
                let file = fs.create_at(dir, "injection", Kind::File).unwrap();
                let boundary = if arg[4] == "avant" { 1 } else { 2 };
                fs.prepare_write_id(file.id, 0, &[boundary])
                    .unwrap()
                    .commit()
                    .unwrap();
            }
            mode @ ("mise-a-jour" | "reinstallation") => {
                let mut reg = Registry::open(&mut fs, &mut ledger, dom).unwrap();
                let owner = created_user(&reg.outcome([1; 16]).unwrap().unwrap());
                let install = installed(&reg.outcome([4; 16]).unwrap().unwrap());
                let auth = reg.grant_attested(owner).unwrap();
                let granted = reg
                    .set_permission(auth, owner, dir, install, Origin::Direct, READ, [6; 16])
                    .unwrap();
                // Deux modifications admises avec la même génération : la seconde doit relire.
                assert!(matches!(
                    reg.set_permission(auth, owner, dir, install, Origin::Direct, WRITE, [66; 16]),
                    Err(Fault::Conflict)
                ));
                assert_eq!(reg.outcome([66; 16]).unwrap(), None);
                if mode == "mise-a-jour" {
                    reg.provision_install(
                        granted,
                        [80; 16],
                        owner,
                        InstallChange::Update(install, decode(&arg[5])),
                    )
                    .unwrap();
                } else {
                    let removed = reg
                        .provision_install(granted, [89; 16], owner, InstallChange::Remove(install))
                        .unwrap();
                    let new = reg
                        .provision_install(
                            removed,
                            [90; 16],
                            owner,
                            InstallChange::New(decode(&arg[5])),
                        )
                        .unwrap();
                    assert_ne!(installed(&new), install);
                }
                println!(
                    "Deux demandes concurrentes de generation 5 : premiere acquittee, seconde en conflit, aucune decision pour la seconde."
                );
            }
            _ => panic!("fixture inconnue"),
        }
    } else if init {
        let reference = |s: &str| -> Id {
            let v = decode::<32>(s);
            let mut h = Sha256::new();
            h.update(b"NovaAuth/Ed25519/credential/v1");
            h.update(dom);
            h.update(v);
            h.finalize()[..16].try_into().unwrap()
        };
        let proof = Enrollment::from_provisioner(&mut ledger).unwrap();
        let mut reg = Registry::open(&mut fs, &mut ledger, dom).unwrap();
        let first = reg
            .enroll(&proof, [1; 16], "alice", reference(&arg[4]), "base")
            .unwrap();
        let alice = created_user(&first);
        reg.mutate(
            reg.grant_attested(alice).unwrap(),
            [2; 16],
            Change::Create {
                name: "bob",
                credential: reference(&arg[5]),
                group: created_group(&first),
            },
        )
        .unwrap();
        let g = reg
            .mutate(
                reg.grant_attested(alice).unwrap(),
                [3; 16],
                Change::AddGroup("atelier"),
            )
            .unwrap();
        let install = reg
            .provision_install(g, [4; 16], alice, InstallChange::New(decode(&arg[6])))
            .unwrap();
        drop(reg);
        let d = fs
            .create_at(fs.root_id(), "\u{1f}NovaAuth-D", Kind::Directory)
            .unwrap()
            .id;
        let data = fs.create_at(d, "registre", Kind::File).unwrap().id;
        let protected = fs.create_at(d, "prive", Kind::Directory).unwrap().id;
        let child = fs.create_at(protected, "temoin", Kind::File).unwrap().id;
        let policy = fs.create_at(d, "politique", Kind::File).unwrap().id;
        let mut seal = [0; 104];
        seal[..8].copy_from_slice(b"NADIR001");
        seal[8..24].copy_from_slice(&alice);
        seal[24..40].copy_from_slice(&ids.fs);
        seal[40..56].copy_from_slice(&ids.lv);
        for (bytes, id) in seal[56..96]
            .chunks_exact_mut(8)
            .zip([d, data, protected, child, policy])
        {
            bytes.copy_from_slice(&id.to_le_bytes());
        }
        seal[96] = 3;
        seal[97] = 1;
        fs.prepare_write_id(policy, 0, &seal)
            .unwrap()
            .commit()
            .unwrap();
        Registry::open(&mut fs, &mut ledger, dom)
            .unwrap()
            .bind_folder(install, alice, d, &seal, [5; 16])
            .unwrap();
        println!(
            "genese 5; installation {:02x?}; OID {d}",
            installed(&install)
        );
    } else {
        let h = ledger.current().unwrap();
        if arg.get(5).is_some_and(|v| v == "recul") {
            assert!(matches!(
                Registry::open(&mut fs, &mut ledger, dom),
                Err(Fault::StaleVolume | Fault::StateMismatch)
            ));
            println!(
                "Recul du P9 seul refuse par comparaison avec la decision externe de generation {}",
                h.generation
            );
            return;
        }
        let reg = Registry::open(&mut fs, &mut ledger, dom).unwrap();
        let alice = created_user(&reg.outcome([1; 16]).unwrap().unwrap());
        let bob = created_user(&reg.outcome([2; 16]).unwrap().unwrap());
        let group = created_group(&reg.outcome([3; 16]).unwrap().unwrap());
        let installation = installed(&reg.outcome([4; 16]).unwrap().unwrap());
        if arg[1] == "verify-path" {
            let bytes = std::fs::read(&arg[4]).unwrap();
            reg.verify_image(Image {
                installation,
                digest: Sha256::digest(bytes).into(),
            })
            .unwrap();
            println!("ELF atteste, installation {:02x?}", installation);
            return;
        }
        let d = reg.outcome([5; 16]).unwrap().unwrap();
        assert_eq!(d.domain, dom);
        drop(reg);
        let dir = fs.lookup_at(fs.root_id(), "\u{1f}NovaAuth-D").unwrap().id;
        let policy = fs.lookup_at(dir, "politique").unwrap();
        let seal = fs.inspect_id(policy.id, 0, 104).unwrap();
        let reg = Registry::open(&mut fs, &mut ledger, dom).unwrap();
        reg.verify_folder(alice, dir, &seal).unwrap();
        let image = Image {
            installation,
            digest: decode(&arg[4]),
        };
        if matches!(arg[5].as_str(), "avant" | "apres") {
            let after = arg[5] == "apres";
            let decision = reg.outcome([6; 16]).unwrap();
            assert_eq!(decision.is_some(), after);
            if after {
                assert_eq!(decision, Some(h));
            }
            assert_eq!(h.generation, if after { 6 } else { 5 });
            assert_eq!(
                reg.stored_permission(
                    reg.grant_attested(alice).unwrap(),
                    dir,
                    installation,
                    Origin::Direct
                )
                .unwrap(),
                if after { READ } else { 0 }
            );
            println!(
                "Coupure {} : generation {}, issue de la requete 06 = {:?}; aucune repetition.",
                arg[5],
                h.generation,
                decision.map(|d| d.generation)
            );
            return;
        }
        let member = arg[5] == "member";
        assert_eq!(
            reg.permission(alice, image, alice, dir, Origin::Direct)
                .unwrap()
                .rights,
            READ
        );
        assert_eq!(
            reg.permission(alice, image, alice, dir, Origin::Group(group))
                .is_ok(),
            member
        );
        for o in [Origin::Direct, Origin::Group(group)] {
            assert!(reg.permission(bob, image, alice, dir, o).is_err());
        }
        drop(reg);
        let data = fs.lookup_at(dir, "registre").unwrap();
        assert_eq!(data.size, 8);
        assert_eq!(fs.inspect_id(data.id, 0, 8).unwrap(), 35u64.to_le_bytes());
        println!(
            "Relecture octet par octet : seule la requete 35 figure dans le fichier; 36 et 37 absentes."
        );
        println!(
            "P9/{sector} domaine et empreinte verifies; generation {}; Alice lecture directe, groupe={member}; Bob exclu",
            h.generation
        );
    }
}
