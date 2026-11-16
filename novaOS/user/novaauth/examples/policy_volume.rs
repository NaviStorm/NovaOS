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
    let init = arg[1] == "init";
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
    if init {
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
        let reg = Registry::open(&mut fs, &mut ledger, dom).unwrap();
        let alice = created_user(&reg.outcome([1; 16]).unwrap().unwrap());
        let bob = created_user(&reg.outcome([2; 16]).unwrap().unwrap());
        let group = created_group(&reg.outcome([3; 16]).unwrap().unwrap());
        let installation = installed(&reg.outcome([4; 16]).unwrap().unwrap());
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
        println!(
            "P9/{sector} domaine et empreinte verifies; generation {}; Alice lecture directe, groupe={member}; Bob exclu",
            h.generation
        );
    }
}
