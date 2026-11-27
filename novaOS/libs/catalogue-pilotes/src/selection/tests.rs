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
    Entry {
        digest: [version as u8; 32],
        priority,
        manifest: Manifest {
            schema: "novaos.driver/1".into(),
            origin: Origin::Laboratoire,
            publisher: "org".into(),
            signing_key_id: "lab".into(),
            security_epoch: 1,
            elf_package_id: [1; 16],
            elf_package_revision: 1,
            provides: vec!["net/1.0".into()],
            id: id.into(),
            class: "net".into(),
            version: [version, 0, 0],
            compat: Compat {
                architectures: vec!["aarch64".into()],
                abi_major: 1,
                os_min: [0, 20, 0],
                os_before: [0, 21, 0],
                required_features: vec![],
            },
            matches: vec![bus(15)],
            image: vec![Image {
                member: "pilote".into(),
                isa: "aarch64".into(),
                package_sha256: [3; 32],
                elf_sha256: [4; 32],
            }],
            required_caps: vec!["rx".into()],
            optional_caps: vec!["stats".into()],
            dependency: vec![Dependency {
                id: "profil".into(),
                kind: Kind::SystemProfile,
                min: [1, 0, 0],
                before: [2, 0, 0],
                sha256: [7; 32],
                required: true,
            }],
            hardware: Hardware {
                dma: vec!["rx:w".into()],
                mmio: vec![],
                irq: vec![],
                queues: 1,
                entries_per_queue: 8,
                max_irq_routes: 0,
                dma_pages: 1,
            },
            quotas: Quotas {
                private_pages: 2,
                ipc_roots_created: 1,
                ipc_handles: 2,
                ipc_boxes_created: 0,
                pending_operations: 1,
            },
        },
    }
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
    entries.sort_by(|a, b| {
        a.manifest
            .id
            .cmp(&b.manifest.id)
            .then(a.manifest.version.cmp(&b.manifest.version))
    });
    Catalog { policy: p, entries }
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
