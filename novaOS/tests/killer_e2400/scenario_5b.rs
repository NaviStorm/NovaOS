//! Scénario commun Darwin et invité Ring 3. Les deux nœuds appartiennent au banc CPU.
use crate::{
    adaptateur::Reception,
    courtier_simule::{Broker, Phy, Register},
    support::*,
};
use ethernet_v1::Binding;
use novaos_gestionnaire_sim::{
    Error, Result,
    catalogue::{Blob, Bus, Catalog},
    dma::{Direction, PageSet},
    e2400::{Permit, Topology},
    inventory::{Address, Node, Ownership, Slot},
    lifecycle::{Event, Lease, Observation, Stamp, State},
    manager::Manager,
    resources::Use,
};

pub fn node(slot: u8, storage: bool) -> Node {
    Node {
        slot: Slot::new(slot).unwrap(),
        appearance: 1,
        controller: 37,
        address: Address::Pci {
            segment: 2,
            bus: 19,
            device: slot,
            function: 0,
            express: true,
            bridge: false,
        },
        identity: if storage {
            Bus::Pci {
                vendor: Some(0x1234),
                device: Some(0x5555),
                subsystem_vendor: None,
                subsystem_device: None,
                revision: Some(1),
                class_base: Some(1),
                subclass: Some(8),
            }
        } else {
            cible().bus
        },
        parent: None,
        needs: 0,
        aliases: 0,
        reset: u16::from(slot) + 1,
        irq_scope: u16::from(slot) + 1,
        ownership: Ownership::Independent,
        provider_trusted: true,
    }
}
pub fn selected(m: &mut Manager<'_, '_>) -> Result<(Stamp, Topology)> {
    let nic = Slot::new(7)?;
    let d = m.discover(nic)?;
    let s = m.select(d, &cible(), None, nic.bit())?;
    let topology = Topology {
        revision: m.inventory().revision(),
        nic: s.attachment,
        storage: m.inventory().attachment(Slot::new(4)?)?,
        aliases_resolved: true,
        iommu_scopes_resolved: true,
        resets_and_irq_isolated: true,
        firmware_released: true,
    };
    Ok((s, topology))
}
pub fn proof(lease: Lease, serial: u64) -> Observation {
    Observation {
        lease,
        serial,
        source_stopped: false,
        stopped_pages: PageSet::default(),
        translations_removed: PageSet::default(),
        ownership_returned: PageSet::default(),
    }
}
fn exchange(
    m: &mut Manager<'_, '_>,
    cap: novaos_gestionnaire_sim::ipc::Capability,
    r: novaos_gestionnaire_sim::ipc::Request,
) -> Result<(novaos_gestionnaire_sim::ipc::Reply, usize)> {
    use novaos_gestionnaire_sim::ipc::{MAX_DATA, Reply};
    let mut buf = [0; MAX_DATA];
    let n = r.encode(&mut buf)?;
    let reply = m.serve(cap, &buf[..n], None)?.encode(r);
    Ok((Reply::decode(r, &reply)?, n + reply.len()))
}
#[derive(Debug)]
pub struct Metrics {
    pub broker_heap: usize,
    pub broker_header: usize,
    pub register_heap: usize,
    pub plan_table: usize,
    pub permit_size: usize,
    pub reservation_and_ack: usize,
    pub replay_bytes: usize,
    pub transfer_cpu_ns: u64,
}
pub fn run(clock: impl Fn() -> u64) -> Result<Metrics> {
    use novaos_gestionnaire_sim::ipc::{Binding as Channel, Body, Op, Request, Role, Status};

    let mut p = politique(false, 3);
    p.add_resource(RECU, PROFIL, SIG_RECU)
        .map_err(Error::Catalogue)?;
    let mut c = Catalog::new(&p);
    c.admit(
        MANIFESTE,
        SIG,
        &[Blob {
            isa: "x86_64",
            bytes: PKG,
        }],
    )
    .map_err(Error::Catalogue)?;
    let mut m = Manager::new(9, &c, 1, Use::LAB, Use::default(), None, 0)?;
    m.refresh(1, true, [node(7, false), node(4, true)])?;
    let (verified, topo) = selected(&mut m)?;
    let idle = m.usage()?;
    let cap = m.open_link(Channel {
        owner: 3,
        peer: 1,
        role: Role::Supervisor,
        group: Some(verified.attachment),
    })?;
    let hello = Request {
        op: Op::Hello,
        epoch: cap.epoch(),
        id: 1,
        revision: 0,
        body: Body::default(),
    };
    assert_eq!(exchange(&mut m, cap, hello)?.0.status, Status::Confirmed);
    let before = m.usage()?;
    let permit = Permit::install(&mut m, verified, topo, 16, 1, 73)?;
    let plan_table = m.plan_slots_bytes();
    assert_eq!(
        Permit::install(&mut m, verified, topo, 16, 1, 73).err(),
        Some(Error::Conflict)
    );
    let req = Request {
        op: Op::Reserve,
        epoch: cap.epoch(),
        id: 2,
        revision: verified.revision,
        body: Body {
            generation: verified.generation,
            a: 16,
            b: 1,
            c: 73,
        },
    };
    let (receipt, wire) = exchange(&mut m, cap, req)?;
    assert_eq!(receipt.status, Status::Confirmed);
    let replay = exchange(&mut m, cap, req)?;
    assert_eq!(replay, (receipt, wire));
    let ack = Request {
        op: Op::Ack,
        id: 3,
        epoch: req.epoch,
        revision: receipt.value,
        body: Body {
            generation: receipt.generation,
            a: req.epoch,
            b: req.id,
            c: 0,
        },
    };
    let acknowledged = exchange(&mut m, cap, ack)?;
    assert_eq!(acknowledged.0, receipt);
    assert_eq!(wire, 152);
    assert_eq!(acknowledged.1, 152);
    let r = m.group(verified.attachment.slot)?.stamp();
    let instance = permit.bind(&m)?;
    let l = instance.lease();
    let register_heap = m.dma_registry_heap();
    assert_eq!(register_heap, 777);
    for i in 0..259 {
        let page = m.dma_page(l, i)?;
        assert_eq!(usize::from(page.frame), i);
        assert_eq!(
            page.direction,
            if i == 2 || i >= 131 {
                Direction::Write
            } else {
                Direction::Read
            }
        );
        assert!(!page.exposed && !page.released);
    }
    let (r, ticket) = m.prepare_launch(r)?;
    let r = m.launch(r, ticket)?;
    let s = m.step(r, Event::Assign)?;
    m.readiness(l, true, true, true)?;
    let s = m.step(s, Event::Initialize)?;
    let active = m.step(s, Event::Publish)?;
    let local = crate::boucle::STATION;
    let peer = crate::boucle::PAIR;
    let phy = Phy {
        oui_msb: 0x1234,
        model_revision: 0xb001,
    };
    let binding = Binding {
        boot: [9; 16],
        port: 3,
        owner: 15,
        incarnation: 2,
        channel: 4,
        epoch: 1,
    };
    assert_eq!(instance.page(&m, l, 2, Direction::Read), Err(Error::Denied));
    let mut b = Broker::open(instance, &m, l, binding, phy, phy)?;
    assert_eq!(b.read(&m, l, Register::PhyId1)?, phy.oui_msb);
    assert_eq!(b.read(&m, l, Register::PhyId2)?, phy.model_revision);
    let mut frame = [0u8; 60];
    frame[..6].copy_from_slice(&peer);
    frame[6..12].copy_from_slice(&local);
    assert_eq!(b.prepare(&m, l, 0, &frame), Err(Error::Denied));
    frame[12..14].copy_from_slice(&[0x88, 0xb5]);
    let mut reply = frame;
    reply[..6].copy_from_slice(&local);
    reply[6..12].copy_from_slice(&peer);
    let start = clock();
    for batch in 0..33u16 {
        assert!(!b.arm(&m, l)?);
        for j in 0..8u16 {
            let idx = (batch * 8 + j) as u8;
            let t = b.prepare(&m, l, j.into(), &frame)?;
            assert_eq!(&t[..8], &[60, 0, 0, 0, 0, 0, 0, 128]);
            assert_eq!(
                u64::from_le_bytes(t[8..].try_into().unwrap()),
                3 * 4096 + u64::from(idx) * 2048
            );
            assert_eq!(
                u64::from_le_bytes(b.post_rx(&m, l)?),
                131 * 4096 + u64::from(idx) * 2048
            );
        }
        assert_eq!(b.post_rx(&m, l), Err(Error::Full));
        assert!(b.tx_image(&m, l, 0).is_err());
        assert_eq!(b.publish(&m, l, 1), Err(Error::Transition));
        b.order_tx(&m, l)?;
        b.publish(&m, l, 3)?;
        for i in 0..3 {
            assert_eq!(b.tx_image(&m, l, i)?.1, frame);
        }
        assert!(b.tx_image(&m, l, 3).is_err());
        let end = b.publish(&m, l, 5)?;
        assert_eq!(b.tx_progress(&m, l, end.into())?, 255);
        assert_eq!(b.tx_progress(&m, l, end.into())?, 0);
        assert!(b.tx_image(&m, l, 0).is_err());
        for j in 0..8u16 {
            let idx = (batch * 8 + j) as u8;
            let mut rrd = [0u8; 16];
            rrd[..4].copy_from_slice(&((u32::from(idx) << 20) | (1 << 16)).to_le_bytes());
            rrd[12..].copy_from_slice(&((1u32 << 31) | 64).to_le_bytes());
            assert_eq!(
                b.rx_progress(&m, l, &rrd, &reply)?,
                Reception::Trame(&reply)
            );
        }
        assert_eq!(b.read(&m, l, Register::IrqPending)?, 1);
        assert!(b.acknowledge(&m, l)?);
        assert!(!b.acknowledge(&m, l)?);
    }
    let transfer_cpu_ns = clock() - start;
    assert_eq!(b.read(&m, l, Register::TxProducer)?, 8);
    assert_eq!(b.read(&m, l, Register::RxConsumer)?, 8);
    assert_eq!(
        b.prepare(
            &m,
            Lease {
                incarnation: 2,
                ..l
            },
            0,
            &frame
        ),
        Err(Error::Stale)
    );
    for coupon in 0..8 {
        b.prepare(&m, l, coupon, &frame)?;
    }
    b.order_tx(&m, l)?;
    b.publish(&m, l, 4)?;
    assert_eq!(b.tx_progress(&m, l, 200), Err(Error::Uncertain));
    let rev = m.step(active, Event::Revoke)?;
    assert!(b.read(&m, l, Register::TxProducer).is_err());
    let stop = m.step(rev, Event::Stop)?;
    let balance = b.stop(&m, l)?;
    assert_eq!((balance.tx_incertains, balance.tx_annules), (4, 4));
    b.reset(&m, l)?;
    assert_eq!(m.group(l.attachment.slot)?.held().dma, 259);
    let rec = m.step(stop, Event::Reconcile)?;
    let q = m.step(rec, Event::Quarantine)?;
    assert_eq!(m.step(q, Event::Reconcile), Err(Error::Uncertain));
    let first = PageSet::range(0, 129)?;
    let all = PageSet::range(0, 259)?;
    let mut event = proof(l, 1);
    event.stopped_pages = first;
    event.ownership_returned = all;
    m.evidence(event)?;
    assert_eq!(m.group(l.attachment.slot)?.held().dma, 259);
    event = proof(l, 2);
    event.translations_removed = first;
    m.evidence(event)?;
    assert_eq!(m.group(l.attachment.slot)?.held().dma, 130);
    assert!(m.dma_page(l, 128)?.released && m.dma_page(l, 129)?.quarantined);
    assert_eq!(m.evidence(event), Err(Error::Stale));
    let mut obsolete = event;
    obsolete.lease.generation += 1;
    obsolete.serial += 1;
    assert_eq!(m.evidence(obsolete), Err(Error::Stale));
    let mut delayed = event;
    delayed.serial = 1;
    delayed.source_stopped = true;
    assert_eq!(m.evidence(delayed), Err(Error::Stale));
    assert_eq!(m.group(l.attachment.slot)?.held().dma, 130);
    event = proof(l, 3);
    event.source_stopped = true;
    event.translations_removed = all;
    m.evidence(event)?;
    let rec = m.step(q, Event::Reconcile)?;
    let returned = m.step(rec, Event::Return)?;
    assert_eq!(m.dma_page(l, 258), Err(Error::Stale));
    assert_eq!(m.group(returned.attachment.slot)?.state(), State::Returned);
    assert_eq!(m.dma_registry_heap(), 0);
    assert_eq!(m.usage()?, before);
    m.close_link(cap)?;
    assert_eq!(m.usage()?, idle);
    Ok(Metrics {
        broker_heap: b.heap_bytes(),
        broker_header: size_of::<Broker>(),
        register_heap,
        plan_table,
        permit_size: size_of::<Permit>(),
        reservation_and_ack: wire + acknowledged.1,
        replay_bytes: replay.1,
        transfer_cpu_ns,
    })
}
