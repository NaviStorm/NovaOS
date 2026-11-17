use ed25519_dalek::{Signer, SigningKey};
use novaos_recovery::*;
use rand_chacha::ChaCha20Rng;
use rand_core::SeedableRng;
use zeroize::Zeroizing;

fn fixture() -> (
    RecoveryContextV2,
    [PrivateKey; 2],
    SigningKey,
    [SigningKey; 2],
    ChaCha20Rng,
) {
    let mut rng = ChaCha20Rng::from_seed([101; 32]);
    let (g, gp) = keypair(&mut rng);
    let (a, ap) = keypair(&mut rng);
    let possession = SigningKey::from_bytes(&[9; 32]);
    let senders = [
        SigningKey::from_bytes(&[11; 32]),
        SigningKey::from_bytes(&[12; 32]),
    ];
    let mut b = [1; 376];
    b[..8].copy_from_slice(b"NARECV02");
    b[88..120].fill(2);
    for (at, value) in [(120, 0u64), (128, 13), (200, 2), (240, 1_000_000_000)] {
        b[at..at + 8].copy_from_slice(&value.to_be_bytes());
    }
    b[248..280].copy_from_slice(&gp);
    b[280..312].copy_from_slice(&ap);
    b[312..344].copy_from_slice(possession.verifying_key().as_bytes());
    (
        RecoveryContextV2::parse(&b).unwrap(),
        [g, a],
        possession,
        senders,
        rng,
    )
}

#[test]
fn requete_exacte_pour_chaque_detenteur() {
    let (recovery, _, possession, _, mut rng) = fixture();
    let requests = [keypair(&mut rng), keypair(&mut rng)];
    let context = recovery.delivery([3; 16], [4; 32]);
    for role in 1..=2 {
        let index = role as usize - 1;
        let envelope =
            seal_request(role, &context, &requests[index].1, &possession, &mut rng).unwrap();
        assert_eq!(envelope.len(), 292);
        assert_eq!(
            &envelope[..10],
            &[b'N', b'A', b'S', b'H', b'R', b'Q', b'0', b'1', 1, role]
        );
        assert_eq!(
            open_request(
                role,
                &context,
                &requests[index].0,
                &possession.verifying_key(),
                &envelope
            ),
            Ok(())
        );
        assert!(
            open_request(
                role,
                &context,
                &requests[1 - index].0,
                &possession.verifying_key(),
                &envelope
            )
            .is_err()
        );
        assert!(
            open_request(
                3 - role,
                &context,
                &requests[index].0,
                &possession.verifying_key(),
                &envelope
            )
            .is_err()
        );
    }
}

#[test]
fn chaque_octet_de_requete_est_protege() {
    let (recovery, _, possession, _, mut rng) = fixture();
    let (secret, public) = keypair(&mut rng);
    let context = recovery.delivery([3; 16], [4; 32]);
    let envelope = seal_request(1, &context, &public, &possession, &mut rng).unwrap();
    for index in 0..292 {
        let mut changed = envelope;
        changed[index] ^= 1;
        assert!(
            open_request(1, &context, &secret, &possession.verifying_key(), &changed).is_err(),
            "{index}"
        );
    }
    for len in [0, 1, 291, 293, 512] {
        let mut changed = envelope.to_vec();
        changed.resize(len, 0);
        assert!(open_request(1, &context, &secret, &possession.verifying_key(), &changed).is_err());
    }
    // Le détenteur vérifie l'AEAD même si une signature extérieure est valide.
    let mut changed = envelope;
    changed[212] ^= 1;
    let signature = possession.sign(&changed[..228]);
    changed[228..].copy_from_slice(&signature.to_bytes());
    assert!(open_request(1, &context, &secret, &possession.verifying_key(), &changed).is_err());
}

#[test]
fn requete_refuse_contexte_ou_signature_substitues() {
    let (recovery, _, possession, senders, mut rng) = fixture();
    let (secret, public) = keypair(&mut rng);
    let context = recovery.delivery([3; 16], [4; 32]);
    let envelope = seal_request(1, &context, &public, &possession, &mut rng).unwrap();
    assert!(open_request(1, &context, &secret, &senders[0].verifying_key(), &envelope).is_err());
    for field in 0..5 {
        let mut changed = recovery.delivery([3; 16], [4; 32]);
        match field {
            0 => changed.recovery[0] ^= 1,
            1 => changed.manifest[0] ^= 1,
            2 => changed.epoch += 1,
            3 => changed.delivery[0] ^= 1,
            _ => changed.nonce[0] ^= 1,
        }
        assert!(
            open_request(1, &changed, &secret, &possession.verifying_key(), &envelope).is_err()
        );
    }
}

#[test]
fn contexte_v2_exact_et_refus_v1() {
    let (context, _, _, _, _) = fixture();
    assert_eq!(context.bytes().len(), 376);
    for len in [0, 344, 375, 377] {
        let mut b = context.bytes().to_vec();
        b.resize(len, 0);
        assert!(RecoveryContextV2::parse(&b).is_err());
    }
    for at in [0, 7, 120, 128, 200, 240] {
        let mut b = *context.bytes();
        b[at] ^= 128;
        assert!(RecoveryContextV2::parse(&b).is_err());
    }
    let mut b = *context.bytes();
    b[280..312].copy_from_slice(&context.recipient(1).unwrap());
    assert!(RecoveryContextV2::parse(&b).is_err());
}

#[test]
fn deux_destinataires_et_collecte_unique() {
    let (recovery, keys, _, senders, mut rng) = fixture();
    let mut slots =
        SharesV2::new(&recovery, senders.each_ref().map(SigningKey::verifying_key)).unwrap();
    for role in 1..=2 {
        let index = role as usize - 1;
        let context = recovery.delivery([role; 16], [role + 4; 32]);
        let share = Zeroizing::new([role + 20; 32]);
        let envelope = seal_share(
            role,
            &context,
            &share,
            &recovery.recipient(role).unwrap(),
            &senders[index],
            &mut rng,
        )
        .unwrap();
        assert!(
            open_share(
                role,
                &context,
                &keys[1 - index],
                &senders[index].verifying_key(),
                &envelope
            )
            .is_err()
        );
        assert!(
            slots
                .receive(role, &context, &keys[index], &envelope)
                .unwrap()
        );
        assert!(
            !slots
                .receive(role, &context, &keys[index], &envelope)
                .unwrap()
        );
        let changed = seal_share(
            role,
            &context,
            &share,
            &recovery.recipient(role).unwrap(),
            &senders[index],
            &mut rng,
        )
        .unwrap();
        assert_eq!(
            slots.receive(role, &context, &keys[index], &changed),
            Err(Error::Duplicate)
        );
    }
    let values = slots.take().unwrap();
    assert_eq!(*values[0], [21; 32]);
    assert_eq!(*values[1], [22; 32]);
}

#[test]
fn part_isolee_et_melange_v2_refuses() {
    let (recovery, keys, _, senders, mut rng) = fixture();
    let mut slots =
        SharesV2::new(&recovery, senders.each_ref().map(SigningKey::verifying_key)).unwrap();
    let context = recovery.delivery([1; 16], [2; 32]);
    let share = Zeroizing::new([3; 32]);
    let envelope = seal_share(
        1,
        &context,
        &share,
        &recovery.recipient(1).unwrap(),
        &senders[0],
        &mut rng,
    )
    .unwrap();
    assert!(slots.receive(1, &context, &keys[0], &envelope).unwrap());
    let mut other = recovery.delivery([3; 16], [4; 32]);
    other.recovery[0] ^= 1;
    let second = seal_share(
        2,
        &other,
        &share,
        &recovery.recipient(2).unwrap(),
        &senders[1],
        &mut rng,
    )
    .unwrap();
    assert_eq!(
        slots.receive(2, &other, &keys[1], &second),
        Err(Error::Scope)
    );
    assert!(slots.take().is_err());
}
