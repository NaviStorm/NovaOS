use crate::*;
use alloc::vec;

fn bus(device: u16) -> Bus {
    Bus::Pci {
        vendor: Some(7),
        device: Some(device),
        class_base: Some(2),
        subclass: None,
        subsystem_vendor: None,
        subsystem_device: None,
        revision: None,
    }
}
fn entry(id: &str, version: u32, priority: u16) -> Entry {
    let raw = mesures::banc::RAW
        .replace("org.novaos.lab.port", id)
        .replace(
            "version=[1,0,0]",
            &alloc::format!("version=[{version},0,0]"),
        )
        .replace("0x1af4", "7")
        .replace("0x1041", "15")
        .replace("ethernet", "net")
        .replace("required_features=[\"courtier\"]", "required_features=[]")
        .replace("org.novaos.lab.courtier", "profil")
        .replace("private_pages=32", "private_pages=2")
        .replace("ipc_handles=6", "ipc_handles=2")
        .replace("pending_operations=8", "pending_operations=1")
        .replace("dma_pages=22", "dma_pages=1")
        .replace("queues=2", "queues=1")
        .replace("entries_per_queue=16", "entries_per_queue=8")
        .replace("irq=[\"intx\"]", "irq=[]")
        .replace("max_irq_routes=1", "max_irq_routes=0")
        .replace("mmio=[\"notification\"]", "mmio=[]");
    let raw = raw.replace(
        mesures::banc::RAW
            .split("sha256=\"")
            .last()
            .unwrap()
            .split('"')
            .next()
            .unwrap(),
        &"07".repeat(32),
    );
    let a = raw.find("[[image]]\nisa=\"x86_64\"").unwrap();
    let b = raw[a..].find("[[match]]").unwrap() + a;
    let mut raw = raw;
    raw.replace_range(a..b, "");
    let raw = raw.replace(",\"x86_64\"", "");
    let m: Manifest = schema::decode(raw.as_bytes()).unwrap();
    Entry::new(raw.as_bytes(), &m, [version as u8; 32], 0, priority).unwrap()
}
fn target() -> Target<'static> {
    Target {
        revision: 1,
        profile: Some([7; 32]),
        attachment: "lab-port",
        class: "net",
        bus: bus(15),
        isa: "aarch64",
        os: [0, 20, 0],
        abi: 1,
        features: &[],
        caps: &["rx"],
        dma: &["rx:w"],
        mmio: &[],
        irq: &[],
        allow_class_match: false,
        limits: Limits {
            private_pages: 2,
            roots: 1,
            handles: 2,
            boxes: 0,
            pending: 1,
            dma_pages: 1,
            queues: 1,
            entries: 8,
            irq_routes: 0,
        },
    }
}
fn index<'a>(p: &'a Policy, entries: Vec<Entry>) -> Catalog<'a> {
    let mut entries = entries;
    entries.sort_by(|a, b| a.id().cmp(b.id()).then(a.digest.cmp(&b.digest)));
    Catalog {
        policy: p,
        entries,
        decode_limit: MAX_DECODE_HEAP,
    }
}
#[test]
fn rang_version_conflit_epingle() {
    let p = Policy::empty_test(7);
    let t = target();
    let conflict = index(&p, vec![entry("org.a.net", 1, 2), entry("org.b.net", 2, 2)]);
    assert_eq!(conflict.select(&t, None), Err(Error::Conflict));
    assert_eq!(
        conflict.select(&t, Some([1; 32])).unwrap().driver(),
        "org.a.net"
    );
    assert_eq!(conflict.select(&t, Some([8; 32])), Err(Error::NoMatch));
    let mut results = vec![];
    for reverse in [false, true] {
        let mut es = vec![
            entry("a", 1, 1),
            entry("b", 1, 1),
            entry("c", 2, 2),
            entry("c", 3, 2),
        ];
        if reverse {
            es.reverse();
        }
        results.push(index(&p, es).select(&t, None).unwrap());
    }
    assert_eq!(results[0], results[1]);
    assert_eq!(results[0].version(), [3, 0, 0]);
}
#[test]
fn quotas_caps_versions_et_hardware() {
    let p = Policy::empty_test(7);
    let c = index(&p, vec![entry("org.a.net", 1, 0)]);
    let mut t = target();
    assert_eq!(c.select(&t, None).unwrap().capabilities(), ["rx"]);
    t.caps = &["stats", "rx"];
    assert_eq!(c.select(&t, None).unwrap().capabilities(), ["rx", "stats"]);
    t.limits.roots = 0;
    assert_eq!(c.select(&t, None), Err(Error::Limit));
    t.limits.roots = 1;
    t.dma = &["rx:r"];
    assert_eq!(c.select(&t, None), Err(Error::Denied));
    t.dma = &["rx:w"];
    t.os = [0, 21, 0];
    assert_eq!(c.select(&t, None), Err(Error::NoMatch));
    t.os = [0, 20, 0];
    t.isa = "x86_64";
    assert_eq!(c.select(&t, None), Err(Error::NoMatch));
}
mod mesures;
