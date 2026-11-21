//! Témoin de composant. L’autorité utilise un fichier distinct des disques restaurés.
//! Ce test de composant utilise un socket privé local, sans TLS ni effecteur IPC invité.
#![cfg(all(unix, feature = "distribution-lab"))]

#[path = "support/autorite_recu.rs"]
mod serveur;
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use novafs::{Engine, Ids, Kind, Mode, device::FileImage};
use novaos_identity::{lab::Ledger, policy::*, *};
use serveur::{Independent, SignedReceipt};
use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};

const DOMAIN: Id = [7; 16];
const GRANT: Id = [5; 16];
const REVOKE: Id = [6; 16];

fn bytes(head: Head) -> Vec<u8> {
    let mut v = b"NovaAuth/fixture/receipt/v1\0".to_vec();
    v.extend_from_slice(&serde_json::to_vec(&head).unwrap());
    v
}
fn device(path: &Path, pages: u64, sector: u32, create: bool) -> FileImage {
    let f = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(create)
        .open(path)
        .unwrap();
    if create {
        f.set_len(pages * 4096).unwrap();
    }
    FileImage::from_file(f, pages, sector).unwrap()
}

struct Folder {
    owner: Id,
    oid: u64,
    image: Image,
}

fn read_folder(
    fs: &mut Engine<FileImage>,
    authority: &mut Independent,
    folder: &Folder,
    historical: &SignedReceipt,
    public: &VerifyingKey,
) -> Result<u64> {
    if !historical.verify(public) || historical.head.domain != DOMAIN {
        return Err(Fault::Denied);
    }
    let registry = Registry::open(fs, authority, DOMAIN)?;
    registry.verify_folder(folder.owner, folder.oid, b"acl du temoin")?;
    let permission = registry.permission(
        folder.owner,
        folder.image,
        folder.owner,
        folder.oid,
        Origin::Direct,
    )?;
    if permission.rights & READ == 0 {
        return Err(Fault::Denied);
    }
    Ok(fs.lookup_at(folder.oid, "temoin")?.id)
}

fn scenario(root: &Path, sector: u32) {
    let local = root.join("serveur");
    let external = root.join("autorite");
    let backup = root.join("sauvegarde");
    for p in [&local, &external, &backup] {
        std::fs::create_dir(p).unwrap();
    }
    let p9 = local.join("identites.p9");
    let remote_disk = external.join("decisions.raw");
    let ids = Ids {
        fs: [8; 16],
        lv: [9; 16],
    };
    let mut auth = Independent::connect(&remote_disk, sector, true);
    let public = auth.public;
    std::fs::write(root.join("autorite.pub"), public.to_bytes()).unwrap();
    let mut fs = provision(
        device(&p9, 512, sector, true),
        ids,
        DOMAIN,
        [4; 16],
        &mut auth,
    )
    .unwrap();
    let oid = fs
        .create_at(fs.root_id(), "dossier", Kind::Directory)
        .unwrap()
        .id;
    let witness = fs.create_at(oid, "temoin", Kind::File).unwrap().id;
    let enrollment = Enrollment::from_provisioner(&mut auth).unwrap();
    let mut reg = Registry::open(&mut fs, &mut auth, DOMAIN).unwrap();
    let enrolled = reg
        .enroll(&enrollment, [1; 16], "alice", [9; 16], "base")
        .unwrap();
    let owner = created_user(&enrolled);
    let installation = reg
        .provision_install(enrolled, [2; 16], owner, InstallChange::New([11; 32]))
        .unwrap();
    let folder = Folder {
        owner,
        oid,
        image: Image {
            installation: installed(&installation),
            digest: [11; 32],
        },
    };
    reg.bind_folder(installation, owner, oid, b"acl du temoin", [3; 16])
        .unwrap();
    let grant = reg
        .set_permission(
            reg.grant_attested(owner).unwrap(),
            owner,
            oid,
            folder.image.installation,
            Origin::Direct,
            READ,
            GRANT,
        )
        .unwrap();
    let old = auth.receipt(GRANT);
    let local_receipt = local.join("cache-recu.img");
    std::fs::write(&local_receipt, serde_json::to_vec(&old).unwrap()).unwrap();
    std::fs::copy(&local_receipt, backup.join("cache-recu.img")).unwrap();
    assert_eq!(old.head, grant);
    assert_eq!(
        read_folder(&mut fs, &mut auth, &folder, &old, &public).unwrap(),
        witness
    );
    fs.unmount().unwrap();
    std::fs::copy(&p9, backup.join("identites.p9")).unwrap();
    File::open(backup.join("identites.p9"))
        .unwrap()
        .sync_all()
        .unwrap();

    let mount =
        || Engine::mount_current(device(&p9, 512, sector, false), ids, Mode::ReadWrite).unwrap();
    let mut fs = mount();
    let mut reg = Registry::open(&mut fs, &mut auth, DOMAIN).unwrap();
    let withdrawn = reg
        .set_permission(
            reg.grant_attested(owner).unwrap(),
            owner,
            oid,
            folder.image.installation,
            Origin::Direct,
            0,
            REVOKE,
        )
        .unwrap();
    let ack = auth.receipt(REVOKE);
    std::fs::write(&local_receipt, serde_json::to_vec(&ack).unwrap()).unwrap();
    std::fs::write(
        root.join("retrait.recu.json"),
        serde_json::to_vec(&ack).unwrap(),
    )
    .unwrap();
    assert!(ack.verify(&public));
    assert_eq!(ack.head, withdrawn);
    assert_eq!(withdrawn.generation, grant.generation + 1);
    assert!(matches!(
        read_folder(&mut fs, &mut auth, &folder, &old, &public),
        Err(Fault::Denied)
    ));
    fs.unmount().unwrap();
    drop(auth);
    // Restaurer tout le stockage local de CE témoin. L’autorité est rouverte depuis son propre fichier.
    std::fs::copy(backup.join("identites.p9"), &p9).unwrap();
    assert_eq!(
        std::fs::read(&p9).unwrap(),
        std::fs::read(backup.join("identites.p9")).unwrap()
    );
    std::fs::copy(backup.join("cache-recu.img"), &local_receipt).unwrap();
    let old: SignedReceipt =
        serde_json::from_slice(&std::fs::read(&local_receipt).unwrap()).unwrap();
    let mut auth = Independent::connect(&remote_disk, sector, false);
    assert_eq!(auth.public, public);
    assert_eq!(auth.current().unwrap(), withdrawn);
    assert!(old.verify(&public));
    assert_eq!(auth.outcome(old.head.request).unwrap(), Some(old.head));
    let mut fs = mount();
    assert!(matches!(
        read_folder(&mut fs, &mut auth, &folder, &old, &public),
        Err(Fault::StaleVolume | Fault::StateMismatch)
    ));
    auth.stop();
    assert!(matches!(
        read_folder(&mut fs, &mut auth, &folder, &old, &public),
        Err(Fault::Unavailable)
    ));
    let auth = Independent::connect(&remote_disk, sector, false);
    assert_eq!(auth.current().unwrap(), withdrawn);
    println!(
        "P9/{sector} octroi={} retrait={} historique={} ouverture_apres_restauration=refusee autorite_indisponible=refusee",
        grant.generation, withdrawn.generation, old.head.generation
    );
}

#[test]
fn le_recu_de_l_octroi_survit_sans_retablir_le_droit() {
    let anchor =
        PathBuf::from(std::env::var_os("NOVAOS_TMPDIR").expect("répertoire autorisé du projet"));
    for sector in [512, 4096] {
        let dir = anchor.join(format!("recu-dossier-{}-{sector}", std::process::id()));
        std::fs::create_dir(&dir).unwrap();
        scenario(&dir, sector);
        println!("preuves_composant={}", dir.display());
    }
}

#[test]
#[ignore = "sous-processus privé démarré par la recette"]
fn processus_autorite() {
    serveur::serve();
}
