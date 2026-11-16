use ed25519_dalek::{Signer, SigningKey};
use novaos_auth_session::{Binding, Factor, Possession, Route, Verifier, sessions::Sessions};

#[test]
fn limites_filiation_fermeture_expiration_et_restitution() {
    let k = SigningKey::from_bytes(&[11; 32]);
    let f = Possession(k.verifying_key());
    let b = Binding {
        domain: [1; 16],
        subject: [8; 16],
        reference: f.reference([1; 16]),
        head: [3; 32],
    };
    let a = Route {
        client: [4; 32],
        issuer: [9; 32],
    };
    let other = Route {
        client: [5; 32],
        ..a
    };
    let mut verifier = Verifier::new();
    let mut book = Sessions::new();
    let mut ids = Vec::new();
    let mut open = |book: &mut Sessions, now: u64| {
        let (t, h) = verifier.begin(b, a, [6; 16], [7; 32], now, 15).unwrap();
        let p = verifier
            .finish(t, a, now, Some(b), &f, &k.sign(&h).to_bytes())
            .unwrap();
        book.open(p, b, now, 100)
    };
    for _ in 0..16 {
        ids.push(open(&mut book, 10).unwrap());
    }
    assert!(open(&mut book, 10).is_err());
    assert!(book.close(ids[0], other).is_err());
    assert!(book.inspect(ids[0], other, Some(b), 11).is_err());
    assert_eq!(book.inspect(ids[0], a, Some(b), 11).unwrap(), b.subject);
    book.close(ids[0], a).unwrap();
    let next = open(&mut book, 12).unwrap();
    assert!(!ids.contains(&next));
    assert!(book.inspect(ids[0], a, Some(b), 13).is_err());
    assert!(book.inspect(ids[1], a, Some(b), 110).is_err());
    assert!(book.inspect(next, a, None, 110).is_err());
    let fresh = open(&mut book, 111).unwrap();
    book.disconnect(a);
    assert!(book.inspect(fresh, a, Some(b), 112).is_err());
    for _ in 0..16 {
        open(&mut book, 113).unwrap();
    }
}
