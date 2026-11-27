mod support;
use sha2::{Digest, Sha256};
use support::*;
#[test]
fn deux_groupes_neuf_paires_et_deux_services_deja_a_quatre_racines() {
    let p = lab::policy();
    let c = catalog(&p, 1);
    let base = Use {
        endpoints: 8,
        nodes: 8,
        processes: 2,
        pages: 32,
        boxes: 8,
        ..Use::default()
    };
    let mut m = Manager::new(1, &c, 1, Use::LAB, base, None, 0).unwrap();
    for actor in [6, 7] {
        m.declare_existing_actor(actor, 4, 4).unwrap();
    }
    m.refresh(1, true, [node(0), node(1)]).unwrap();
    let _discovery = m
        .agreed(Binding {
            owner: 2,
            peer: 1,
            role: Role::Discovery,
            group: None,
        })
        .unwrap();
    let mut held = Vec::new();
    for i in 0..2 {
        let d = m.discover(sl(i)).unwrap();
        for (owner, peer, role) in [
            (3, 1, Role::Catalog),
            (4, 1, Role::Supervisor),
            (5, 1, Role::Launcher),
            (1, 6 + i, Role::Service),
        ] {
            m.agreed(Binding {
                owner,
                peer,
                role,
                group: Some(d.attachment),
            })
            .unwrap();
        }
        let v = checked(&mut m, d).unwrap();
        let r = m
            .reserve(v, 16 + u64::from(i), 1 + u64::from(i), 1 + u32::from(i))
            .unwrap();
        activate(&mut m, r);
        held.push(m.group(sl(i)).unwrap().lease().unwrap());
    }
    assert_eq!(m.actor_counts(1), Ok((4, 18)));
    for a in [3, 4, 5] {
        assert_eq!(m.actor_counts(a), Ok((4, 4)))
    }
    for a in [6, 7] {
        assert_eq!(m.actor_counts(a), Ok((4, 6)))
    }
    let u = m.usage().unwrap();
    assert_eq!((u.endpoints, u.nodes, u.boxes), (28, 56, 8));
    let leaf = m.leaf(held[1], Right::Class, "rx").unwrap();
    let s = m.group(sl(1)).unwrap().stamp();
    let old = m.management_epoch();
    assert_eq!(m.restart(old).unwrap().len(), 9);
    m.driver_death(held[0]).unwrap();
    assert_eq!(m.group(sl(1)).unwrap().stamp(), s);
    m.authorize(leaf, 17, 2, Right::Class, "rx").unwrap();
    assert!(m.authorize(leaf, 16, 1, Right::Class, "rx").is_err());
}
#[test]
fn le_ticket_fige_le_plan_complet_et_le_bail() {
    let p = lab::policy();
    let c = catalog(&p, 1);
    let mut m = sim(&c, 2);
    let d = m.discover(sl(0)).unwrap();
    let cap = m
        .agreed(Binding {
            owner: 2,
            peer: 1,
            role: Role::Catalog,
            group: Some(d.attachment),
        })
        .unwrap();
    let v = checked(&mut m, d).unwrap();
    let mut data = Vec::new();
    let mut cursor = 1;
    let mut id = 1;
    loop {
        let mut r = packet(Op::ReadPlan, cap, id, v);
        r.body.a = cursor;
        let reply = send(&mut m, cap, r).unwrap();
        assert_eq!(reply.status, Status::Confirmed);
        cursor = u64::from_le_bytes(reply.digest[..8].try_into().unwrap());
        let n = reply.digest[8] as usize;
        data.extend_from_slice(&reply.digest[9..9 + n]);
        if cursor == 0 {
            break;
        }
        id += 1;
    }
    assert!(data.starts_with(b"NPL1"));
    let expected: [u8; 32] = Sha256::digest(&data).into();
    let r = m.reserve(v, 16, 1, 1).unwrap();
    assert_eq!(m.step(r, Event::Assign), Err(Error::Denied));
    let (prepared, ticket) = m.prepare_launch(r).unwrap();
    assert_eq!(ticket.seal(), expected);
    let other = reserve(&mut m, 1, 2);
    let (other, foreign) = m.prepare_launch(other).unwrap();
    assert_eq!(m.launch(prepared, foreign), Err(Error::Denied));
    assert_eq!(m.launch(other, ticket), Err(Error::Denied));
    let launched = m.launch(prepared, ticket).unwrap();
    assert_eq!(m.step(launched, Event::Cancel), Err(Error::Uncertain));
    let rec = m
        .driver_death(m.group(sl(0)).unwrap().lease().unwrap())
        .unwrap();
    assert_eq!(m.step(rec, Event::Return), Err(Error::Uncertain));
}
#[test]
fn preuves_de_source_traduction_et_propriete_differees() {
    let p = lab::policy();
    let c = catalog(&p, 1);
    let mut m = sim(&c, 1);
    let r = reserve(&mut m, 0, 1);
    let l = m.group(sl(0)).unwrap().lease().unwrap();
    let rec = m.step(r, Event::Uncertain).unwrap();
    let q = m.step(rec, Event::Quarantine).unwrap();
    let mut evidence = proof(l, 1, PAGES);
    evidence.source_stopped = false;
    evidence.ownership_returned = pages(0);
    m.evidence(evidence).unwrap();
    assert_eq!(m.group(sl(0)).unwrap().held().dma, 22);
    evidence.serial = 2;
    evidence.translations_removed = pages(0);
    evidence.ownership_returned = pages(PAGES);
    m.evidence(evidence).unwrap();
    assert_eq!(m.group(sl(0)).unwrap().held().dma, 22);
    evidence.serial = 3;
    evidence.source_stopped = true;
    evidence.ownership_returned = pages(1 << 25);
    assert_eq!(m.evidence(evidence), Err(Error::Format));
    assert_eq!(m.group(sl(0)).unwrap().held().dma, 22);
    evidence.ownership_returned = pages(0);
    m.evidence(evidence).unwrap();
    assert_eq!(m.group(sl(0)).unwrap().held().dma, 0);
    let rec = m.step(q, Event::Reconcile).unwrap();
    m.step(rec, Event::Return).unwrap();
    assert_eq!(m.usage().unwrap(), Use::default());
}
#[test]
fn saturation_attentes_et_file_privee() {
    let p = lab::policy();
    let c = catalog(&p, 1);
    let mut m = sim(&c, 1);
    let r = reserve(&mut m, 0, 1);
    let a = activate(&mut m, r);
    let cap = m
        .agreed(Binding {
            owner: 4,
            peer: 1,
            role: Role::Supervisor,
            group: Some(a.attachment),
        })
        .unwrap();
    let mut bytes = [0; 128];
    for id in 1..=7 {
        let r = packet(Op::Suspend, cap, id, a);
        let n = r.encode(&mut bytes).unwrap();
        assert!(matches!(
            m.submit(cap, &bytes[..n]),
            Ok(Delivery::Pending(_))
        ));
    }
    let r = packet(Op::Suspend, cap, 8, a);
    let n = r.encode(&mut bytes).unwrap();
    assert!(matches!(m.submit(cap, &bytes[..n]), Err(Error::Full)));
    let mut stale = packet(Op::Revoke, cap, 9, a);
    stale.revision -= 1;
    let n = stale.encode(&mut bytes).unwrap();
    assert!(matches!(m.submit(cap, &bytes[..n]), Err(Error::Full)));
    let r = packet(Op::Revoke, cap, 9, a);
    let n = r.encode(&mut bytes).unwrap();
    let p = match m.submit(cap, &bytes[..n]).unwrap() {
        Delivery::Pending(p) => p,
        _ => unreachable!(),
    };
    assert_eq!(m.usage().unwrap().waits, 8);
    assert_eq!(m.complete(cap, p, None).unwrap().status, Status::Confirmed);
}

#[test]
fn huit_controleurs_et_collecte_apres_restitution() {
    let p = lab::policy();
    let c = catalog(&p, 1);
    let mut m = sim(&c, 0);
    let devices = |i: u8| {
        let mut n = node(i);
        n.controller = 1 + u16::from(i);
        n
    };
    m.refresh(2, true, (0..9).map(devices)).unwrap();
    for i in 0..8 {
        reserve(&mut m, i, u64::from(i) + 1);
    }
    let d = m.discover(sl(8)).unwrap();
    let v = checked(&mut m, d).unwrap();
    assert_eq!(m.reserve(v, 24, 9, 9), Err(Error::Full));
    let old = m.group(sl(0)).unwrap().stamp();
    let end = m.step(old, Event::Cancel).unwrap();
    assert_eq!(m.forget(end), Err(Error::Conflict));
    m.refresh(3, true, (1..9).map(devices)).unwrap();
    m.forget(end).unwrap();
    assert_eq!(m.groups().len(), 8);
    assert!(m.step(end, Event::Readmit).is_err());
}
#[test]
fn feuilles_obsoletes_et_quota_de_processus() {
    let p = lab::policy();
    let c = catalog(&p, 1);
    let mut m = Manager::new(
        1,
        &c,
        1,
        Use {
            processes: 2,
            ..Use::LAB
        },
        Use::default(),
        None,
        0,
    )
    .unwrap();
    m.refresh(1, true, [node(0)]).unwrap();
    let d = m.discover(sl(0)).unwrap();
    let a = m
        .agreed(Binding {
            owner: 3,
            peer: 1,
            role: Role::Supervisor,
            group: Some(d.attachment),
        })
        .unwrap();
    let v = checked(&mut m, d).unwrap();
    assert_eq!(m.reserve(v, 16, 1, 1), Err(Error::Full));
    m.close_link(a).unwrap();
    let r = m.reserve(v, 16, 1, 1).unwrap();
    activate(&mut m, r);
    let l = m.group(sl(0)).unwrap().lease().unwrap();
    let leaf = m.leaf(l, Right::Class, "rx").unwrap();
    m.change_catalog(&c, 2, 2).unwrap();
    assert_eq!(
        m.authorize(leaf, 16, 1, Right::Class, "rx"),
        Err(Error::Stale)
    );
}

#[test]
fn dernier_handle_et_derniere_attente_globale() {
    let p = lab::policy();
    let c = catalog(&p, 1);
    let initial = Use {
        pages: 16,
        processes: 1,
        nodes: 31,
        waits: 31,
        ..Use::default()
    };
    let mut m = Manager::new(1, &c, 1, Use::LAB, initial, None, 0).unwrap();
    m.declare_existing_actor(4, 0, 31).unwrap();
    m.refresh(1, true, [node(0), node(1)]).unwrap();
    let d = m.discover(sl(0)).unwrap();
    let too_many = Binding {
        owner: 1,
        peer: 4,
        role: Role::Service,
        group: Some(d.attachment),
    };
    assert_eq!(m.open_link(too_many).unwrap_err(), Error::Full);
    let cap = m
        .agreed(Binding {
            owner: 3,
            peer: 1,
            role: Role::Supervisor,
            group: Some(d.attachment),
        })
        .unwrap();
    let v = checked(&mut m, d).unwrap();
    let r = m.reserve(v, 16, 1, 1).unwrap();
    let a = activate(&mut m, r);
    let d = m.discover(sl(1)).unwrap();
    let v = checked(&mut m, d).unwrap();
    let used = m.usage().unwrap();
    assert_eq!(used.waits, 32);
    assert_eq!(m.reserve(v, 17, 2, 2), Err(Error::Full));
    assert_eq!(m.usage().unwrap(), used);
    assert_eq!(
        send(&mut m, cap, packet(Op::Suspend, cap, 1, a)),
        Err(Error::Full)
    );
    assert_eq!(
        send(&mut m, cap, packet(Op::Revoke, cap, 2, a))
            .unwrap()
            .status,
        Status::Confirmed
    );
}
