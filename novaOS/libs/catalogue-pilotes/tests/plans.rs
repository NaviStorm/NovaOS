#[path = "../examples/support/maximal.rs"]
mod fixture;
#[allow(dead_code)]
#[path = "../examples/support/banc.rs"]
mod lab;
use novaos_catalogue::*;

/// Même test compilé contre les deux versions par comparer.py.
#[test]
fn plans_et_decisions() {
    let p = lab::policy();
    for n in [1, 16, 32, 64] {
        let mut c = Catalog::new(&p);
        let mut hashes = Vec::new();
        for i in (0..n).rev() {
            let s = lab::RAW.replace("version=[1,0,0]", &format!("version=[1,{i},0]"));
            let hash = c
                .admit(s.as_bytes(), &lab::sign(s.as_bytes(), 1), &lab::objects())
                .unwrap();
            hashes.push(hash);
        }
        for isa in ["aarch64", "x86_64"] {
            let t = lab::target(isa);
            for pin in [None, Some(hashes[n - 1]), Some([0; 32])] {
                let plan = c.select(&t, pin);
                if pin == Some([0; 32]) {
                    assert_eq!(plan, Err(Error::NoMatch));
                } else {
                    assert_eq!(
                        plan.as_ref().unwrap().version(),
                        [1, if pin.is_none() { n as u32 - 1 } else { 0 }, 0]
                    );
                }
                println!("PLAN|{n}|{isa}|{plan:?}");
            }
        }
    }
    let s = fixture::maximal(lab::RAW);
    let mut c = Catalog::new(&p);
    let hash = c
        .admit(s.as_bytes(), &lab::sign(s.as_bytes(), 1), &lab::objects())
        .unwrap();
    for isa in ["aarch64", "x86_64"] {
        let t = lab::target(isa);
        let plan = c.select(&t, Some(hash)).unwrap();
        assert_eq!(plan.driver().len(), 96);
        assert_eq!(plan.capabilities(), &["rx"]);
        println!("PLAN|max|{isa}|{plan:?}");
    }
}

/// Exécute la chaîne de 63 ressources et un document imbriqué sur une pile hôte
/// explicitement réservée. L'autorité et les objets sont ceux du banc fictif.
#[cfg(target_os = "macos")]
#[test]
fn plan_transitif_sur_pile_bornee() {
    use ed25519_dalek::{Signer as _, SigningKey};
    use std::ffi::c_void;
    unsafe extern "C" {
        fn pthread_self() -> *mut c_void;
        fn pthread_get_stacksize_np(thread: *mut c_void) -> usize;
    }
    const STACK: usize = 128 * 1024;
    fn check() {
        // pthread_self donne le thread courant vivant pour cette consultation.
        let actual = unsafe { pthread_get_stacksize_np(pthread_self()) };
        println!("Pile Darwin réservée {actual}");
        assert!((STACK..=192 * 1024).contains(&actual));
        let mut policy =
            Policy::new(1, Origin::Laboratoire, 1, vec![lab::signer()], lab::SYS).unwrap();
        let record = include_str!("../vecteurs/profil.toml");
        let artifact = include_bytes!("../vecteurs/profil.bin");
        let digest = record
            .lines()
            .find_map(|l| l.strip_prefix("sha256="))
            .unwrap();
        let key = SigningKey::from_bytes(&[3; 32]);
        for i in (0..63).rev() {
            let id = if i == 0 {
                "org.novaos.lab.courtier".to_owned()
            } else {
                format!("lab.service.{i}")
            };
            let kind = if i == 0 { "system-profile" } else { "service" };
            let mut doc = record
                .replace("org.novaos.lab.courtier", &id)
                .replace("system-profile", kind);
            if i < 62 {
                doc=doc.replace("dependencies=[]",&format!("dependencies=[{{id='lab.service.{}',kind='service',min=[1,0,0],before=[2,0,0],sha256={digest},required=true}}]",i+1));
            }
            let mut envelope = b"NovaOS/driver-resource/v1\0".to_vec();
            envelope.extend_from_slice(&(doc.len() as u32).to_le_bytes());
            envelope.extend_from_slice(doc.as_bytes());
            policy
                .add_resource(doc.as_bytes(), artifact, &key.sign(&envelope).to_bytes())
                .unwrap();
        }
        let mut catalog = Catalog::new(&policy);
        for doc in [
            fixture::maximal(lab::RAW),
            format!(
                "{}\n[[match]]\nbus='futur'\nextra={}0{}",
                lab::RAW,
                "[".repeat(28),
                "]".repeat(28)
            ),
        ] {
            // Les 15 références optionnelles du manifeste maximal s'ajoutent à la chaîne.
            let mut isolated = Catalog::new(&policy);
            let h = isolated
                .admit(
                    doc.as_bytes(),
                    &lab::sign(doc.as_bytes(), 1),
                    &lab::objects(),
                )
                .unwrap();
            let result = isolated.select(&lab::target("aarch64"), Some(h));
            if doc.len() == MAX_MANIFEST {
                assert_eq!(result, Err(Error::Limit));
            } else {
                assert_eq!(result.unwrap().dependencies().len(), 63);
            }
        }
        catalog
            .admit(lab::RAW.as_bytes(), lab::SIG, &lab::objects())
            .unwrap();
        for isa in ["aarch64", "x86_64"] {
            let p = catalog.select(&lab::target(isa), None).unwrap();
            assert_eq!(p.dependencies().len(), 63);
            println!("PLAN|chaine63|{isa}|{p:?}");
        }
    }
    std::thread::Builder::new()
        .stack_size(STACK)
        .spawn(check)
        .unwrap()
        .join()
        .unwrap();
}
