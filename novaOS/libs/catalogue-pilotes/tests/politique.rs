#[path = "../examples/support/banc.rs"]
mod lab;
use lab::*;
use novaos_catalogue::*;
use sha2::{Digest, Sha256};

#[test]
fn publication_atomique_et_derniere_place() {
    let policy = policy();
    let mut c = Catalog::new(&policy);
    for i in 0..65 {
        let s = RAW.replace("org.novaos.lab.port", &format!("org.novaos.lab.port{i:02}"));
        let r = c.admit(s.as_bytes(), &sign(s.as_bytes(), 1), &objects());
        if i < 64 {
            assert!(r.is_ok())
        } else {
            assert_eq!(r, Err(Error::Limit))
        }
    }
    assert_eq!(c.len(), 64);
}
#[test]
fn profil_obsolete_revoque_ou_etranger() {
    let mut p = policy();
    let trust = *include_bytes!("../vecteurs/systeme.pub");
    let record = include_bytes!("../vecteurs/profil.toml");
    let sig = include_bytes!("../vecteurs/profil.sig");
    let art = include_bytes!("../vecteurs/profil.bin");
    let mut fresh = Policy::new(2, Origin::Laboratoire, 1, vec![signer()], trust).unwrap();
    assert_eq!(fresh.add_resource(record, art, sig), Err(Error::Denied));
    let digest = Sha256::digest(art).into();
    let mut target = target("aarch64");
    target.profile = Some([5; 32]);
    {
        let mut c = Catalog::new(&p);
        c.admit(RAW.as_bytes(), SIG, &objects()).unwrap();
        assert_eq!(c.select(&target, None), Err(Error::Denied));
    }
    p.block_digest(digest).unwrap();
    assert_eq!(p.add_resource(record, art, sig), Err(Error::Revoked));
    {
        let mut c = Catalog::new(&p);
        c.admit(RAW.as_bytes(), SIG, &objects()).unwrap();
        assert_eq!(c.select(&target, None), Err(Error::Dependency));
    }
    p.block_digest(Sha256::digest(RAW.as_bytes()).into())
        .unwrap();
    assert_eq!(
        Catalog::new(&p).admit(RAW.as_bytes(), SIG, &objects()),
        Err(Error::Revoked)
    );
}
#[test]
fn dependance_facultative_et_droits() {
    let p = policy();
    let mut c = Catalog::new(&p);
    let s = format!(
        "{RAW}\n[[dependency]]\nid=\"absente\"\nkind=\"service\"\nmin=[1,0,0]\nbefore=[2,0,0]\nsha256=\"{}\"\nrequired=false\n",
        "77".repeat(32)
    );
    c.admit(s.as_bytes(), &sign(s.as_bytes(), 1), &objects())
        .unwrap();
    let mut t = target("aarch64");
    t.caps = &["rx", "stats"];
    let plan = c.select(&t, None).unwrap();
    assert_eq!(plan.capabilities(), ["rx"]);
    assert_eq!(plan.quotas().ipc_roots_created, 1);
    t.abi = 2;
    assert_eq!(c.select(&t, None), Err(Error::NoMatch));
    t.abi = 1;
    t.features = &[];
    assert_eq!(c.select(&t, None), Err(Error::NoMatch));
}
#[test]
fn borne_exacte_et_signataire_non_injecte() {
    let p = policy();
    let mut c = Catalog::new(&p);
    let s = format!("{RAW}{}", " ".repeat(MAX_MANIFEST - RAW.len()));
    assert!(
        c.admit(s.as_bytes(), &sign(s.as_bytes(), 1), &objects())
            .is_ok()
    );
    let foreign = RAW.replace("lab-v1", "cle-du-paquet");
    assert_eq!(
        c.admit(foreign.as_bytes(), &sign(foreign.as_bytes(), 1), &objects()),
        Err(Error::Denied)
    );
    let s = RAW.replace(
        "elf_sha256=\"",
        &format!("elf_sha256=\"{}\"\n#", "77".repeat(32)),
    );
    assert_eq!(
        c.admit(s.as_bytes(), &sign(s.as_bytes(), 1), &objects()),
        Err(Error::Digest)
    );
}
#[test]
fn distribution_fictive_authentifiee() {
    let k = Signer {
        origin: Origin::Distribution,
        key_id: "dist-v1".into(),
        manifest_key: *include_bytes!("../vecteurs/dist-pilote.pub"),
        elf_key: *include_bytes!("../vecteurs/dist-elf.pub"),
        ..signer()
    };
    let mut p = Policy::new(
        1,
        Origin::Distribution,
        3,
        vec![k, signer()],
        *include_bytes!("../vecteurs/dist-systeme.pub"),
    )
    .unwrap();
    p.add_resource(
        include_bytes!("../vecteurs/dist-profil.toml"),
        include_bytes!("../vecteurs/profil.bin"),
        include_bytes!("../vecteurs/dist-profil.sig"),
    )
    .unwrap();
    let mut c = Catalog::new(&p);
    assert_eq!(c.admit(RAW.as_bytes(), SIG, &objects()), Err(Error::Denied));
    let blobs = [
        Blob {
            isa: "aarch64",
            bytes: include_bytes!("../vecteurs/dist-aarch64.pkg01"),
        },
        Blob {
            isa: "x86_64",
            bytes: include_bytes!("../vecteurs/dist-x86_64.pkg01"),
        },
    ];
    c.admit(
        include_bytes!("../vecteurs/dist-pilote.toml"),
        include_bytes!("../vecteurs/dist-pilote.sig"),
        &blobs,
    )
    .unwrap();
    assert_eq!(
        c.select(&target("x86_64"), None)
            .unwrap()
            .dependencies()
            .len(),
        1
    );
}
#[test]
fn variante_de_bus_future() {
    let p = policy();
    let mut c = Catalog::new(&p);
    let s = RAW.replace("[[match]]", "[[match]]\nbus=\"bus-futur\"\n\n[[match]]");
    c.admit(s.as_bytes(), &sign(s.as_bytes(), 1), &objects())
        .unwrap();
    assert!(c.select(&target("aarch64"), None).is_ok());
}
