#![cfg(feature = "administration-lab")]
use ed25519_dalek::{Signer, SigningKey};
use novaos_recovery::{exec_admission as old, subject_admission::*};
fn request() -> Request {
    let mut b = [b'X'; old::SIZE];
    b[..8].copy_from_slice(b"NAEXEC01");
    b[72..80].copy_from_slice(&6u64.to_be_bytes());
    Request::bind(&old::Request::parse(&b).unwrap(), [b'A'; 16], 2).unwrap()
}
fn signed(prefix: &[u8], data: &[u8]) -> Vec<u8> {
    let key = SigningKey::from_bytes(&[7; 32]);
    [data, &key.sign(&[prefix, data].concat()).to_bytes()].concat()
}
#[test]
fn canonical_subject_epoch_and_legacy_contracts_are_distinct() {
    let q = request();
    assert_eq!(q.bytes().len(), 280);
    assert_eq!(q.subject(), [b'A'; 16]);
    assert_eq!(q.epoch(), 2);
    assert!(old::Request::parse(q.bytes()).is_err());
    assert!(old::Request::parse(&q.bytes()[..256]).is_err());
    assert!(q.scope(&[b'X'; 16], &[b'X'; 16], &[b'X'; 32]).is_ok());
    for n in [0, 255, 256, 279] {
        assert!(Request::parse(&q.bytes()[..n]).is_err());
    }
    for epoch in [0, 1u64 << 63, u64::MAX] {
        let mut b = *q.bytes();
        b[272..].copy_from_slice(&epoch.to_be_bytes());
        assert!(Request::parse(&b).is_err());
    }
    let mut b = *q.bytes();
    b[256..272].fill(0);
    assert!(Request::parse(&b).is_err());
}
#[test]
fn receipt_and_terminal_proof_bind_subject_epoch_and_session() {
    let q = request();
    let key = SigningKey::from_bytes(&[7; 32]).verifying_key();
    let receipt = signed(old::RECEIPT, &[&4u64.to_be_bytes()[..], q.bytes()].concat());
    assert_eq!(q.admission(&key, &receipt).unwrap(), 4);
    let finish = q.finish(1, &[5; 32]).unwrap();
    for offset in [144, 256, 279] {
        let mut bytes = *q.bytes();
        bytes[offset] ^= 1;
        let other = Request::parse(&bytes).unwrap();
        assert!(other.admission(&key, &receipt).is_err());
        assert_ne!(other.finish(1, &[5; 32]).unwrap(), finish);
    }
    assert!(q.finish(2, &[5; 32]).is_err());
    assert!(q.finish(1, &[0; 32]).is_err());
}
#[test]
fn delayed_creation_and_lost_reply_remain_non_terminal() {
    let q = request();
    let fresh = [3; 32];
    let key = SigningKey::from_bytes(&[7; 32]).verifying_key();
    for (state, child, terminal) in [(0, [0; 32], 0u64), (1, [5; 32], 9), (2, [0; 32], 9)] {
        let data = [
            q.bytes().as_slice(),
            &fresh,
            &4u64.to_be_bytes(),
            &[state],
            &child,
            &terminal.to_be_bytes(),
        ]
        .concat();
        let reply = signed(old::QUERY, &data);
        let out = q.outcome(&key, &fresh, &reply).unwrap();
        assert_eq!(out.state, state);
        assert_eq!(out.child, child);
        assert!(q.outcome(&key, &[4; 32], &reply).is_err());
    }
    for (state, child, terminal) in [
        (0, [5; 32], 0u64),
        (0, [0; 32], 9),
        (1, [0; 32], 9),
        (2, [5; 32], 9),
        (1, [5; 32], 4),
    ] {
        let data = [
            q.bytes().as_slice(),
            &fresh,
            &4u64.to_be_bytes(),
            &[state],
            &child,
            &terminal.to_be_bytes(),
        ]
        .concat();
        assert!(q.outcome(&key, &fresh, &signed(old::QUERY, &data)).is_err());
    }
}
#[test]
fn receipt_produced_by_python_durable_authority_is_verified_natively() {
    fn hex(s: &str) -> Vec<u8> {
        s.as_bytes()
            .chunks_exact(2)
            .map(|p| u8::from_str_radix(core::str::from_utf8(p).unwrap(), 16).unwrap())
            .collect()
    }
    let fields: Vec<_> = include_str!("vectors/subject_admission.txt")
        .lines()
        .map(hex)
        .collect();
    assert_eq!(fields.len(), 5);
    let key =
        ed25519_dalek::VerifyingKey::from_bytes(fields[0].as_slice().try_into().unwrap()).unwrap();
    let q = Request::parse(&fields[1]).unwrap();
    assert_eq!(q.subject(), [b'A'; 16]);
    assert_eq!(q.epoch(), 2);
    let position = q.admission(&key, &fields[2]).unwrap();
    let out = q
        .outcome(&key, fields[3].as_slice().try_into().unwrap(), &fields[4])
        .unwrap();
    assert_eq!(out.admission, position);
    assert_eq!(out.state, 1);
    assert_eq!(out.child, [b'C'; 32]);
    assert!(out.terminal > position);
    let mut changed = *q.bytes();
    changed[279] = 3;
    assert!(
        Request::parse(&changed)
            .unwrap()
            .outcome(&key, fields[3].as_slice().try_into().unwrap(), &fields[4])
            .is_err()
    );
}
