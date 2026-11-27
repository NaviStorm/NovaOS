mod support;
use support::*;
#[test]
fn instantanes_bornes_et_deterministes() {
    let mut inv = Inventory::new(27).unwrap();
    for n in 1..=128 {
        inv.refresh(n, true, (0..n as u8).rev().map(node)).unwrap();
        assert_eq!(inv.nodes().len(), n as usize);
        assert_eq!(inv.nodes()[0].slot, sl(0));
    }
    let stable = inv.attachment(sl(0)).unwrap();
    assert_eq!(
        inv.refresh(129, true, (0..128).map(node).chain([node(0)])),
        Err(Error::Conflict)
    );
    assert_eq!(inv.revision(), 128);
    assert!(inv.notify(130).unwrap());
    assert!(!inv.ready());
    assert_eq!(inv.scope(sl(0)), Err(Error::Incomplete));
    inv.refresh(130, false, [node(3)]).unwrap();
    assert!(!inv.ready());
    assert!(inv.refresh(131, true, [node(0)]).is_err());
    let mut replaced = node(0);
    replaced.appearance = 2;
    inv.refresh(131, true, [replaced]).unwrap();
    assert_ne!(inv.attachment(sl(0)).unwrap(), stable);
    assert_eq!(inv.attachment(sl(0)).unwrap().topology, 131);
}
#[test]
fn pont_fonctions_alias_et_portee_de_reset() {
    let p = lab::policy();
    let c = catalog(&p, 1);
    let mut m = sim(&c, 0);
    let mut bridge = node(0);
    bridge.address = Address::Pci {
        segment: 0,
        bus: 0,
        device: 0,
        function: 0,
        express: true,
        bridge: true,
    };
    let mut first = node(1);
    first.parent = Some(sl(0));
    first.aliases = sl(2).bit();
    first.reset = 5;
    let mut sister = node(2);
    sister.parent = Some(sl(0));
    sister.address = Address::Pci {
        segment: 0,
        bus: 0,
        device: 1,
        function: 1,
        express: true,
        bridge: false,
    };
    let mut storage = node(3);
    storage.reset = 5;
    m.refresh(2, true, [sister, bridge, storage, first])
        .unwrap();
    let scope = m.inventory().scope(sl(1)).unwrap();
    assert_eq!(scope.members, 6);
    assert_eq!(scope.reset_peers, 8);
    assert_eq!(scope.dependencies, 1);
    assert_eq!(scope, m.inventory().scope(sl(2)).unwrap());
    let a = m.discover(sl(1)).unwrap();
    let mut t = lab::target("aarch64");
    t.revision = 2;
    t.attachment = "dev-001";
    assert_eq!(m.select(a, &t, None, sl(1).bit()), Err(Error::Denied));
    let s = checked(&mut m, a).unwrap();
    assert_eq!(m.reserve(s, 17, 1, 2), Err(Error::Denied));
    let bridge = reserve(&mut m, 0, 1);
    let bridge = activate(&mut m, bridge);
    let storage = reserve(&mut m, 3, 2);
    let storage = activate(&mut m, storage);
    assert_eq!(m.reserve(s, 17, 3, 2), Err(Error::Conflict));
    release(&mut m, storage);
    let child = m.reserve(s, 17, 3, 2).unwrap();
    activate(&mut m, child);
    let sibling = m.discover(sl(2)).unwrap();
    let sibling = checked(&mut m, sibling).unwrap();
    assert_eq!(m.reserve(sibling, 18, 4, 3), Err(Error::Conflict));
    assert_eq!(m.step(bridge, Event::Suspend), Err(Error::Conflict));
    let l = m.group(sl(0)).unwrap().lease().unwrap();
    m.driver_death(l).unwrap();
    assert_eq!(m.group(sl(1)).unwrap().state(), State::Revoking);
}
fn usb(i: u8, config: u8) -> Node {
    let mut n = node(i);
    n.ownership = Ownership::Parent;
    n.parent = Some(sl(0));
    n.address = Address::Usb {
        port: 1,
        configuration: config,
        interface: Some(i),
    };
    n.identity = Bus::Usb {
        vendor: Some(1),
        product: Some(2),
        configuration: Some(config),
        interface: Some(i),
        class_base: Some(3),
        subclass: None,
        protocol: None,
        parent: Some("dev-000".into()),
    };
    n
}
#[test]
fn usb_composite_reconfiguration_et_retrait() {
    let p = lab::policy();
    let c = catalog(&p, 1);
    let mut m = sim(&c, 0);
    m.refresh(2, true, [node(0), usb(1, 1), usb(2, 1)]).unwrap();
    let s = reserve(&mut m, 0, 1);
    activate(&mut m, s);
    let l = m.group(sl(0)).unwrap().lease().unwrap();
    let old = m.mediated(sl(1), l).unwrap();
    assert_eq!(m.inventory().scope(sl(2)), Err(Error::Denied));
    assert!(m.mediated(sl(2), l).is_ok());
    let mut new = usb(1, 2);
    new.appearance = 2;
    let mut other = usb(2, 2);
    other.appearance = 2;
    m.refresh(3, true, [node(0), new, other]).unwrap();
    assert_eq!(m.use_mediated(old, l), Err(Error::Stale));
    assert_ne!(m.mediated(sl(1), l).unwrap(), old);
    m.refresh(4, true, []).unwrap();
    assert!(m.mediated(sl(1), l).is_err());
    assert_eq!(m.group(sl(0)).unwrap().state(), State::Removed);
    let gone = m.group(sl(0)).unwrap().stamp();
    let rec = m.step(gone, Event::Reconcile).unwrap();
    m.evidence(proof(l, 1, PAGES)).unwrap();
    let end = m.step(rec, Event::Return).unwrap();
    assert!(m.step(end, Event::Readmit).is_err());
    let mut back = node(0);
    back.appearance = 3;
    m.refresh(5, true, [back]).unwrap();
    let again = m.step(end, Event::Readmit).unwrap();
    let d = m.step(again, Event::Rediscover).unwrap();
    assert_ne!(d.attachment, l.attachment);
}
#[test]
fn plateformes_virtuelles_parents_et_vues_incompletes() {
    let mut inv = Inventory::new(1).unwrap();
    let mut a = node(0);
    a.address = Address::Acpi(0x45);
    a.identity = Bus::Platform {
        provider: "acpi".into(),
        firmware_id: "NVA01".into(),
    };
    let mut dt = node(1);
    dt.address = Address::DeviceTree(2);
    dt.parent = Some(sl(0));
    dt.identity = Bus::Platform {
        provider: "dt".into(),
        firmware_id: "nova.bus".into(),
    };
    let mut v = node(2);
    v.address = Address::Virtual {
        provider: 7,
        instance: 3,
    };
    v.identity = Bus::Platform {
        provider: "lab".into(),
        firmware_id: "vport".into(),
    };
    v.provider_trusted = false;
    inv.refresh(1, true, [v, dt, a]).unwrap();
    assert_eq!(inv.scope(sl(1)).unwrap().dependencies, 1);
    assert_eq!(inv.scope(sl(2)), Err(Error::Denied));
    let mut missing = node(3);
    missing.parent = Some(sl(4));
    assert_eq!(inv.refresh(2, true, [missing]), Err(Error::Incomplete));
    let mut a = node(3);
    a.needs = sl(4).bit();
    let mut b = node(4);
    b.needs = sl(3).bit();
    assert_eq!(inv.refresh(2, true, [a, b]), Err(Error::Conflict));
    assert_eq!(inv.revision(), 1);
}
#[test]
fn trente_deux_groupes_et_soixante_quatre_catalogues() {
    let p = lab::policy();
    let mut c = Catalog::new(&p);
    for n in 1..=64 {
        let raw = lab::RAW.replace("version=[1,0,0]", &format!("version=[1,0,{n}]"));
        c.admit(
            raw.as_bytes(),
            &lab::sign(raw.as_bytes(), 1),
            &lab::objects(),
        )
        .unwrap();
        let mut m = sim(&c, 33);
        let a = m.discover(sl(0)).unwrap();
        checked(&mut m, a).unwrap();
        assert_eq!(m.plan(sl(0)).unwrap().version(), [1, 0, n]);
        for i in 1..32 {
            m.discover(sl(i)).unwrap();
            assert_eq!(m.groups().len(), usize::from(i) + 1)
        }
        assert_eq!(m.discover(sl(32)), Err(Error::Full));
    }
}

fn signed_variant(c: &mut Catalog<'_>, rule: &str, id: &str, logical: bool) {
    let mut s = lab::RAW
        .replace("org.novaos.lab.port", &format!("org.novaos.lab.{id}"))
        .replace("bus=\"pci\"\nvendor=0x1af4\ndevice=0x1041", rule);
    if logical {
        for (key, new) in [
            ("dma=[\"rx:w\"]", "dma=[]"),
            ("mmio=[\"notification\"]", "mmio=[]"),
            ("irq=[\"intx\"]", "irq=[]"),
            ("max_irq_routes=1", "max_irq_routes=0"),
            ("dma_pages=22", "dma_pages=0"),
            ("queues=2", "queues=0"),
            ("entries_per_queue=16", "entries_per_queue=0"),
        ] {
            s = s.replace(key, new);
        }
    }
    c.admit(s.as_bytes(), &lab::sign(s.as_bytes(), 1), &lab::objects())
        .unwrap();
}
#[test]
fn selection_signee_des_interfaces_et_des_fournisseurs() {
    let p = lab::policy();
    let mut c = catalog(&p, 1);
    signed_variant(
        &mut c,
        "bus='usb'\nvendor=1\nproduct=2\nconfiguration=1\ninterface=1\nparent='dev-000'",
        "usb",
        true,
    );
    for provider in ["acpi", "dt", "lab"] {
        signed_variant(
            &mut c,
            &format!("bus='platform'\nprovider='{provider}'\nfirmware_id='NVA01'"),
            provider,
            false,
        );
    }
    let mut m = sim(&c, 0);
    let platform = |i, provider: &str, address| {
        let mut n = node(i);
        n.address = address;
        n.identity = Bus::Platform {
            provider: provider.into(),
            firmware_id: "NVA01".into(),
        };
        n
    };
    m.refresh(
        2,
        true,
        [
            node(0),
            usb(1, 1),
            platform(2, "acpi", Address::Acpi(7)),
            platform(3, "dt", Address::DeviceTree(9)),
            platform(
                4,
                "lab",
                Address::Virtual {
                    provider: 2,
                    instance: 1,
                },
            ),
        ],
    )
    .unwrap();
    let r = reserve(&mut m, 0, 1);
    activate(&mut m, r);
    let parent = m.group(sl(0)).unwrap().lease().unwrap();
    let d = m.discover(sl(1)).unwrap();
    let mut t = lab::target("aarch64");
    t.attachment = "dev-001";
    t.revision = 2;
    t.bus = usb(1, 1).identity;
    let v = m.select(d, &t, None, sl(1).bit()).unwrap();
    let leaf = m.bind_child(v, parent, 20, 9).unwrap();
    assert!(m.authorize_child(leaf, 20, 9, "rx").is_ok());
    assert!(m.authorize_child(leaf, 21, 9, "rx").is_err());
    assert!(m.authorize_child(leaf, 20, 9, "admin").is_err());
    assert_eq!(m.reserve(v, 17, 2, 2), Err(Error::Denied));
    assert_eq!(m.usage().unwrap().dma, 22);
    for (i, provider) in [(2, "acpi"), (3, "dt"), (4, "lab")] {
        let d = m.discover(sl(i)).unwrap();
        let label = sl(i).label();
        let mut t = lab::target("aarch64");
        t.revision = 2;
        t.attachment = std::str::from_utf8(&label).unwrap();
        t.bus = Bus::Platform {
            provider: provider.into(),
            firmware_id: "NVA01".into(),
        };
        let v = m.select(d, &t, None, sl(i).bit()).unwrap();
        assert!(m.plan(sl(i)).unwrap().driver().ends_with(provider));
        m.step(v, Event::Invalidate).unwrap();
    }
    m.change_catalog(&c, 2, 1).unwrap();
    assert_eq!(m.mediated(sl(1), parent), Err(Error::Stale));
    let again = m.step(v, Event::Invalidate).unwrap();
    assert_eq!(m.select(again, &t, None, sl(1).bit()), Err(Error::Stale));
    m.driver_death(parent).unwrap();
    assert!(m.authorize_child(leaf, 20, 9, "rx").is_err());
}

#[test]
fn meme_plan_et_refus_sur_deux_ordres_inventaire_catalogue() {
    let policy = lab::policy();
    let mut seals = Vec::new();
    for reversed in [false, true] {
        let mut catalogue = Catalog::new(&policy);
        for version in if reversed { [2, 1] } else { [1, 2] } {
            let raw = lab::RAW.replace("version=[1,0,0]", &format!("version=[1,{version},0]"));
            catalogue
                .admit(
                    raw.as_bytes(),
                    &lab::sign(raw.as_bytes(), 1),
                    &lab::objects(),
                )
                .unwrap();
        }
        let mut manager = sim(&catalogue, 0);
        let mut devices = [node(0), node(1)];
        if reversed {
            devices.reverse();
        }
        manager.refresh(2, true, devices).unwrap();
        let d = manager.discover(sl(0)).unwrap();
        let mut target = lab::target("aarch64");
        target.attachment = "dev-000";
        target.revision = 2;
        assert!(manager.select(d, &target, Some([0; 32]), 1).is_err());
        assert!(manager.plan(sl(0)).is_none());
        let v = manager.select(d, &target, None, 1).unwrap();
        assert_eq!(manager.plan(sl(0)).unwrap().version(), [1, 2, 0]);
        let r = manager.reserve(v, 16, 1, 1).unwrap();
        seals.push(manager.prepare_launch(r).unwrap().1.seal());
    }
    assert_eq!(seals[0], seals[1]);
}
