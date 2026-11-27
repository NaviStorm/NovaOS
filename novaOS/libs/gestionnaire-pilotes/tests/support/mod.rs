#![allow(dead_code, unused_imports)]
pub use novaos_gestionnaire_sim::{
    Error,
    catalogue::{Bus, Catalog, Plan, Policy, Target},
    inventory::*,
    ipc::*,
    lifecycle::*,
    manager::*,
    resources::Use,
    rights::*,
};
pub mod lab {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../catalogue-pilotes/examples/support/banc.rs"
    ));
}
pub fn catalog(p: &Policy, n: usize) -> Catalog<'_> {
    let mut c = Catalog::new(p);
    for i in 0..n {
        let raw = lab::RAW.replace("version=[1,0,0]", &format!("version=[1,{i},0]"));
        c.admit(
            raw.as_bytes(),
            &lab::sign(raw.as_bytes(), 1),
            &lab::objects(),
        )
        .unwrap();
    }
    c
}
pub fn sl(i: u8) -> Slot {
    Slot::new(i).unwrap()
}
pub fn node(i: u8) -> Node {
    Node {
        slot: sl(i),
        appearance: 1,
        controller: 1,
        address: Address::Pci {
            segment: 0,
            bus: i / 32,
            device: i % 32,
            function: 0,
            express: true,
            bridge: false,
        },
        identity: lab::target("aarch64").bus,
        parent: None,
        needs: 0,
        aliases: 0,
        reset: 0,
        irq_scope: 0,
        ownership: Ownership::Independent,
        provider_trusted: true,
    }
}
pub fn sim<'c, 'p>(c: &'c Catalog<'p>, n: u8) -> Manager<'c, 'p> {
    let mut m = Manager::new(1, c, 1, Use::LAB, Use::default(), None, 0).unwrap();
    m.refresh(1, true, (0..n).map(node)).unwrap();
    m
}
pub fn checked(m: &mut Manager<'_, '_>, s: Stamp) -> novaos_gestionnaire_sim::Result<Stamp> {
    let label = s.attachment.slot.label();
    let mut t = lab::target(if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else {
        "x86_64"
    });
    t.revision = m.inventory().revision();
    t.attachment = std::str::from_utf8(&label).unwrap();
    let mask = m.inventory().scope(s.attachment.slot)?.members;
    m.select(s, &t, None, mask)
}
pub fn reserve(m: &mut Manager<'_, '_>, i: u8, inc: u64) -> Stamp {
    let s = m.discover(sl(i)).unwrap();
    let s = checked(m, s).unwrap();
    m.reserve(s, 16 + u64::from(i), inc, 1 + u32::from(i))
        .unwrap()
}
pub fn activate(m: &mut Manager<'_, '_>, r: Stamp) -> Stamp {
    let (s, ticket) = m.prepare_launch(r).unwrap();
    let r = m.launch(s, ticket).unwrap();
    let a = m.step(r, Event::Assign).unwrap();
    let l = m.group(a.attachment.slot).unwrap().lease().unwrap();
    m.readiness(l, true, true, true).unwrap();
    let i = m.step(a, Event::Initialize).unwrap();
    m.step(i, Event::Publish).unwrap()
}
pub fn pages(bits: u128) -> novaos_gestionnaire_sim::dma::PageSet {
    let count = bits.count_ones() as usize;
    let first = if bits == 0 {
        0
    } else {
        bits.trailing_zeros() as usize
    };
    assert!(count == 128 || (bits >> first) == (1u128 << count) - 1);
    novaos_gestionnaire_sim::dma::PageSet::range(first, count).unwrap()
}
pub fn proof(l: Lease, serial: u64, mask: u128) -> Observation {
    Observation {
        lease: l,
        serial,
        source_stopped: true,
        stopped_pages: Default::default(),
        translations_removed: pages(mask),
        ownership_returned: pages(mask),
    }
}
pub const PAGES: u128 = (1 << 22) - 1;
pub fn release(m: &mut Manager<'_, '_>, s: Stamp) -> Stamp {
    let l = m.group(s.attachment.slot).unwrap().lease().unwrap();
    let r = m.step(s, Event::Revoke).unwrap();
    let stop = m.step(r, Event::Stop).unwrap();
    let rec = m.step(stop, Event::Reconcile).unwrap();
    m.evidence(proof(l, 1, PAGES)).unwrap();
    m.step(rec, Event::Return).unwrap()
}
pub fn packet(op: Op, c: Capability, id: u64, s: Stamp) -> Request {
    Request {
        op,
        epoch: c.epoch(),
        id: id + 1,
        revision: s.revision,
        body: Body {
            generation: s.generation,
            ..Body::default()
        },
    }
}
pub fn send(
    m: &mut Manager<'_, '_>,
    cap: Capability,
    r: Request,
) -> novaos_gestionnaire_sim::Result<Reply> {
    let mut b = [0; MAX_DATA];
    let n = r.encode(&mut b)?;
    m.serve(cap, &b[..n], None)
}

/// La première demande de chaque époque est HELLO (ID 1).
pub fn hello(m: &mut Manager<'_, '_>, cap: Capability) {
    let request = Request {
        op: Op::Hello,
        epoch: cap.epoch(),
        id: 1,
        revision: 0,
        body: Body::default(),
    };
    assert_eq!(send(m, cap, request), Ok(Reply::confirmed(1, 0)));
}
pub trait Agreed {
    fn agreed(&mut self, binding: Binding) -> novaos_gestionnaire_sim::Result<Capability>;
}
impl Agreed for Manager<'_, '_> {
    fn agreed(&mut self, binding: Binding) -> novaos_gestionnaire_sim::Result<Capability> {
        let cap = self.open_link(binding)?;
        hello(self, cap);
        Ok(cap)
    }
}
