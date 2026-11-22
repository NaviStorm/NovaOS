#![cfg(feature = "administration-lab")]
use ed25519_dalek::{Signer, SigningKey};
use novaos_auth_session::{
    Binding, Error, Factor, Possession, Route,
    epoch_sessions::{EpochBinding, EpochSessions, EpochVerifier},
};
fn setup() -> (SigningKey, EpochBinding, Route) {
    let k = SigningKey::from_bytes(&[11; 32]);
    let b = EpochBinding {
        binding: Binding {
            domain: [1; 16],
            subject: [8; 16],
            reference: Possession(k.verifying_key()).reference([1; 16]),
            head: [3; 32],
        },
        epoch: 1,
    };
    (
        k,
        b,
        Route {
            client: [4; 32],
            issuer: [9; 32],
        },
    )
}
fn open(
    v: &mut EpochVerifier,
    s: &mut EpochSessions,
    k: &SigningKey,
    b: EpochBinding,
    r: Route,
    t: u64,
) -> [u8; 32] {
    let (token, h) = v.begin(b, r, [6; 16], [7; 32], t, 15).unwrap();
    let proof = v
        .finish(
            token,
            r,
            t,
            Some(b),
            &Possession(k.verifying_key()),
            &k.sign(&h).to_bytes(),
        )
        .unwrap();
    s.open(proof, b, t, 100).unwrap()
}
#[test]
fn dormant_lease_never_resurrects_and_fresh_authentication_succeeds() {
    let (k, b, r) = setup();
    let mut v = EpochVerifier::new();
    let mut s = EpochSessions::new();
    let old = open(&mut v, &mut s, &k, b, r, 10);
    // Aucune consultation ou notification pendant l'état désactivé : la
    // comparaison avec l'époque fraîche doit suffire au prochain contrôle.
    let next = EpochBinding {
        epoch: 3,
        binding: Binding {
            head: [5; 32],
            ..b.binding
        },
    };
    assert_eq!(s.inspect(old, r, Some(next), 12), Err(Error::Refused));
    let fresh = open(&mut v, &mut s, &k, next, r, 13);
    assert_ne!(old, fresh);
    assert_eq!(s.inspect(fresh, r, Some(next), 14), Ok(b.binding.subject));
    assert_eq!(s.inspect(old, r, Some(next), 14), Err(Error::Refused));
    assert_eq!(s.inspect(old, r, Some(b), 15), Err(Error::Refused));
}
#[test]
fn retirement_is_idempotent_scoped_and_does_not_close_new_epoch() {
    let (k, b, r) = setup();
    let mut v = EpochVerifier::new();
    let mut s = EpochSessions::new();
    let old = open(&mut v, &mut s, &k, b, r, 10);
    let other = EpochBinding {
        binding: Binding {
            subject: [9; 16],
            ..b.binding
        },
        ..b
    };
    let unrelated = open(&mut v, &mut s, &k, other, r, 11);
    assert_eq!(
        s.retire(b.binding.domain, b.binding.subject, b.epoch),
        Ok(1)
    );
    assert_eq!(
        s.retire(b.binding.domain, b.binding.subject, b.epoch),
        Ok(0)
    );
    let next = EpochBinding { epoch: 3, ..b };
    let fresh = open(&mut v, &mut s, &k, next, r, 12);
    assert_eq!(
        s.retire(b.binding.domain, b.binding.subject, b.epoch),
        Ok(0)
    );
    assert!(s.inspect(old, r, Some(b), 13).is_err());
    assert!(s.inspect(fresh, r, Some(next), 13).is_ok());
    assert!(s.inspect(unrelated, r, Some(other), 13).is_ok());
}
#[test]
fn head_change_alone_keeps_independent_rights_and_factor_change_does_not() {
    let (k, b, r) = setup();
    let mut v = EpochVerifier::new();
    let mut s = EpochSessions::new();
    let id = open(&mut v, &mut s, &k, b, r, 10);
    let changed = EpochBinding {
        binding: Binding {
            head: [9; 32],
            ..b.binding
        },
        ..b
    };
    assert!(s.inspect(id, r, Some(changed), 11).is_ok());
    let factor = EpochBinding {
        binding: Binding {
            reference: [3; 16],
            ..changed.binding
        },
        ..changed
    };
    assert!(s.inspect(id, r, Some(factor), 12).is_err());
    assert!(s.inspect(id, r, Some(changed), 13).is_err());
}
#[test]
fn challenge_binds_epoch_and_consumes_mismatched_attempt() {
    let (k, b, r) = setup();
    let mut v = EpochVerifier::new();
    let f = Possession(k.verifying_key());
    let (token, h) = v.begin(b, r, [6; 16], [7; 32], 10, 15).unwrap();
    let sig = k.sign(&h).to_bytes();
    assert!(
        v.finish(token, r, 11, Some(EpochBinding { epoch: 2, ..b }), &f, &sig)
            .is_err()
    );
    assert!(v.finish(token, r, 11, Some(b), &f, &sig).is_err());
    assert!(
        v.begin(EpochBinding { epoch: 0, ..b }, r, [7; 16], [8; 32], 12, 15)
            .is_err()
    );
}
#[test]
fn expiry_releases_both_tables_and_clock_reversal_never_reopens_them() {
    let (k, b, r) = setup();
    let mut v = EpochVerifier::new();
    let mut s = EpochSessions::new();
    let old = open(&mut v, &mut s, &k, b, r, 10);
    for _ in 1..16 {
        open(&mut v, &mut s, &k, b, r, 10);
    }
    for _ in 0..16 {
        open(&mut v, &mut s, &k, b, r, 110);
    }
    assert!(s.inspect(old, r, Some(b), 110).is_err());
    assert_eq!(s.inspect(old, r, Some(b), 109), Err(Error::Unavailable));
    assert!(s.inspect(old, r, Some(b), 111).is_err());
    for _ in 0..16 {
        open(&mut v, &mut s, &k, b, r, 112);
    }
}

#[test]
fn already_verified_proof_cannot_open_after_retirement() {
    let (k, b, r) = setup();
    let mut v = EpochVerifier::new();
    let mut s = EpochSessions::new();
    let (token, h) = v.begin(b, r, [6; 16], [7; 32], 10, 15).unwrap();
    let proof = v
        .finish(
            token,
            r,
            11,
            Some(b),
            &Possession(k.verifying_key()),
            &k.sign(&h).to_bytes(),
        )
        .unwrap();
    assert_eq!(
        s.retire(b.binding.domain, b.binding.subject, b.epoch),
        Ok(0)
    );
    assert!(s.open(proof, b, 12, 100).is_err()); // même vue courante devenue obsolète
    let (token, h) = v.begin(b, r, [8; 16], [9; 32], 13, 15).unwrap();
    let proof = v
        .finish(
            token,
            r,
            14,
            Some(b),
            &Possession(k.verifying_key()),
            &k.sign(&h).to_bytes(),
        )
        .unwrap();
    assert!(s.open(proof, b, 14, 100).is_err());
    let next = EpochBinding { epoch: 3, ..b };
    let fresh = open(&mut v, &mut s, &k, next, r, 15);
    assert_eq!(
        s.retire(b.binding.domain, b.binding.subject, b.epoch),
        Ok(0)
    );
    assert!(s.inspect(fresh, r, Some(next), 16).is_ok());
}
#[test]
fn saturated_retirement_history_fails_closed_without_eviction() {
    let (k, b, r) = setup();
    let mut v = EpochVerifier::new();
    let mut s = EpochSessions::new();
    let live = open(&mut v, &mut s, &k, b, r, 10);
    for i in 1..=64u8 {
        assert!(s.retire([2; 16], [i; 16], 1).is_ok());
    }
    assert_eq!(s.retire([2; 16], [65; 16], 1), Err(Error::Busy));
    assert!(s.inspect(live, r, Some(b), 11).is_err());
    let (token, h) = v.begin(b, r, [6; 16], [7; 32], 12, 15).unwrap();
    let proof = v
        .finish(
            token,
            r,
            12,
            Some(b),
            &Possession(k.verifying_key()),
            &k.sign(&h).to_bytes(),
        )
        .unwrap();
    assert!(s.open(proof, b, 12, 100).is_err());
}

#[test]
fn deux_verificateurs_partagent_des_sessions_uniques_et_des_routes_isolees() {
    let (ka,a,ra)=setup();
    let kb=SigningKey::from_bytes(&[12;32]);
    let b=EpochBinding{binding:Binding{subject:[10;16],reference:Possession(kb.verifying_key()).reference(a.domain),..a.binding},..a};
    let rb=Route{client:[5;32],..ra};
    let mut va=EpochVerifier::new();let mut vb=EpochVerifier::new();let mut sessions=EpochSessions::new();
    // Même issuer, request, horloge et premier compteur de défi de chaque route.
    let sa=open(&mut va,&mut sessions,&ka,a,ra,10);
    let sb=open(&mut vb,&mut sessions,&kb,b,rb,10);
    assert_ne!(sa,sb);
    assert!(sessions.inspect(sa,rb,Some(a),11).is_err());
    assert!(sessions.inspect(sb,ra,Some(b),11).is_err());
    assert_eq!(sessions.inspect(sa,ra,Some(a),11),Ok(a.subject));
    assert_eq!(sessions.inspect(sb,rb,Some(b),11),Ok(b.subject));
    sessions.disconnect(rb);
    assert!(sessions.inspect(sb,rb,Some(b),12).is_err());
    assert_eq!(sessions.inspect(sa,ra,Some(a),12),Ok(a.subject));
}
