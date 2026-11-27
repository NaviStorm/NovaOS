#[path = "../examples/support/banc.rs"]
mod lab;
use lab::*;
use novaos_catalogue::*;
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    ptr,
};
thread_local! {
    static FAIL:Cell<Option<usize>>=const{Cell::new(None)};
    static LIVE:Cell<isize>=const{Cell::new(0)};
}
struct Allocator;
#[global_allocator]
static ALLOC: Allocator = Allocator;
// Comptage propre au thread de l'épreuve. realloc utilise alloc/copie/dealloc.
unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        let fail = FAIL.with(|c| match c.get() {
            Some(0) => {
                c.set(None);
                true
            }
            Some(i) => {
                c.set(Some(i - 1));
                false
            }
            None => false,
        });
        if fail {
            return ptr::null_mut();
        }
        let p = unsafe { System.alloc(l) };
        if !p.is_null() {
            LIVE.with(|c| c.set(c.get() + l.size() as isize));
        }
        p
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        LIVE.with(|c| c.set(c.get() - l.size() as isize));
        unsafe { System.dealloc(p, l) }
    }
}
fn fail_each(mut op: impl FnMut() -> Result<()>, mut after: impl FnMut()) {
    for ordinal in 0..4096 {
        let start = LIVE.with(Cell::get);
        FAIL.with(|c| c.set(Some(ordinal)));
        let result = op();
        let unused = FAIL.with(|c| c.replace(None));
        if unused.is_some() {
            result.unwrap();
            println!("Allocations injectées {ordinal}");
            return;
        }
        assert!(result.is_err(), "allocation {ordinal}");
        assert_eq!(LIVE.with(Cell::get), start, "fuite à {ordinal}");
        after();
    }
    panic!("nombre inattendu d'allocations");
}
#[test]
fn refus_a_chaque_allocation_et_conservation() {
    let p = policy();
    let mut catalog = Catalog::new(&p);
    let h = catalog.admit(RAW.as_bytes(), SIG, &objects()).unwrap();
    let before = catalog.select(&target("aarch64"), Some(h)).unwrap();
    let raw = RAW.replace("version=[1,0,0]", "version=[2,0,0]");
    let sig = sign(raw.as_bytes(), 1);
    let blobs = objects();
    let t = target("aarch64");
    for ordinal in 0..4096 {
        let start = LIVE.with(Cell::get);
        FAIL.with(|c| c.set(Some(ordinal)));
        let res = catalog.admit(raw.as_bytes(), &sig, &blobs);
        let unused = FAIL.with(|c| c.replace(None));
        if unused.is_some() {
            res.unwrap();
            println!("Allocations injectées admission {ordinal}");
            break;
        }
        assert!(res.is_err(), "{ordinal}");
        assert_eq!(start, LIVE.with(Cell::get), "{ordinal}");
        assert_eq!(catalog.len(), 1);
        assert_eq!(before, catalog.select(&t, None).unwrap());
    }
    assert_eq!(catalog.len(), 2);
    fail_each(
        || catalog.select(&t, None).map(drop),
        || assert_eq!(catalog.len(), 2),
    );
    let record = include_bytes!("../vecteurs/profil.toml");
    let artifact = include_bytes!("../vecteurs/profil.bin");
    let rsig = include_bytes!("../vecteurs/profil.sig");
    let mut pol = Policy::new(1, Origin::Laboratoire, 1, vec![signer()], SYS).unwrap();
    fail_each(|| pol.add_resource(record, artifact, rsig), || {});
}

#[test]
fn budgets_et_refus_repetes() {
    let p = policy();
    for limit in [0, 64, 1024, 8192] {
        let mut c = Catalog::with_decode_limit(&p, limit);
        let start = LIVE.with(Cell::get);
        for _ in 0..64 {
            assert!(c.admit(RAW.as_bytes(), SIG, &objects()).is_err());
            assert!(c.is_empty());
        }
        assert_eq!(LIVE.with(Cell::get), start);
    }
}

#[path = "../examples/support/maximal.rs"]
mod fixture;

#[test]
fn allocation_exacte_et_documents_denses() {
    let p = policy();
    let maximal = fixture::maximal(RAW);
    let newlines = format!("{}{}", RAW, "\n".repeat(MAX_MANIFEST - RAW.len()));
    let escaped = RAW.replace("org.novaos.lab.port", r"org.novaos.lab.\u0070ort");
    for (kind, s) in [
        ("nominal", RAW),
        ("maximal", maximal.as_str()),
        ("lignes", &newlines),
        ("echappe", &escaped),
    ] {
        let sig = sign(s.as_bytes(), 1);
        let accepts = |limit| {
            Catalog::with_decode_limit(&p, limit)
                .admit(s.as_bytes(), &sig, &objects())
                .is_ok()
        };
        assert!(accepts(MAX_DECODE_HEAP));
        let (mut lo, mut hi) = (0, MAX_DECODE_HEAP);
        while lo + 1 < hi {
            let mid = lo + (hi - lo) / 2;
            if accepts(mid) {
                hi = mid;
            } else {
                lo = mid;
            }
        }
        println!("Budget cumulé {kind} {hi}");
        let mut c = Catalog::with_decode_limit(&p, hi);
        for _ in 0..16 {
            let hash = c.admit(s.as_bytes(), &sig, &objects()).unwrap();
            assert_eq!(c.len(), 1);
            assert_eq!(
                c.select(&target("aarch64"), Some(hash))
                    .unwrap()
                    .manifest_digest(),
                hash
            );
        }
        assert!(!accepts(hi - 1));
    }
    let mut c = Catalog::new(&p);
    let h = c.admit(RAW.as_bytes(), SIG, &objects()).unwrap();
    let old = c.select(&target("x86_64"), Some(h)).unwrap();
    let dense = format!("schema=[{}]", "0,".repeat(8000));
    assert_eq!(
        c.admit(dense.as_bytes(), SIG, &objects()),
        Err(Error::Limit)
    );
    assert_eq!(c.select(&target("x86_64"), None).unwrap(), old);
    let bad = format!("#avant\n#interdit{}\n{}", '\u{0001}', RAW);
    assert_eq!(c.admit(bad.as_bytes(), SIG, &objects()), Err(Error::Format));
}

#[test]
fn champs_maximaux_et_transaction_de_refus() {
    let p = policy();
    let mut c = Catalog::new(&p);
    let full = fixture::maximal(RAW);
    let trimmed = full.trim_end();
    let stable = c
        .admit(full.as_bytes(), &sign(full.as_bytes(), 1), &objects())
        .unwrap();
    let old = c.select(&target("aarch64"), Some(stable)).unwrap();
    for s in [
        format!("{trimmed}\n[[match]]\nbus='pci'\nvendor=1\ndevice=1"),
        trimmed.replace("optional_caps=[", "optional_caps=['extra',"),
        trimmed.replace("org.novaos.lab.pp", "org.novaos.lab.xpp"),
        format!(
            "{trimmed}\n[[dependency]]\nid='option.17'\nkind='service'\nmin=[1,0,0]\nbefore=[2,0,0]\nsha256='{}'\nrequired=false",
            "aa".repeat(32)
        ),
        format!("{full} "),
    ] {
        assert!(
            c.admit(s.as_bytes(), &sign(s.as_bytes(), 1), &objects())
                .is_err()
        );
        assert_eq!(c.len(), 1);
        assert_eq!(c.select(&target("aarch64"), None).unwrap(), old);
    }
}
