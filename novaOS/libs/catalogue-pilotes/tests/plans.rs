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
