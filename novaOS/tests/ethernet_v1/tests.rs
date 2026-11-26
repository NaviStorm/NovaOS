use super::*;
use protocol::Endpoint;
use wire::Header;
const MAC: [u8; 6] = [2, 0, 0, 0, 0, 1];
fn key(port: u64) -> Binding {
    Binding {
        boot: [7; 16],
        port,
        owner: 12,
        incarnation: 4,
        channel: 19,
        epoch: 9,
    }
}
fn opened(k: Binding, rights: u8) -> Port {
    Port::open(
        k,
        15,
        rights,
        negotiate(1, 0, 0, MULTICAST, 1).unwrap(),
        MAC,
    )
    .unwrap()
}
fn bytes() -> [u8; 60] {
    let mut f = [0x35; 60];
    f[..6].copy_from_slice(&MAC);
    f[6..12].copy_from_slice(&MAC);
    f
}
fn submit(p: &mut Port, k: Binding, id: u64, index: usize) -> Ticket {
    let s = p.tx_slot(k, index).unwrap();
    p.submit(
        k,
        id,
        &[Frame {
            slot: s,
            bytes: &bytes(),
        }],
    )
    .unwrap()
}
fn finish(p: &mut Port, t: Ticket) {
    let mut b = p.backend();
    b.transition(t, Stage::Admitted, Stage::Publishing).unwrap();
    b.transition(t, Stage::Publishing, Stage::Published)
        .unwrap();
    b.transition(t, Stage::Published, Stage::Complete).unwrap();
}
#[test]
fn versions_features_and_initial_denial() {
    assert_eq!(negotiate(2, 0, 0, 0, 0), Err(Error::Version));
    assert_eq!(negotiate(1, 1, 0, 0, 0), Err(Error::Version));
    assert_eq!(negotiate(1, 0, 2, 0, 15), Err(Error::Feature));
    assert_eq!(negotiate(1, 0, 0, 15, 15).unwrap().features, 1);
    let k = key(1);
    assert_eq!(negotiate(1, 0, 0, BOITE_GROUPEE, 0).unwrap().features, 0);
    assert_eq!(
        negotiate(1, 0, 0, BOITE_GROUPEE, BOITE_GROUPEE)
            .unwrap()
            .features,
        BOITE_GROUPEE
    );
    let v = negotiate(1, 0, 0, 0, 0).unwrap();
    assert!(matches!(
        Port::open(k, 0, TX_RIGHT, v, MAC),
        Err(Error::Denied)
    ));
    assert!(matches!(Port::open(k, 15, 0, v, MAC), Err(Error::Denied)));
    let mut p = opened(k, OBSERVE);
    assert_eq!(p.tx_slot(k, 0), Err(Error::Denied));
    assert_eq!(p.backend().receive(&bytes()), Err(Error::Denied));
    assert!(p.observe(k).is_ok());
    assert_eq!(
        p.configure_link(
            k,
            Link {
                carrier: Carrier::Down,
                bps: None,
                duplex: Duplex::Unknown
            }
        ),
        Err(Error::Denied)
    );
}
#[test]
fn identities_ports_and_incarnations() {
    let k = key(1);
    let mut p = opened(k, 15);
    let a = submit(&mut p, k, 1, 0);
    for alien in [
        Binding { boot: [1; 16], ..k },
        Binding { owner: 3, ..k },
        Binding {
            incarnation: 7,
            ..k
        },
        Binding { channel: 3, ..k },
        Binding { epoch: 4, ..k },
        key(2),
    ] {
        assert_eq!(p.status(alien, a), Err(Error::Denied));
        let mut other = opened(alien, 15);
        let t = submit(&mut other, alien, 1, 0);
        assert_eq!(
            p.backend()
                .transition(t, Stage::Admitted, Stage::Publishing),
            Err(Error::Stale)
        );
        assert_eq!(other.status(alien, a), Err(Error::Stale));
        assert_eq!(
            other.release(
                alien,
                Slot {
                    binding: k,
                    index: 0,
                    generation: 1
                }
            ),
            Err(Error::Denied)
        );
        other.revoke();
        assert_eq!(p.status(k, a), Ok(Stage::Admitted));
    }
}
#[test]
fn atomic_batch_and_generations() {
    let k = key(1);
    let mut p = opened(k, 15);
    let data = bytes();
    let slot = p.tx_slot(k, 0).unwrap();
    let valid = Frame { slot, bytes: &data };
    let other = p.tx_slot(k, 1).unwrap();
    for bad in [
        valid,
        Frame {
            slot: other,
            bytes: &data[..13],
        },
        Frame {
            slot: Slot {
                generation: 2,
                ..other
            },
            bytes: &data,
        },
    ] {
        assert!(p.submit(k, 1, &[valid, bad]).is_err());
        assert_eq!(p.credits(k), Ok(8));
        assert_eq!(p.tx_slot(k, 0), Ok(slot));
        assert_eq!(p.observe(k).unwrap().1.admitted, 0);
    }
    let t = p.submit(k, 1, &[valid]).unwrap();
    assert_eq!(p.status(k, t), Ok(Stage::Admitted));
    assert_eq!(p.submit(k, 2, &[valid]), Err(Error::Stale));
}
#[test]
fn lost_response_and_partial_publication() {
    let k = key(1);
    let mut p = opened(k, 15);
    let data = bytes();
    let frames = [0, 1, 2].map(|i| Frame {
        slot: p.tx_slot(k, i).unwrap(),
        bytes: &data,
    });
    let t = p.submit(k, 1, &frames).unwrap();
    assert_eq!(p.submit(k, 1, &frames), Ok(t));
    assert_eq!(p.credits(k), Ok(5));
    let mut different = data;
    different[20] ^= 1;
    assert_eq!(
        p.submit(
            k,
            1,
            &[
                Frame {
                    bytes: &different,
                    ..frames[0]
                },
                frames[1],
                frames[2]
            ]
        ),
        Err(Error::Conflict)
    );
    finish(&mut p, t);
    let b = Ticket { index: 1, ..t };
    let c = Ticket { index: 2, ..t };
    p.backend()
        .transition(b, Stage::Admitted, Stage::Publishing)
        .unwrap();
    assert_eq!(
        p.backend()
            .transition(b, Stage::Admitted, Stage::Publishing),
        Err(Error::State)
    );
    assert_eq!(
        p.backend().transition(t, Stage::Published, Stage::Complete),
        Err(Error::State)
    );
    assert_eq!(p.status(k, b), Ok(Stage::Publishing));
    assert_eq!(p.status(k, c), Ok(Stage::Admitted));
    assert_eq!(p.ack(k, 1), Err(Error::State));
    assert_eq!(p.credits(k), Ok(6));
    p.backend().cancel_unpublished(b).unwrap();
    finish(&mut p, c);
    assert_eq!(p.credits(k), Ok(8));
    assert_eq!(p.observe(k).unwrap().1.published, 2);
    p.ack(k, 1).unwrap();
    p.ack(k, 1).unwrap();
    assert_eq!(p.submit(k, 1, &frames), Err(Error::Stale));
}
#[test]
fn snapshot_padding_and_remote_delivery_oracle() {
    let k = key(1);
    let mut p = opened(k, 15);
    let mut f = bytes();
    let saved = f;
    let slot = p.tx_slot(k, 0).unwrap();
    let t = p
        .submit(
            k,
            1,
            &[Frame {
                slot,
                bytes: &f[..14],
            }],
        )
        .unwrap();
    f.fill(255);
    let mut b = p.backend();
    b.transition(t, Stage::Admitted, Stage::Publishing).unwrap();
    assert_eq!(&b.image(t).unwrap()[..14], &saved[..14]);
    assert_eq!(&b.image(t).unwrap()[14..], &[0; 46]);
    let mut peer = opened(key(2), RX_RIGHT);
    b.transition(t, Stage::Publishing, Stage::Published)
        .unwrap();
    b.transition(t, Stage::Published, Stage::Complete).unwrap();
    assert_eq!(p.observe(k).unwrap().1.complete, 1);
    // Première trame perdue sur le lien synthétique. Le pair n'a reçu aucun slot.
    assert!(peer.rx.iter().all(|r| r.len == 0));
    let t = submit(&mut p, k, 2, 0);
    p.backend()
        .transition(t, Stage::Admitted, Stage::Publishing)
        .unwrap();
    let slot = peer
        .backend()
        .receive(p.backend().image(t).unwrap())
        .unwrap();
    p.backend()
        .transition(t, Stage::Publishing, Stage::Published)
        .unwrap();
    p.backend()
        .transition(t, Stage::Published, Stage::Complete)
        .unwrap();
    assert_eq!(peer.read_rx(key(2), slot).unwrap(), saved);
    assert_eq!(p.observe(k).unwrap().1.complete, 2);
    assert_eq!(peer.stats.rx, 1);
}
#[test]
fn credits_and_journal_saturation_recovery() {
    let k = key(1);
    let mut p = opened(k, 15);
    let data = bytes();
    let fs = core::array::from_fn::<_, LIMIT, _>(|i| Frame {
        slot: p.tx_slot(k, i).unwrap(),
        bytes: &data,
    });
    let first = p.submit(k, 1, &fs).unwrap();
    assert_eq!(p.submit(k, 2, &fs[..1]), Err(Error::Full));
    for index in 0..8 {
        finish(&mut p, Ticket { index, ..first });
    }
    let second = submit(&mut p, k, 2, 0);
    assert_eq!(p.submit(k, 3, &fs[..1]), Err(Error::Full));
    p.ack(k, 1).unwrap();
    finish(&mut p, second);
    submit(&mut p, k, 3, 1);
    assert_eq!(p.credits(k), Ok(7));
}
#[test]
fn multicast_rx_quota_and_single_return() {
    let k = key(3);
    let mut p = opened(k, 15);
    let m = [0x33, 0x33, 0, 0, 0, 1];
    let mut frame = bytes();
    frame[..6].copy_from_slice(&m);
    assert_eq!(p.backend().receive(&frame), Err(Error::Denied));
    p.multicast(k, m, true).unwrap();
    p.multicast(k, m, true).unwrap();
    let s = p.backend().receive(&frame).unwrap();
    assert_eq!(p.read_rx(k, s).unwrap(), frame);
    for _ in 1..8 {
        p.backend().receive(&frame).unwrap();
    }
    assert_eq!(p.backend().receive(&frame), Err(Error::Full));
    p.release(k, s).unwrap();
    p.release(k, s).unwrap();
    let later = p.backend().receive(&frame).unwrap();
    assert_eq!(s.index, later.index);
    assert_eq!(p.release(k, s), Err(Error::Stale));
    p.multicast(k, m, false).unwrap();
    assert_eq!(p.backend().receive(&frame), Err(Error::Denied));
    assert_eq!(p.observe(k).unwrap().1.returned, 1);
}
#[test]
fn link_stats_notifications_rearm() {
    let k = key(1);
    let mut p = opened(k, 15);
    let v = p.revision(k).unwrap();
    assert_eq!(p.arm(k, v), Ok(false));
    let down = Link {
        carrier: Carrier::Down,
        bps: None,
        duplex: Duplex::Unknown,
    };
    p.configure_link(k, down).unwrap();
    assert_eq!(p.events_since(k, v).unwrap().1, EV_LINK);
    let s = p.tx_slot(k, 0).unwrap();
    assert_eq!(
        p.submit(
            k,
            1,
            &[Frame {
                slot: s,
                bytes: &bytes()
            }]
        ),
        Err(Error::LinkDown)
    );
    let up = Link {
        carrier: Carrier::Up,
        bps: Some(100_000_000_000),
        duplex: Duplex::Full,
    };
    p.configure_link(k, up).unwrap();
    let seen = p.revision(k).unwrap();
    submit(&mut p, k, 1, 0);
    p.backend().receive(&bytes()).unwrap();
    assert_eq!(p.events_since(k, seen).unwrap().1, EV_TX | EV_RX);
    assert_eq!(p.arm(k, seen), Ok(true));
    assert_eq!(p.arm(k, p.revision(k).unwrap()), Ok(false));
    assert_eq!(p.observe(k).unwrap().0, up);
}
#[test]
fn revoke_during_lot_scrubs_and_preserves_unknown() {
    let k = key(1);
    let mut p = opened(k, 15);
    let data = bytes();
    let fs = [0, 1, 2].map(|i| Frame {
        slot: p.tx_slot(k, i).unwrap(),
        bytes: &data,
    });
    let t = p.submit(k, 1, &fs).unwrap();
    let b = Ticket { index: 1, ..t };
    p.backend()
        .transition(b, Stage::Admitted, Stage::Publishing)
        .unwrap();
    let c = Ticket { index: 2, ..t };
    finish(&mut p, c);
    p.backend().receive(&data).unwrap();
    let result = p.close(k).unwrap();
    assert_eq!((result.uncertain, result.cancelled), (1, 1));
    assert_eq!(p.revoke(), result);
    assert_eq!(p.backend().retired_status(b), Ok(Stage::Unknown));
    assert_eq!(p.backend().retired_status(c), Ok(Stage::Complete));
    assert_eq!(p.submit(k, 1, &fs), Err(Error::Closed));
    assert_eq!(p.backend().image(b), Err(Error::Closed));
    assert!(
        p.journal
            .iter()
            .flat_map(|b| &b.frames)
            .all(|f| f.bytes.iter().all(|&x| x == 0))
    );
    assert!(
        p.rx.iter()
            .all(|r| r.len == 0 && r.bytes.iter().all(|&b| b == 0))
    );
}
#[test]
fn all_transports_same_bytes_and_counts() {
    fn run<const N: usize>() {
        use transport::Channel;
        let a = key(1);
        let b = Binding {
            owner: 99,
            channel: 40,
            ..a
        };
        for count in [1usize, 2, 4, 8] {
            for len in [60usize, 1514] {
                let mut ch = Channel::<N>::new(a, b).unwrap();
                let mut input = [0u8; transport::WINDOW];
                let mut out = [0u8; transport::WINDOW];
                let size = count * len;
                for (i, x) in input[..size].iter_mut().enumerate() {
                    *x = (i * 7) as u8;
                }
                let mut body = [0u8; 96];
                for i in 0..count {
                    body[12 * i..12 * i + 12]
                        .copy_from_slice(&wire::descriptor(i as u16, len as u16, 1));
                }
                let h = Header {
                    op: wire::TX,
                    reply: false,
                    count: count as u16,
                    epoch: a.epoch,
                    request: 1,
                };
                let mut buf = [0u8; 128];
                let n = h.encode(&mut buf, &body[..count * 12]).unwrap();
                ch.begin(a, 1).unwrap();
                assert_eq!(ch.begin(a, 1), Err(Error::Full));
                assert_eq!(
                    ch.transfer(a, a, &buf[..n], &input[..size], &mut out[..size]),
                    Err(Error::Denied)
                );
                assert!(
                    ch.transfer(a, b, &buf[..n], &input[..size], &mut out[..size - 1])
                        .is_err()
                );
                assert!(out.iter().all(|&x| x == 0));
                let c = ch
                    .transfer(a, b, &buf[..n], &input[..size], &mut out[..size])
                    .unwrap();
                assert_eq!(c.bytes, (if N == 128 { 4 } else { 2 }) * size as u64);
                assert_eq!(out[..size], input[..size]);
                assert_eq!(
                    c.calls,
                    4 + match N {
                        128 => 2 * (count * len.div_ceil(96)) as u64,
                        4096 => 2 * count.div_ceil(4096 / len) as u64,
                        _ => 0,
                    }
                );
                assert_eq!(ch.begin(a, 1), Err(Error::Stale));
                ch.begin(a, 2).unwrap();
                ch.revoke();
                assert!(ch.clean());
                assert_eq!(ch.begin(a, 2), Err(Error::Closed));
            }
        }
    }
    run::<128>();
    run::<4096>();
    run::<{ transport::WINDOW }>();
}
#[test]
fn binary_vectors_canonical_both_isa() {
    let corpus = include_bytes!("vecteurs.bin");
    assert_eq!(&corpus[..4], b"ETH1");
    let mut i = 4;
    let mut counts = [0; 3];
    while i < corpus.len() {
        let kind = corpus[i] as usize;
        let len = wire::word(corpus, i + 1) as usize;
        i += 3;
        let data = &corpus[i..i + len];
        i += len;
        counts[kind] += 1;
        match kind {
            0 => {
                let (h, b) = Header::decode(data).unwrap();
                let mut out = [0xa5; 128];
                let n = h.encode(&mut out, b).unwrap();
                assert_eq!(&out[..n], data);
            }
            1 => assert!(Header::decode(data).is_err()),
            2 => {
                let (h, e) = protocol::read_failure(data).unwrap();
                assert_eq!(protocol::failure(h, e), data);
            }
            _ => unreachable!(),
        }
    }
    assert_eq!(counts, [30, 8, 1]);
}
#[test]
fn protocol_admission_and_receipts() {
    let k = key(1);
    let mut ep = Endpoint::new(k, 15, 1, MAC);
    let mut rpc = |op: u16, req: u64, n: u16, body: &[u8], slots: &[&[u8]]| {
        let h = Header {
            op,
            reply: false,
            count: n,
            epoch: k.epoch,
            request: req,
        };
        let mut input = [0; 128];
        let mut out = [0; 128];
        let len = h.encode(&mut input, body).unwrap();
        let count = ep.exchange(k, &input[..len], slots, &mut out)?;
        Header::decode(&out[..count]).unwrap();
        Ok::<_, Error>((count, out))
    };
    assert_eq!(
        rpc(wire::OPEN, 1, 0, &[15, 0, 0, 0, 0, 0, 0, 0], &[]),
        Err(Error::State)
    );
    rpc(wire::NEGOTIATE, 1, 0, &[0; 16], &[]).unwrap();
    rpc(wire::OPEN, 2, 0, &[15, 0, 0, 0, 0, 0, 0, 0], &[]).unwrap();
    assert_eq!(
        rpc(wire::QUERY, 3, 0, &1u64.to_le_bytes(), &[]),
        Err(Error::NotAdmitted)
    );
    let data = bytes();
    let desc = wire::descriptor(0, 60, 1);
    let first = rpc(wire::TX, 1, 1, &desc, &[&data]).unwrap();
    assert_eq!(first, rpc(wire::TX, 1, 1, &desc, &[&data]).unwrap());
    let (_, status) = rpc(wire::QUERY, 4, 0, &1u64.to_le_bytes(), &[]).unwrap();
    assert_eq!(wire::word(&status, 34), 0);
    assert_eq!(
        rpc(wire::ACK, 5, 0, &1u64.to_le_bytes(), &[]),
        Err(Error::State)
    );
    let (_, stats) = rpc(wire::STATS, 6, 0, &[], &[]).unwrap();
    assert_eq!(stats[32], 1);
    let closed = rpc(wire::CLOSE, 7, 0, &[], &[]).unwrap();
    assert_eq!(rpc(wire::CLOSE, 7, 0, &[], &[]).unwrap(), closed);
    assert_eq!(rpc(wire::CLOSE, 8, 0, &[], &[]), Err(Error::Closed));
    assert_eq!(rpc(wire::TX, 1, 1, &desc, &[&data]), Err(Error::Closed));
}
#[test]
fn binary_multicast_rx_atomic_return_and_link() {
    fn rpc(ep: &mut Endpoint, k: Binding, op: u16, n: u16, body: &[u8]) -> Result<[u8; 128]> {
        let mut input = [0u8; 128];
        let mut out = [0u8; 128];
        let len = Header {
            op,
            reply: false,
            count: n,
            epoch: k.epoch,
            request: 1,
        }
        .encode(&mut input, body)
        .unwrap();
        let size = ep.exchange(k, &input[..len], &[], &mut out)?;
        Header::decode(&out[..size]).unwrap();
        Ok(out)
    }
    let k = key(7);
    let mut ep = Endpoint::new(k, 15, 1, MAC);
    let mut negotiation = [0; 16];
    negotiation[0] = 1;
    rpc(&mut ep, k, wire::NEGOTIATE, 0, &negotiation).unwrap();
    rpc(&mut ep, k, wire::OPEN, 0, &[15, 0, 0, 0, 0, 0, 0, 0]).unwrap();
    let group = [0x33, 0x33, 0, 0, 0, 1];
    rpc(&mut ep, k, wire::JOIN, 0, &group).unwrap();
    let mut f = bytes();
    f[..6].copy_from_slice(&group);
    let a = ep.port().unwrap().backend().receive(&f).unwrap();
    let b = ep.port().unwrap().backend().receive(&f).unwrap();
    let rx = rpc(&mut ep, k, wire::RX, 0, &[]).unwrap();
    assert_eq!(wire::word(&rx, 10), 2);
    let mut ret = [0; 24];
    ret[..12].copy_from_slice(&wire::descriptor(a.index, 0, a.generation));
    ret[12..].copy_from_slice(&wire::descriptor(b.index, 0, b.generation + 1));
    assert_eq!(rpc(&mut ep, k, wire::RELEASE, 2, &ret), Err(Error::Stale));
    assert_eq!(ep.port().unwrap().read_rx(k, a).unwrap(), f);
    ret[12..].copy_from_slice(&wire::descriptor(b.index, 0, b.generation));
    rpc(&mut ep, k, wire::RELEASE, 2, &ret).unwrap();
    rpc(&mut ep, k, wire::RELEASE, 2, &ret).unwrap();
    assert_eq!(
        wire::word(&rpc(&mut ep, k, wire::RX, 0, &[]).unwrap(), 10),
        0
    );
    rpc(&mut ep, k, wire::LEAVE, 0, &group).unwrap();
    assert_eq!(ep.port().unwrap().backend().receive(&f), Err(Error::Denied));
    let mut link = [0; 16];
    link[0] = 2;
    link[1] = 2;
    link[8..].copy_from_slice(&25_000_000_000u64.to_le_bytes());
    rpc(&mut ep, k, wire::SET_LINK, 0, &link).unwrap();
    assert_eq!(rpc(&mut ep, k, wire::LINK, 0, &[]).unwrap()[32..48], link);
    let event = rpc(&mut ep, k, wire::ARM, 0, &1u64.to_le_bytes()).unwrap();
    assert_ne!(event[40] & EV_LINK, 0);
}
#[test]
fn session_death_and_error_envelopes() {
    let k = key(1);
    let mut p = opened(k, 15);
    let a = submit(&mut p, k, 1, 0);
    let mut backend = p.backend();
    backend
        .transition(a, Stage::Admitted, Stage::Publishing)
        .unwrap();
    backend
        .transition(a, Stage::Publishing, Stage::Published)
        .unwrap();
    assert_eq!(p.revoke().uncertain, 1);
    assert_eq!(p.backend().retired_status(a), Ok(Stage::Unknown));
    let new = Binding {
        epoch: k.epoch + 1,
        ..k
    };
    let mut fresh = opened(new, 15);
    assert_eq!(
        fresh
            .backend()
            .transition(a, Stage::Published, Stage::Complete),
        Err(Error::Stale)
    );
    let h = Header {
        op: wire::QUERY,
        reply: true,
        count: 0,
        epoch: 9,
        request: 1,
    };
    for e in [
        Error::Denied,
        Error::Stale,
        Error::Version,
        Error::Feature,
        Error::Invalid,
        Error::Full,
        Error::Conflict,
        Error::Closed,
        Error::State,
        Error::LinkDown,
        Error::Exhausted,
        Error::NotAdmitted,
    ] {
        let mut buf = protocol::failure(h, e);
        assert_eq!(protocol::read_failure(&buf), Ok((h, e)));
        buf[39] = 42;
        assert!(protocol::read_failure(&buf).is_err());
    }
}

#[test]
fn revoke_between_put_and_take() {
    let a = key(1);
    let b = Binding {
        owner: 27,
        channel: 4,
        ..a
    };
    transport::Channel::<4096>::new(a, b)
        .unwrap()
        .partial_box_test(a, b);
}
#[test]
fn armed_wait_wakes_once_including_revoke() {
    let k = key(1);
    let mut p = opened(k, 15);
    let seen = p.revision(k).unwrap();
    assert!(!p.arm(k, seen).unwrap());
    assert_eq!(p.take_notification(k), Ok(None));
    submit(&mut p, k, 1, 0);
    p.backend().receive(&bytes()).unwrap();
    assert_eq!(p.take_notification(k).unwrap().unwrap().1, EV_TX | EV_RX);
    assert_eq!(p.take_notification(k), Ok(None));
    let seen = p.revision(k).unwrap();
    assert!(!p.arm(k, seen).unwrap());
    p.revoke();
    p.revoke();
    assert_eq!(p.take_notification(k).unwrap().unwrap().1, EV_CLOSED);
    assert_eq!(p.take_notification(k), Err(Error::Closed));
}
#[test]
fn publication_incertaine_conserve_credit_et_recu() {
    let k = key(1);
    let mut p = opened(k, TX_RIGHT);
    let t = submit(&mut p, k, 1, 0);
    assert_eq!(p.backend().incertain(t), Err(Error::State));
    p.backend()
        .transition(t, Stage::Admitted, Stage::Publishing)
        .unwrap();
    let restant = p.credit;
    p.backend().incertain(t).unwrap();
    assert_eq!(p.status(k, t), Ok(Stage::Unknown));
    assert_eq!(p.credit, restant);
    assert_eq!(p.backend().incertain(t), Err(Error::State));
    assert_eq!(p.ack(k, 1), Err(Error::State));
    let bilan = p.revoke();
    assert_eq!((bilan.uncertain, bilan.cancelled), (1, 0));
    assert_eq!(p.backend().retired_status(t), Ok(Stage::Unknown));
    assert_eq!(p.backend().incertain(t), Err(Error::Closed));
}

#[test]
fn edge_counters_fail_closed() {
    let k = key(1);
    let mut p = opened(k, 15);
    p.tx_generation[0] = u32::MAX;
    let s = p.tx_slot(k, 0).unwrap();
    assert_eq!(
        p.submit(
            k,
            1,
            &[Frame {
                slot: s,
                bytes: &bytes()
            }]
        ),
        Err(Error::Exhausted)
    );
    p.revision = u64::MAX;
    assert_eq!(p.arm(k, u64::MAX), Ok(true));
    p.rx[0].generation = u32::MAX;
    assert_eq!(p.backend().receive(&bytes()).unwrap().index, 1);
}
