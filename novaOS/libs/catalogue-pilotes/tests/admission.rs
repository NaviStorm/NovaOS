#[path = "../examples/support/banc.rs"]
mod lab;
use lab::*;
use novaos_catalogue::*;
use sha2::{Digest, Sha256};
#[test]
fn vecteurs_independants_et_idempotence() {
    let p = policy();
    let mut c = Catalog::new(&p);
    assert_eq!(sign(RAW.as_bytes(), 1).as_slice(), SIG);
    let h = c.admit(RAW.as_bytes(), SIG, &objects()).unwrap();
    assert_eq!(c.admit(RAW.as_bytes(), SIG, &objects()), Ok(h));
    assert_eq!(c.len(), 1);
    for isa in ["aarch64", "x86_64"] {
        let plan = c.select(&target(isa), Some(h)).unwrap();
        assert_eq!(plan.architecture(), isa);
        assert_eq!(plan.dependencies().len(), 1);
    }
}
#[test]
fn schema_bornes_et_troncatures() {
    let p = policy();
    let mut c = Catalog::new(&p);
    for text in [
        RAW.replace("ipc_roots_created=1", "ipc_roots_created=5"),
        RAW.replace("[hardware]", "[hardware]\ninconnu=7"),
        RAW.replace("rx:w", "rx:all"),
        RAW.replace("required_caps=[\"rx\"]", "required_caps=[\"rx\",\"rx\"]"),
        RAW.replace("schema=", "schema=\"novaos.driver/1\"\nschema="),
        RAW.replace("0x1041", "65536"),
        RAW.replace("org.novaos.lab.port", "org.autre.port"),
        RAW.replace("x86_64", "mips"),
        RAW.replace("os_before=[0,21,0]", "os_before=[0,19,0]"),
        RAW.replace("sha256=\"", "sha256=\"Z"),
    ] {
        assert!(
            c.admit(text.as_bytes(), &sign(text.as_bytes(), 1), &objects())
                .is_err()
        );
    }
    for cut in [0, 10, RAW.len() / 2] {
        assert!(c.admit(&RAW.as_bytes()[..cut], SIG, &objects()).is_err());
    }
    assert_eq!(
        c.admit(&vec![32; 16385], SIG, &objects()),
        Err(Error::Limit)
    );
    assert_eq!(c.admit(&[255], SIG, &objects()), Err(Error::Format));
    assert!(c.is_empty());
}
#[test]
fn deux_signatures_et_empreintes() {
    let p = policy();
    let mut c = Catalog::new(&p);
    assert_eq!(
        c.admit(RAW.as_bytes(), &sign(RAW.as_bytes(), 3), &objects()),
        Err(Error::Signature)
    );
    assert_eq!(
        c.admit(RAW.as_bytes(), &SIG[..63], &objects()),
        Err(Error::Signature)
    );
    let mut bad = ARM.to_vec();
    bad[120] ^= 1;
    let blobs = [
        Blob {
            isa: "aarch64",
            bytes: &bad,
        },
        Blob {
            isa: "x86_64",
            bytes: X86,
        },
    ];
    assert_eq!(c.admit(RAW.as_bytes(), SIG, &blobs), Err(Error::Digest));
    let text = RAW.replace(
        &format!("{:x}", Sha256::digest(ARM)),
        &format!("{:x}", Sha256::digest(&bad)),
    );
    assert_eq!(
        c.admit(text.as_bytes(), &sign(text.as_bytes(), 1), &blobs),
        Err(Error::Elf)
    );
    for text in [
        RAW.replace("elf_package_revision=1", "elf_package_revision=2"),
        RAW.replace("member=\"pilote\"", "member=\"autre\""),
    ] {
        assert_eq!(
            c.admit(text.as_bytes(), &sign(text.as_bytes(), 1), &objects()),
            Err(Error::Elf)
        );
    }
    let mut p = policy();
    assert_eq!(
        p.add_resource(
            include_bytes!("../vecteurs/profil.toml"),
            b"modifie",
            include_bytes!("../vecteurs/profil.sig")
        ),
        Err(Error::Digest)
    );
    assert!(c.is_empty());
}
#[test]
fn rotation_retrait_origines_plancher() {
    for (mode, floor, revoked, first, expected) in [
        (Origin::Distribution, 1, false, 1, Error::Denied),
        (Origin::Laboratoire, 4, false, 1, Error::Floor),
        (Origin::Laboratoire, 1, true, 1, Error::Revoked),
        (Origin::Laboratoire, 1, false, 4, Error::Floor),
    ] {
        let mut k = signer();
        k.revoked = revoked;
        k.first = first;
        let p = Policy::new(2, mode, floor, vec![k], SYS).unwrap();
        assert_eq!(
            Catalog::new(&p).admit(RAW.as_bytes(), SIG, &objects()),
            Err(expected)
        );
    }
    let mut old = signer();
    old.revoked = true;
    let mut next = signer();
    next.key_id = "lab-v2".into();
    next.manifest_key = ed25519_dalek::SigningKey::from_bytes(&[4; 32])
        .verifying_key()
        .to_bytes();
    let p = Policy::new(2, Origin::Laboratoire, 3, vec![old, next], SYS).unwrap();
    let text = RAW.replace("lab-v1", "lab-v2");
    assert!(
        Catalog::new(&p)
            .admit(text.as_bytes(), &sign(text.as_bytes(), 4), &objects())
            .is_ok()
    );
}
#[test]
fn rollback_en_memoire_et_conflit_de_revision() {
    let p = policy();
    let mut c = Catalog::new(&p);
    let old = c.admit(RAW.as_bytes(), SIG, &objects()).unwrap();
    let alt = RAW.replace("private_pages=32", "private_pages=16");
    assert_eq!(
        c.admit(alt.as_bytes(), &sign(alt.as_bytes(), 1), &objects()),
        Err(Error::Conflict)
    );
    let next = RAW.replacen("version=[1,0,0]", "version=[2,0,0]", 1);
    c.admit(next.as_bytes(), &sign(next.as_bytes(), 1), &objects())
        .unwrap();
    assert_eq!(
        c.select(&target("aarch64"), None).unwrap().version(),
        [2, 0, 0]
    );
    assert_eq!(
        c.select(&target("aarch64"), Some(old)).unwrap().version(),
        [1, 0, 0]
    );
}
#[test]
fn conflit_editeurs_et_ordre() {
    let mut b = signer();
    b.publisher = "org.autre".into();
    b.key_id = "autre".into();
    b.manifest_key = ed25519_dalek::SigningKey::from_bytes(&[7; 32])
        .verifying_key()
        .to_bytes();
    let mut p = Policy::new(1, Origin::Laboratoire, 1, vec![signer(), b], SYS).unwrap();
    p.add_resource(
        include_bytes!("../vecteurs/profil.toml"),
        include_bytes!("../vecteurs/profil.bin"),
        include_bytes!("../vecteurs/profil.sig"),
    )
    .unwrap();
    let alt = RAW
        .replace("org.novaos.lab.port", "org.autre.port")
        .replace("publisher=\"org.novaos.lab\"", "publisher=\"org.autre\"")
        .replace("lab-v1", "autre");
    let sig = sign(alt.as_bytes(), 7);
    for reverse in [false, true] {
        let mut c = Catalog::new(&p);
        let mut entries = [(RAW, SIG), (alt.as_str(), sig.as_slice())];
        if reverse {
            entries.reverse();
        }
        for (m, s) in entries {
            c.admit(m.as_bytes(), s, &objects()).unwrap();
        }
        assert_eq!(c.select(&target("aarch64"), None), Err(Error::Conflict));
    }
}
