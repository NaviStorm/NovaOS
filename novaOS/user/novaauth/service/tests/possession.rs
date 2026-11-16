use ed25519_dalek::{Signer, SigningKey};
use novaos_auth_session::{Binding, Error, Factor, Possession, Route, Verifier};

fn fixtures() -> (SigningKey, Binding, Route) {
    // Vecteur public de laboratoire. Aucun binaire invité ne lie ce test hôte.
    let key = SigningKey::from_bytes(&[1; 32]);
    let domain = [3; 16];
    let reference = Possession(key.verifying_key()).reference(domain);
    (
        key,
        Binding {
            domain,
            subject: [4; 16],
            reference,
            head: [9; 32],
        },
        Route {
            client: [5; 32],
            issuer: [6; 32],
        },
    )
}

#[test]
fn canal_expiration_rejeu_et_changement_de_generation() {
    let (key, account, route) = fixtures();
    let factor = Possession(key.verifying_key());
    let mut auth = Verifier::new();
    let (id, challenge) = auth
        .begin(account, route, [1; 16], [7; 32], 10, 50)
        .unwrap();
    let signature = key.sign(&challenge).to_bytes();
    let other = Route {
        client: [8; 32],
        ..route
    };
    assert!(matches!(
        auth.finish(id, other, 11, Some(account), &factor, &signature),
        Err(Error::Refused)
    ));
    assert_eq!(
        auth.finish(id, route, 12, Some(account), &factor, &signature)
            .unwrap()
            .subject(),
        account.subject
    );
    assert!(
        auth.finish(id, route, 13, Some(account), &factor, &signature)
            .is_err()
    );
    let (id, challenge) = auth
        .begin(account, route, [1; 16], [7; 32], 14, 50)
        .unwrap();
    let signature = key.sign(&challenge).to_bytes();
    assert!(
        auth.finish(id, route, 64, Some(account), &factor, &signature)
            .is_err()
    );
    for current in [
        None,
        Some(Binding {
            head: [10; 32],
            ..account
        }),
    ] {
        let (id, challenge) = auth
            .begin(account, route, [1; 16], [7; 32], 65, 50)
            .unwrap();
        assert!(
            auth.finish(
                id,
                route,
                66,
                current,
                &factor,
                &key.sign(&challenge).to_bytes()
            )
            .is_err()
        );
        assert!(
            auth.finish(
                id,
                route,
                67,
                Some(account),
                &factor,
                &key.sign(&challenge).to_bytes()
            )
            .is_err()
        );
        if current.is_none() {
            auth = Verifier::new();
        }
    }
}

#[test]
fn substitution_et_reponses_concurrentes() {
    let (key, account, route) = fixtures();
    let factor = Possession(key.verifying_key());
    let mut auth = Verifier::new();
    let (first, one) = auth
        .begin(account, route, [1; 16], [7; 32], 10, 50)
        .unwrap();
    assert!(matches!(
        auth.begin(account, route, [1; 16], [9; 32], 11, 50),
        Err(Error::Busy)
    ));
    let (second, two) = auth
        .begin(account, route, [2; 16], [7; 32], 12, 50)
        .unwrap();
    assert_ne!(one, two);
    assert!(
        auth.finish(
            second,
            route,
            13,
            Some(account),
            &factor,
            &key.sign(&one).to_bytes()
        )
        .is_err()
    );
    let impostor = SigningKey::from_bytes(&[2; 32]);
    assert!(
        auth.finish(
            first,
            route,
            14,
            Some(account),
            &Possession(impostor.verifying_key()),
            &impostor.sign(&one).to_bytes()
        )
        .is_err()
    );
    for n in 1..=8 {
        auth.begin(account, route, [n; 16], [n; 32], 15, 5).unwrap();
    }
    assert!(matches!(
        auth.begin(account, route, [9; 16], [9; 32], 15, 5),
        Err(Error::Busy)
    ));
    assert!(auth.begin(account, route, [9; 16], [9; 32], 20, 5).is_ok());
}

#[test]
fn horloge_regressive_ferme_les_tentatives() {
    let (key, account, route) = fixtures();
    let mut auth = Verifier::new();
    let (token, challenge) = auth
        .begin(account, route, [1; 16], [7; 32], 100, 20)
        .unwrap();
    let response = key.sign(&challenge).to_bytes();
    let factor = Possession(key.verifying_key());
    assert!(matches!(
        auth.finish(token, route, 99, Some(account), &factor, &response),
        Err(Error::Unavailable)
    ));
    assert!(
        auth.finish(token, route, 100, Some(account), &factor, &response)
            .is_err()
    );
}
