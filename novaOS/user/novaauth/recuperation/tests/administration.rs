#![cfg(feature = "administration-lab")]
use ed25519_dalek::VerifyingKey;
use novaos_recovery::administration::*;
fn vector() -> Vec<Vec<u8>> {
    include_str!("vectors/administration.txt")
        .lines()
        .map(|l| {
            (0..l.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&l[i..i + 2], 16).unwrap())
                .collect()
        })
        .collect()
}
#[test]
fn real_python_authority_receipts_and_request_encodings_match() {
    let v = vector();
    let key = VerifyingKey::from_bytes(v[0].as_slice().try_into().unwrap()).unwrap();
    let publish = Message::parse(PUBLISH, &v[1]).unwrap();
    publish.publication(&key, &v[2]).unwrap();
    publish
        .scope(&[b'S'; 16], &[b'D'; 16], &[b'I'; 32])
        .unwrap();
    let ack = Message::parse(ACKNOWLEDGE, &v[3]).unwrap();
    assert!(ack.acknowledged(&key, &v[4]).unwrap() > 0);
    let query = Message::parse(OUTCOME, &v[5]).unwrap();
    let (effects, terminal) = query.outcome(&key, &v[6]).unwrap();
    assert_eq!(effects, 7);
    assert_eq!(terminal.unwrap().len(), 192);
    let mut signed = [0; 384];
    let n = publish.signed(&mut signed);
    assert_eq!(&signed[..CHANGE.len()], CHANGE);
    assert_eq!(&signed[CHANGE.len()..n], v[1]);
    assert!(publish.outcome(&key, &v[6]).is_err());
    assert!(query.publication(&key, &v[2]).is_err());
}
#[test]
fn refusal_of_substituted_scope_freshness_and_receipts() {
    let v = vector();
    let key = VerifyingKey::from_bytes(v[0].as_slice().try_into().unwrap()).unwrap();
    let q = Message::parse(OUTCOME, &v[5]).unwrap();
    for i in 0..v[6].len() {
        let mut reply = v[6].clone();
        reply[i] ^= 1;
        assert!(q.outcome(&key, &reply).is_err(), "{i}");
    }
    for i in [0, 16, 32, 64, 72, 80, 112, 128, 159] {
        let mut bytes = v[5].clone();
        bytes[i] ^= 1;
        if let Ok(changed) = Message::parse(OUTCOME, &bytes) {
            assert!(changed.outcome(&key, &v[6]).is_err());
        }
    }
    assert!(q.scope(&[b'Z'; 16], &[b'D'; 16], &[b'I'; 32]).is_err());
    let ack = Message::parse(ACKNOWLEDGE, &v[3]).unwrap();
    for i in 0..v[4].len() {
        let mut reply = v[4].clone();
        reply[i] ^= 1;
        assert!(ack.acknowledged(&key, &reply).is_err());
    }
}
#[test]
fn strict_bounds_no_destructive_operation_no_zero_identity() {
    let v = vector();
    for (op, index) in [(PUBLISH, 1), (ACKNOWLEDGE, 3), (OUTCOME, 5)] {
        let b = &v[index];
        for length in 0..b.len() {
            assert!(Message::parse(op, &b[..length]).is_err());
        }
        let mut longer = b.clone();
        longer.push(0);
        assert!(Message::parse(op, &longer).is_err());
        for (start, end) in [(0, 16), (16, 32), (32, 64), (80, 112), (112, 128)] {
            let mut zero = b.clone();
            zero[start..end].fill(0);
            assert!(Message::parse(op, &zero).is_err());
        }
    }
    for op in [0, 30, 31, 40, 41, 42, 49, 53, u16::MAX] {
        assert!(Message::parse(op, &v[1]).is_err());
    }
    for kind in [0, 5, 6, 255] {
        let mut q = v[1].clone();
        q[208] = kind;
        assert!(Message::parse(PUBLISH, &q).is_err());
    }
    for at in [64, 72, 176, 200] {
        let mut q = v[1].clone();
        q[at..at + 8].fill(255);
        assert!(Message::parse(PUBLISH, &q).is_err());
    }
    for mask in [0, 8, 255] {
        let mut q = v[3].clone();
        q[128] = mask;
        assert!(Message::parse(ACKNOWLEDGE, &q).is_err());
    }
    let mut q = v[5].clone();
    q[128..].fill(0);
    assert!(Message::parse(OUTCOME, &q).is_err());
}
