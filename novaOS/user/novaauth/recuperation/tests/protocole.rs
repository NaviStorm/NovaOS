use ed25519_dalek::SigningKey;
use novaos_recovery::*;
use rand_chacha::ChaCha20Rng;
use rand_core::SeedableRng;
use zeroize::Zeroizing;

fn rng() -> ChaCha20Rng {
    ChaCha20Rng::from_seed([5; 32])
}
fn fixture() -> (Capture, [PrivateKey; 2], [Vec<u8>; 3]) {
    let mut r = rng();
    let (g, gp) = keypair(&mut r);
    let (a, ap) = keypair(&mut r);
    let mut head = [0; 144];
    head[..16].copy_from_slice(&[7; 16]);
    head[88..96].copy_from_slice(&13u64.to_be_bytes());
    head[112..].copy_from_slice(&[6; 32]);
    (
        Capture {
            server: [1; 16],
            domain: [7; 16],
            backup: [3; 32],
            generation: 13,
            key_epoch: 2,
            recipients: [gp, ap],
            head,
        },
        [g, a],
        [vec![1; 1024], vec![2; 1025], vec![3; 16384]],
    )
}
fn backup() -> (Manifest, [PrivateKey; 2], Vec<(Vec<u8>, [u8; 16])>) {
    let (c, keys, objects) = fixture();
    let mut enc = Encoder::new(&c, objects.each_ref().map(|x| x.len() as u32), &mut rng()).unwrap();
    let mut chunks = Vec::new();
    for p in &objects {
        for v in p.chunks(CHUNK) {
            let mut b = v.to_vec();
            let tag = enc.seal(&mut b).unwrap();
            assert_ne!(v, b);
            chunks.push((b, tag));
        }
    }
    (
        Manifest::parse(&enc.finish().unwrap()).unwrap(),
        keys,
        chunks,
    )
}
#[test]
fn chiffrement_flux_exact_borne() {
    let (m, k, chunks) = backup();
    let shares = [m.share(1, &k[0]).unwrap(), m.share(2, &k[1]).unwrap()];
    let mut dec = Decoder::new(m, &shares).unwrap();
    let mut all = Vec::new();
    for (mut b, t) in chunks {
        dec.open(&mut b, &t).unwrap();
        all.extend(b);
    }
    dec.finish().unwrap();
    let expected: Vec<_> = fixture().2.into_iter().flatten().collect();
    assert_eq!(all, expected);
}
#[test]
fn un_detenteur_ne_dechiffre_pas() {
    for role in 1..=2 {
        let (m, k, chunks) = backup();
        let mut shares = [Zeroizing::new([0; 32]), Zeroizing::new([0; 32])];
        shares[role as usize - 1] = m.share(role, &k[role as usize - 1]).unwrap();
        let mut dec = Decoder::new(m, &shares).unwrap();
        let (mut b, t) = chunks[0].clone();
        assert_eq!(dec.open(&mut b, &t), Err(Error::Crypto));
        assert!(b.iter().all(|&x| x == 0));
        assert!(dec.finish().is_err());
    }
}
#[test]
fn manifeste_canonique_et_complet() {
    let (m, keys, _) = backup();
    assert_eq!(m.bytes().len(), 418);
    for n in [0, 417, 419] {
        let mut b = m.bytes().to_vec();
        b.resize(n, 0);
        assert!(Manifest::parse(&b).is_err());
    }
    for i in [0, 8, 158, 246, 382] {
        let mut b = *m.bytes();
        b[i] ^= 128;
        assert!(Manifest::parse(&b).is_err());
    }
    assert!(m.share(1, &keys[1]).is_err());
    assert!(m.share(2, &keys[0]).is_err());
    assert!(m.share(0, &keys[0]).is_err());
    let (mut capture, _, _) = fixture();
    capture.recipients[1] = capture.recipients[0];
    assert!(Encoder::new(&capture, [1, 1, 1], &mut rng()).is_err());
    for sizes in [[0, 1, 1], [16385, 1, 1], [u32::MAX, 1, 1]] {
        assert!(Encoder::new(&fixture().0, sizes, &mut rng()).is_err());
    }
}
#[test]
fn fragments_alteres_reordonnes_absents() {
    for mode in 0..5 {
        let (m, k, mut chunks) = backup();
        let shares = [m.share(1, &k[0]).unwrap(), m.share(2, &k[1]).unwrap()];
        match mode {
            0 => chunks[0].0[0] ^= 1,
            1 => chunks[0].1[0] ^= 1,
            2 => chunks.swap(0, 1),
            3 => {
                chunks.remove(0);
            }
            _ => {
                chunks.pop();
            }
        }
        let mut dec = Decoder::new(m, &shares).unwrap();
        let mut failed = false;
        for (mut b, t) in chunks {
            if dec.open(&mut b, &t).is_err() {
                failed = true;
                assert!(b.iter().all(|&x| x == 0));
                break;
            }
        }
        assert!(failed || dec.finish().is_err());
    }
}
fn context() -> DeliveryContext {
    DeliveryContext {
        recovery: [3; 32],
        manifest: [4; 32],
        epoch: 2,
        delivery: [5; 16],
        nonce: [6; 32],
    }
}
#[test]
fn remises_independantes_idempotentes() {
    let (m, k, _) = backup();
    let (recipient, pk) = keypair(&mut rng());
    let g = SigningKey::from_bytes(&[71; 32]);
    let a = SigningKey::from_bytes(&[72; 32]);
    let shares = [m.share(1, &k[0]).unwrap(), m.share(2, &k[1]).unwrap()];
    let mut ctx = context();
    ctx.manifest = m.digest();
    let mut box_g = seal_share(1, &ctx, &shares[0], &pk, &g, &mut rng()).unwrap();
    let box_a = seal_share(2, &ctx, &shares[1], &pk, &a, &mut rng()).unwrap();
    assert_eq!(box_g.len(), 324);
    let mut slots = Shares::default();
    assert!(
        slots
            .receive(1, &ctx, &recipient, &g.verifying_key(), &box_g)
            .unwrap()
    );
    assert!(
        !slots
            .receive(1, &ctx, &recipient, &g.verifying_key(), &box_g)
            .unwrap()
    );
    assert!(
        slots
            .receive(2, &ctx, &recipient, &a.verifying_key(), &box_a)
            .unwrap()
    );
    let got = slots.take().unwrap();
    assert_eq!(got[0], shares[0]);
    assert_eq!(got[1], shares[1]);
    box_g[110] ^= 1;
    assert!(open_share(1, &ctx, &recipient, &g.verifying_key(), &box_g).is_err());
}
#[test]
fn mauvaises_parts_contexte_et_cible() {
    let (recipient, pk) = keypair(&mut rng());
    let sender = SigningKey::from_bytes(&[90; 32]);
    let ctx = context();
    let secret = Zeroizing::new([99; 32]);
    let envelope = seal_share(1, &ctx, &secret, &pk, &sender, &mut rng()).unwrap();
    assert!(open_share(2, &ctx, &recipient, &sender.verifying_key(), &envelope).is_err());
    assert!(
        open_share(
            1,
            &ctx,
            &private(&[9; 32]).unwrap(),
            &sender.verifying_key(),
            &envelope
        )
        .is_err()
    );
    assert!(
        open_share(
            1,
            &ctx,
            &recipient,
            &SigningKey::from_bytes(&[1; 32]).verifying_key(),
            &envelope
        )
        .is_err()
    );
    for n in [0, 323, 325] {
        let mut b = envelope.to_vec();
        b.resize(n, 0);
        assert!(open_share(1, &ctx, &recipient, &sender.verifying_key(), &b).is_err());
    }
    for field in 0..5 {
        let mut wrong = context();
        match field {
            0 => wrong.recovery[0] ^= 1,
            1 => wrong.manifest[0] ^= 1,
            2 => wrong.epoch += 1,
            3 => wrong.delivery[0] ^= 1,
            _ => wrong.nonce[0] ^= 1,
        };
        assert!(open_share(1, &wrong, &recipient, &sender.verifying_key(), &envelope).is_err());
    }
    let mut slots = Shares::default();
    slots
        .receive(1, &ctx, &recipient, &sender.verifying_key(), &envelope)
        .unwrap();
    assert!(slots.take().is_err());
}
#[test]
fn deux_operations_ne_peuvent_se_combiner() {
    let (recipient, pk) = keypair(&mut rng());
    let sender = SigningKey::from_bytes(&[90; 32]);
    let ctx = context();
    let secret = Zeroizing::new([99; 32]);
    let g = seal_share(1, &ctx, &secret, &pk, &sender, &mut rng()).unwrap();
    let mut other = context();
    other.recovery[0] ^= 1;
    let a = seal_share(2, &other, &secret, &pk, &sender, &mut rng()).unwrap();
    let mut slots = Shares::default();
    slots
        .receive(1, &ctx, &recipient, &sender.verifying_key(), &g)
        .unwrap();
    assert_eq!(
        slots.receive(2, &other, &recipient, &sender.verifying_key(), &a),
        Err(Error::Scope)
    );
    assert!(slots.take().is_err());
}
