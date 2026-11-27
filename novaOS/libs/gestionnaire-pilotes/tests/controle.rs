mod support;
use support::*;
fn link(m: &mut Manager<'_, '_>, role: Role, owner: u8, group: Option<Attachment>) -> Capability {
    m.agreed(Binding {
        owner,
        peer: 1,
        role,
        group,
    })
    .unwrap()
}
fn deferred(m: &mut Manager<'_, '_>, cap: Capability, r: Request) -> Pending {
    let mut b = [0; 128];
    let n = r.encode(&mut b).unwrap();
    match m.submit(cap, &b[..n]).unwrap() {
        Delivery::Pending(p) => p,
        Delivery::Reply(_) => panic!("reçu déjà présent"),
    }
}
fn receipt(c: Capability, r: Request, id: u64, generation: u64) -> Request {
    Request {
        op: Op::Receipt,
        epoch: c.epoch(),
        id: id + 1,
        revision: 0,
        body: Body {
            generation,
            a: r.epoch,
            b: r.id,
            c: 0,
        },
    }
}
#[test]
fn vecteurs_data_et_refus_avant_effet() {
    let raw = include_bytes!("../vecteurs/reserver.data");
    let r = Request::decode(raw).unwrap();
    assert_eq!(r.op, Op::Reserve);
    assert_eq!(r.epoch, 0x0102030405060708);
    assert_eq!(r.id, 13);
    assert_eq!(r.revision, 7);
    assert_eq!(
        r.body,
        Body {
            generation: 0,
            a: 17,
            b: 42,
            c: 3
        }
    );
    let mut out = [0; 128];
    assert_eq!(r.encode(&mut out).unwrap(), 64);
    assert_eq!(&out[..64], raw);
    let rep = Reply::confirmed(8, 1);
    assert_eq!(rep.encode(r), *include_bytes!("../vecteurs/confirme.data"));
    assert_eq!(Reply::decode(r, &rep.encode(r)), Ok(rep));
    for n in 0..64 {
        assert!(Request::decode(&raw[..n]).is_err())
    }
    for i in [0, 2, 4, 6, 60, 63] {
        let mut v = *raw;
        v[i] ^= 0x80;
        assert!(Request::decode(&v).is_err())
    }
    for i in [8, 16, 24] {
        let mut v = *raw;
        v[i..i + 8].fill(0);
        assert!(Request::decode(&v).is_err())
    }
    assert!(Request::decode(&[0; 129]).is_err());
    let mut wrong = rep.encode(r);
    wrong[8] ^= 1;
    assert!(Reply::decode(r, &wrong).is_err());
    let p = lab::policy();
    let c = catalog(&p, 1);
    let mut m = sim(&c, 1);
    let s = m.discover(sl(0)).unwrap();
    let cap = m
        .open_link(Binding {
            owner: 2,
            peer: 1,
            role: Role::Supervisor,
            group: Some(s.attachment),
        })
        .unwrap();
    assert_eq!(
        send(&mut m, cap, packet(Op::State, cap, 1, s)),
        Err(Error::Denied)
    );
    let old = m.usage().unwrap();
    assert_eq!(
        send(&mut m, cap, packet(Op::Publish, cap, 1, s)),
        Err(Error::Denied)
    );
    assert_eq!(m.usage().unwrap(), old);
    let hello = Request {
        op: Op::Hello,
        epoch: cap.epoch(),
        id: 2,
        revision: 0,
        body: Body::default(),
    };
    assert_eq!(hello.encode(&mut out).unwrap(), 32);
    assert_eq!(send(&mut m, cap, hello).unwrap().value, 1);
}
#[test]
fn selection_lancement_et_publication_par_data() {
    let p = lab::policy();
    let c = catalog(&p, 1);
    let mut m = sim(&c, 1);
    let d = m.discover(sl(0)).unwrap();
    let cat = link(&mut m, Role::Catalog, 2, Some(d.attachment));
    let sup = link(&mut m, Role::Supervisor, 3, Some(d.attachment));
    let launch = link(&mut m, Role::Launcher, 4, Some(d.attachment));
    let svc = link(&mut m, Role::Service, 5, Some(d.attachment));
    let mut target = lab::target("aarch64");
    target.attachment = "dev-000";
    target.revision = 1;
    let r = packet(Op::Resolve, cat, 1, d);
    let mut b = [0; 128];
    let n = r.encode(&mut b).unwrap();
    let v = m
        .serve(
            cat,
            &b[..n],
            Some(Selection {
                target: &target,
                pin: None,
                approved_members: 1,
            }),
        )
        .unwrap();
    assert_eq!(v.status, Status::Confirmed);
    let s = m.group(sl(0)).unwrap().stamp();
    let digest = send(&mut m, cat, packet(Op::ReadPlan, cat, 2, s))
        .unwrap()
        .digest;
    assert_eq!(digest, m.plan(sl(0)).unwrap().manifest_digest());
    let mut r = packet(Op::Reserve, sup, 1, s);
    r.body = Body {
        generation: 0,
        a: 16,
        b: 1,
        c: 7,
    };
    assert_eq!(send(&mut m, sup, r).unwrap().status, Status::Confirmed);
    for (cap, op, id) in [
        (launch, Op::Prepare, 1),
        (launch, Op::Launch, 2),
        (sup, Op::Assign, 2),
    ] {
        let s = m.group(sl(0)).unwrap().stamp();
        let outcome = send(&mut m, cap, packet(op, cap, id, s)).unwrap();
        assert_eq!(outcome.status, Status::Confirmed, "{op:?}");
    }
    let l = m.group(sl(0)).unwrap().lease().unwrap();
    m.readiness(l, true, false, false).unwrap();
    let s = m.group(sl(0)).unwrap().stamp();
    let i = m.step(s, Event::Initialize).unwrap();
    assert_eq!(
        send(&mut m, svc, packet(Op::PreparePort, svc, 1, i))
            .unwrap()
            .status,
        Status::Confirmed
    );
    let s = m.group(sl(0)).unwrap().stamp();
    let r = packet(Op::Publish, svc, 2, s);
    let a = send(&mut m, svc, r).unwrap();
    assert_eq!(a.status, Status::Confirmed);
    assert_eq!(send(&mut m, svc, r), Ok(a));
    let s = m.group(sl(0)).unwrap().stamp();
    assert_eq!(s.revision, a.value);
    m.readiness(l, true, true, true).unwrap();
    assert_eq!(
        send(&mut m, sup, packet(Op::Suspend, sup, 3, s))
            .unwrap()
            .status,
        Status::Confirmed
    );
    let s = m.group(sl(0)).unwrap().stamp();
    assert_eq!(
        send(&mut m, svc, packet(Op::Close, svc, 3, s))
            .unwrap()
            .status,
        Status::Confirmed
    );
    let s = m.group(sl(0)).unwrap().stamp();
    let stop = m.step(s, Event::Stop).unwrap();
    assert_eq!(
        send(&mut m, sup, packet(Op::Reconcile, sup, 4, stop))
            .unwrap()
            .status,
        Status::Confirmed
    );
    let state = m.group(sl(0)).unwrap().stamp();
    assert_eq!(
        send(&mut m, sup, packet(Op::State, sup, 5, state))
            .unwrap()
            .digest[0],
        State::Reconciling as u8
    );
}
#[test]
fn publications_differees_reprise_et_consommation_unique() {
    let p = lab::policy();
    let c = catalog(&p, 1);
    let mut m = sim(&c, 1);
    let d = m.discover(sl(0)).unwrap();
    let s = checked(&mut m, d).unwrap();
    let binding = Binding {
        owner: 2,
        peer: 1,
        role: Role::Supervisor,
        group: Some(d.attachment),
    };
    let cap = m.agreed(binding).unwrap();
    let other = link(&mut m, Role::Service, 3, Some(d.attachment));
    let mut r = packet(Op::Reserve, cap, 1, s);
    r.body = Body {
        generation: 0,
        a: 16,
        b: 1,
        c: 1,
    };
    let pending = deferred(&mut m, cap, r);
    let mut competing = r;
    competing.id = r.id + 1;
    competing.body.a = 17;
    competing.body.b = 2;
    let race = deferred(&mut m, cap, competing);
    assert_eq!(send(&mut m, cap, r).unwrap().status, Status::Pending);
    assert_eq!(m.usage().unwrap().dma, 0);
    let mut conflict = r;
    conflict.body.c = 3;
    assert_eq!(send(&mut m, cap, conflict), Err(Error::Conflict));
    let generation = m.management_epoch();
    let caps = m.restart(generation).unwrap();
    assert!(m.restart(generation).is_err());
    assert_eq!(m.complete(cap, pending, None), Err(Error::Stale));
    let new = caps.iter().find(|(b, _)| *b == binding).unwrap().1;
    let other_new = caps.iter().find(|(b, _)| b.owner == 3).unwrap().1;
    assert_eq!(m.complete(new, pending, None), Err(Error::Denied));
    hello(&mut m, new);
    hello(&mut m, other_new);
    assert_eq!(m.complete(other_new, pending, None), Err(Error::Denied));
    assert!(send(&mut m, other, receipt(other, r, 3, 0)).is_err());
    let first = m.complete(new, pending, None).unwrap();
    assert_eq!(first.status, Status::Confirmed);
    assert_eq!(m.complete(new, pending, None), Ok(first));
    assert_eq!(m.complete(new, race, None).unwrap().status, Status::Stale);
    let fetch = receipt(new, r, 1, 0);
    assert_eq!(send(&mut m, new, fetch), Ok(first));
    assert_eq!(
        send(
            &mut m,
            other_new,
            receipt(other_new, r, 1, first.generation)
        ),
        Err(Error::Denied)
    );
    let mut ack = receipt(new, r, 2, first.generation);
    ack.op = Op::Ack;
    ack.revision = first.value;
    assert_eq!(send(&mut m, new, ack), Ok(first));
    assert_eq!(send(&mut m, new, ack), Ok(first));
    assert_eq!(
        send(&mut m, new, receipt(new, r, 3, first.generation))
            .unwrap()
            .status,
        Status::Uncertain
    );
    assert_eq!(m.complete(new, pending, None), Err(Error::Stale));
    assert_eq!(m.usage().unwrap().dma, 22);
}
#[test]
fn credits_cloture_et_liaisons_independantes() {
    let p = lab::policy();
    let c = catalog(&p, 1);
    let cap = Use {
        receipts: 2,
        ..Use::LAB
    };
    let baseline = Use {
        boxes: 8,
        endpoints: 4,
        nodes: 4,
        processes: 1,
        pages: 16,
        ..Use::default()
    };
    let mut m = Manager::new(11, &c, 1, cap, baseline, None, 0).unwrap();
    m.declare_existing_actor(2, 4, 4).unwrap();
    m.refresh(1, true, (0..2).map(node)).unwrap();
    let d = m.discover(sl(0)).unwrap();
    let bind = Binding {
        owner: 2,
        peer: 1,
        role: Role::Supervisor,
        group: Some(d.attachment),
    };
    assert!(m.agreed(bind).is_err());
    let cap = link(&mut m, Role::Supervisor, 3, Some(d.attachment));
    let v = checked(&mut m, d).unwrap();
    let mut r = packet(Op::Reserve, cap, 1, v);
    r.body = Body {
        generation: 0,
        a: 16,
        b: 1,
        c: 1,
    };
    assert_eq!(send(&mut m, cap, r).unwrap().status, Status::Confirmed);
    let s = m.group(sl(0)).unwrap().stamp();
    let a = activate(&mut m, s);
    let refused = packet(Op::Suspend, cap, 2, a);
    assert_eq!(send(&mut m, cap, refused), Err(Error::Full));
    let close = packet(Op::Revoke, cap, 3, a);
    assert_eq!(send(&mut m, cap, close).unwrap().status, Status::Confirmed);
    assert_eq!(m.usage().unwrap().receipts, 2);
    assert_eq!(m.usage().unwrap().boxes, 8);
    let st = m.group(sl(0)).unwrap().stamp();
    let other = m.discover(sl(1)).unwrap();
    let second = m.agreed(Binding {
        owner: 3,
        peer: 1,
        role: Role::Supervisor,
        group: Some(other.attachment),
    });
    // Les deux racines restantes peuvent accueillir une paire DATA indépendante.
    let second = second.unwrap();
    assert_ne!(second.epoch(), cap.epoch());
    assert_eq!(m.actor_counts(3).unwrap().0, 4);
    assert!(
        m.agreed(Binding {
            owner: 3,
            peer: 1,
            role: Role::Launcher,
            group: Some(st.attachment)
        })
        .is_err()
    );
    m.close_link(second).unwrap();
}
#[test]
fn enumeration_revisionnee_et_annulation() {
    let p = lab::policy();
    let c = catalog(&p, 1);
    let mut m = sim(&c, 2);
    let d = m.discover(sl(0)).unwrap();
    let disc = link(&mut m, Role::Discovery, 2, None);
    let mut r = packet(Op::Enumerate, disc, 1, d);
    r.revision = 0;
    let first = send(&mut m, disc, r).unwrap();
    assert_eq!(first.digest[0], 0);
    r.id = 3;
    r.body.a = 1;
    r.revision = first.value;
    assert_eq!(send(&mut m, disc, r).unwrap().digest[0], 1);
    let mut change = packet(Op::Change, disc, 3, d);
    change.revision = 1;
    change.body.a = 3;
    assert_eq!(send(&mut m, disc, change).unwrap().digest[0], 1);
    r.id = 5;
    assert_eq!(send(&mut m, disc, r).unwrap().status, Status::Refused);
    let mut read = packet(Op::Reread, disc, 5, d);
    read.revision = 0;
    assert_eq!(send(&mut m, disc, read).unwrap().value, 1);
    m.refresh(3, true, (0..2).map(node)).unwrap();
    r.id = 7;
    assert_eq!(send(&mut m, disc, r).unwrap().status, Status::Stale);
    let v = checked(&mut m, d).unwrap();
    let s = m.reserve(v, 16, 1, 1).unwrap();
    let launcher = link(&mut m, Role::Launcher, 3, Some(d.attachment));
    assert_eq!(
        send(&mut m, launcher, packet(Op::Cancel, launcher, 1, s))
            .unwrap()
            .status,
        Status::Confirmed
    );
    assert_eq!(m.group(sl(0)).unwrap().held(), Use::default());
}

#[test]
fn saturation_des_soixante_quatre_recus_et_retrait() {
    let p = lab::policy();
    let c = catalog(&p, 1);
    let mut m = sim(&c, 1);
    let r = reserve(&mut m, 0, 1);
    let a = activate(&mut m, r);
    let cap = link(&mut m, Role::Supervisor, 3, Some(a.attachment));
    let service = link(&mut m, Role::Service, 4, Some(a.attachment));
    // Refus métier confirmés et consultables, sans changement du groupe.
    for id in 1..64 {
        let request = packet(Op::Assign, cap, id, a);
        assert_eq!(send(&mut m, cap, request).unwrap().status, Status::Refused);
    }
    assert_eq!(m.usage().unwrap().receipts, 64);
    assert_eq!(
        send(&mut m, cap, packet(Op::Suspend, cap, 64, a)),
        Err(Error::Full)
    );
    assert_eq!(
        send(&mut m, service, packet(Op::Close, service, 1, a)),
        Err(Error::Full)
    );
    let mut obsolete = packet(Op::Revoke, cap, 65, a);
    obsolete.revision -= 1;
    assert_eq!(send(&mut m, cap, obsolete), Err(Error::Full));
    let close = send(&mut m, cap, packet(Op::Revoke, cap, 65, a)).unwrap();
    assert_eq!(close.status, Status::Confirmed);
    assert_eq!(m.usage().unwrap().receipts, 64);
    assert_eq!(m.group(sl(0)).unwrap().state(), State::Revoking);
}
