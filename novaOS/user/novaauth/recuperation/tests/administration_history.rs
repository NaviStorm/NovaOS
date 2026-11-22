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

#[test]
fn additive_non_retiring_history_survives_without_volatile_completion() {
    use ed25519_dalek::{SigningKey, Signer};
    let v=vectors(); let signer=SigningKey::from_bytes(&[99;32]);let key=signer.verifying_key();
    let mut original=v[2].clone(); original[184..208].fill(0);
    let generation=u64::from_be_bytes(original[72..80].try_into().unwrap())+1;
    let mut query=v[1].clone(); query[72..80].copy_from_slice(&generation.to_be_bytes());query[80..112].copy_from_slice(&original[128..160]);
    fn reply(signer:&SigningKey,q:&[u8],o:&[u8])->Vec<u8>{
        let mut r=q.to_vec();r.extend_from_slice(o);r.push(0);
        let mut signed=HISTORY_PREFIX.to_vec();signed.extend_from_slice(&r);
        r.extend_from_slice(&signer.sign(&signed).to_bytes());r
    }
    let owner=original[160..176].try_into().unwrap();
    for kind in [RENAME,CREATE_GROUP,ADD_MEMBER] {
        original[208]=kind;
        assert!(Message::parse(PUBLISH,&original).is_ok());
        let r=reply(&signer,&query,&original);let message=Message::parse(HISTORY,&query).unwrap();
        assert_eq!(message.durable_non_retiring_history(&key,&r,&owner).unwrap(),Some(generation));
        assert!(message.durable_non_retiring_history(&key,&r,&[8;16]).is_err());
        let mut damaged=r.clone();damaged[200]^=1;assert!(message.durable_non_retiring_history(&key,&damaged,&owner).is_err());
        let mut before=query.clone();before[72..80].copy_from_slice(&(generation-1).to_be_bytes());
        let r=reply(&signer,&before,&original);
        assert_eq!(Message::parse(HISTORY,&before).unwrap().durable_non_retiring_history(&key,&r,&owner).unwrap(),None);
        let mut wrong=query.clone();wrong[80]^=1;let r=reply(&signer,&wrong,&original);
        assert!(Message::parse(HISTORY,&wrong).unwrap().durable_non_retiring_history(&key,&r,&owner).is_err());
        let mut bad=original.clone();bad[184]=1;assert!(Message::parse(PUBLISH,&bad).is_err());
    }
    original[208]=UPDATE;let r=reply(&signer,&query,&original);
    assert_eq!(Message::parse(HISTORY,&query).unwrap().durable_non_retiring_history(&key,&r,&owner).unwrap(),None);
}

#[test]
fn retired_history_requires_exact_durable_terminal() {
    let v=vectors();let key=VerifyingKey::from_bytes(v[0].as_slice().try_into().unwrap()).unwrap();
    let q=Message::parse(HISTORY,&v[1]).unwrap();let owner=v[2][160..176].try_into().unwrap();
    assert_eq!(q.durable_retired_history(&key,&v[3],&owner).unwrap(),None);
    assert!(q.durable_retired_history(&key,&v[4],&owner).unwrap().is_some());
}
