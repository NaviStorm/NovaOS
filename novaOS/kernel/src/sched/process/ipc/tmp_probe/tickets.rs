use super::*;

pub(super) fn ticket_race(boot: &TicketBoot, issuer: Pid) {
    const MEDIA: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/storage-media.bin"));
    let parent = ready(210);
    let issue = |ttl: u64| {
        let image = storage::seal_fixture(MEDIA);
        let binding = exec::Binding {
            session: [1; 32],
            domain: [2; 16],
            holder: [3; 16],
            installation: [4; 16],
            epoch: 1,
            app: (1 << 63) | 1,
            object: image.object(),
            digest: image.digest(),
        };
        let mut config = alloc::vec![0; 16];
        config[8..].copy_from_slice(&101u64.to_le_bytes());
        let deadline = ticks() + ttl;
        let key = with(|s| {
            let permit = exec::Permit::fixture(boot, s, issuer, parent.0, binding)?;
            exec::issue(s, permit, image, config, deadline)
        })
        .unwrap();
        (key, binding, deadline)
    };
    let (key, _, _) = issue(60 * u64::from(HZ));
    assert_eq!(with(|s| exec::outcome(s, issuer, key)).err(), Some(DENIED));
    word(&parent, 1, Some(key));
    word(&parent, 5, Some(1));
    await_condition(|| word(&parent, 4, None) == 2);
    let t0 = word(&parent, 10, None);
    let t1 = word(&parent, 11, None);
    let a = word(&parent, 6, None);
    let b = word(&parent, 7, None);
    assert_ne!(t0, t1);
    assert!(matches!((a, b), (OK, REFUSED) | (REFUSED, OK)));
    let (child, wait) = with(|s| exec::outcome(s, parent.0, key)).unwrap();
    hal::log(format_args!(
        "EXEC ticket pid={} tid0={t0} status0={a} tid1={t1} status1={b} enfant={}",
        parent.0.number, child.number
    ));
    assert_eq!(wait.unwrap() & !(1 << 63), child.number);
    word(&parent, 5, Some(2));
    await_condition(|| word(&parent, 4, None) == 3);
    assert_eq!(with(|s| exec::outcome(s, parent.0, key)), Ok((child, None)));
    with(|s| exec::acknowledge(s, parent.0, key)).unwrap();
    let (revoked, binding, _) = issue(60 * u64::from(HZ));
    assert_ne!(key, revoked);
    let mut other = binding;
    other.session[31] ^= 1;
    assert_eq!(
        with(|s| exec::revoke(s, issuer, parent.0, other)),
        Err(DENIED)
    );
    assert_eq!(
        with(|s| exec::revoke(s, parent.0, parent.0, binding)),
        Err(DENIED)
    );
    with(|s| exec::revoke(s, issuer, parent.0, binding)).unwrap();
    word(&parent, 1, Some(revoked));
    word(&parent, 5, Some(3));
    await_condition(|| word(&parent, 4, None) == 4);
    with(|s| exec::acknowledge(s, parent.0, revoked)).unwrap();
    let (expired, _, deadline) = issue(4);
    await_condition(|| ticks() >= deadline);
    word(&parent, 1, Some(expired));
    word(&parent, 5, Some(4));
    assert_eq!(parent.join(), Outcome::Exit(0));
    hal::log(format_args!(
        "EXEC ticket anciennes formes refusees recu relu collecte unique revocation expiration OK"
    ));
}

pub(super) fn contention(writer: &Child) {
    let other = ready(213);
    let old = ready(212);
    let cap = word(&old, 1, None);
    let h = word(&old, 2, None);
    let family = with(|s| s.processes.get(old.0).instance);
    word(writer, 17, Some(1));
    await_condition(|| word(writer, 18, None) == 1);
    let before = word(writer, 14, None);
    let held = word(writer, 15, None);
    assert!(held < before);
    with(|s| {
        assert!(!tmp::inaccessible(s, old.0, cap));
        stop_process(s, old.0, Outcome::Exit(125), hal::timer::counter());
        assert!(tmp::inaccessible(s, old.0, cap));
    });
    assert_eq!(old.join(), Outcome::Exit(125));
    await_condition(|| tmp::removal_acknowledged(family));
    word(&other, 3, Some(cap));
    word(&other, 6, Some(h));
    word(&other, 5, Some(1));
    await_condition(|| word(&other, 4, None) == 2);
    word(writer, 17, Some(2));
    await_condition(|| word(writer, 18, None) == 2);
    let free = word(writer, 16, None);
    hal::log(format_args!(
        "TMP mesure avant={before} reserve={held} libre={free}"
    ));
    assert!(free > before); // Les huit LBA sont vérifiés individuellement par l’oracle.
    word(&other, 5, Some(2));
    assert_eq!(other.join(), Outcome::Exit(0));
    hal::log(format_args!(
        "TMP contention blocs_avant={before} disponibles_reservation={held} apres_drainage={free} ancien_handle_refuse autre_contenu_verifie"
    ));
    word(writer, 17, Some(3));
}
