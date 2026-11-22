#![cfg(feature = "administration-lab")]
use ed25519_dalek::VerifyingKey;
use novaos_recovery::administration::*;
fn vectors() -> Vec<Vec<u8>> {
    include_str!("vectors/administration_history.txt")
        .lines()
        .map(|s| {
            s.as_bytes()
                .chunks_exact(2)
                .map(|p| u8::from_str_radix(core::str::from_utf8(p).unwrap(), 16).unwrap())
                .collect()
        })
        .collect()
}
#[test]
fn python_history_is_authenticated_and_keeps_the_original_principal() {
    let v = vectors();
    let key = VerifyingKey::from_bytes(v[0].as_slice().try_into().unwrap()).unwrap();
    let q = Message::parse(HISTORY, &v[1]).unwrap();
    let (original, mask, terminal) = q.history(&key, &v[3]).unwrap();
    assert_eq!(original, v[2]);
    assert_eq!(mask, 0);
    assert!(terminal.is_none());
    let (original, mask, terminal) = q.history(&key, &v[4]).unwrap();
    assert_eq!(original, v[2]);
    assert_eq!(mask, 7);
    assert_eq!(terminal.unwrap().len(), 192);
    assert_eq!(q.outcome(&key, &v[4]), Err(novaos_recovery::Error::Scope));
}
#[test]
fn altered_records_truncations_replays_and_cross_operation_responses_fail_closed() {
    let v = vectors();
    let key = VerifyingKey::from_bytes(v[0].as_slice().try_into().unwrap()).unwrap();
    let q = Message::parse(HISTORY, &v[1]).unwrap();
    for good in [&v[3], &v[4]] {
        for i in 0..good.len() {
            let mut bad = good.clone();
            bad[i] ^= 1;
            assert!(q.history(&key, &bad).is_err(), "offset {i}");
        }
        for len in 0..good.len() {
            assert!(q.history(&key, &good[..len]).is_err());
        }
    }
    let mut fresh = v[1].clone();
    fresh[128] ^= 1;
    let other = Message::parse(HISTORY, &fresh).unwrap();
    assert!(other.history(&key, &v[4]).is_err());
    let old = Message::parse(OUTCOME, &v[1]).unwrap();
    assert!(old.history(&key, &v[4]).is_err());
}
