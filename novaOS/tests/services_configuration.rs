//! Qualification de la représentation des déclarations autorisées et de leur décision de boot.
#![allow(dead_code)]
#[path = "../user/services/catalog.rs"]
mod catalog;
#[path = "../user/services/policy.rs"]
mod policy;
#[path = "../user/services/settings.rs"]
mod settings;

fn snapshot() -> Vec<u8> {
    let mut out = vec![0; settings::BYTES];
    let used = settings::encode(&settings::defaults(), &mut out).unwrap();
    out.truncate(used);
    out
}

#[test]
fn copie_canonique_et_troncatures() {
    let mut records = settings::defaults();
    records[2].auto = false;
    records[2].restart = policy::Restart::Never;
    records[2].retries = 0;
    records[2].start_ticks = 1000;
    let mut bytes = [0; settings::BYTES];
    let len = settings::encode(&records, &mut bytes).unwrap();
    let read = settings::decode(&bytes[..len]).unwrap();
    assert!(
        read.iter()
            .zip(&records)
            .all(|(a, b)| !settings::differs(a, b))
    );
    assert!(settings::differs(&read[2], &catalog::INSTALLED[2]));
    for end in 0..len {
        assert!(settings::decode(&bytes[..end]).is_none());
    }
    assert!(settings::decode(&bytes[..len + 1]).is_none());
    assert!(settings::encode(&records, &mut bytes[..len - 1]).is_none());
}

#[test]
fn rejets_de_catalogues_non_autorises() {
    let mut bytes = snapshot();
    bytes.resize(settings::BYTES, 0);
    bytes[7] = b'2';
    assert!(settings::decode(&bytes).is_none());
    for mutate in [
        (|r: &mut policy::Declaration| r.version = 2) as fn(&mut policy::Declaration),
        |r| r.id = 99,
        |r| r.image = 15,
        |r| r.caps = 9,
        |r| r.args = &[1],
        |r| r.required = &[99],
        |r| r.required = &[1],
        |r| r.optional = &[2],
        |r| r.start_ticks = 0,
        |r| r.stop_ticks += 1,
        |r| r.heartbeat_ticks = 0,
        |r| r.retries += 1,
        |r| r.restart = policy::Restart::Always,
        |r| r.window = 0,
        |r| r.backoff = 0,
        |r| r.log = false,
    ] {
        let mut records = settings::defaults();
        mutate(&mut records[0]);
        let n = settings::encode(&records, &mut bytes).unwrap_or(0);
        assert!(settings::decode(&bytes[..n]).is_none());
    }
}

#[test]
fn blocage_du_cache_et_demarrage_manuel_du_fournisseur() {
    let mut r = settings::defaults();
    r[2].auto = false;
    let mut p = policy::Policy::configured(&r[..catalog::INSTALLED.len()]).unwrap();
    let mut launched = Vec::new();
    while let Some(action) = p.next_action(1) {
        if let policy::Action::Launch { id, attempt } = action {
            launched.push(id);
            p.ready(id, attempt, 1).unwrap();
        } else {
            panic!("action inattendue {action:?}");
        }
    }
    assert_eq!(launched, [1, 2]);
    assert_eq!(p.state(9), Ok(policy::State::Registered));
    assert_eq!(p.state(10), Ok(policy::State::Waiting));
    assert_eq!(p.blocking_provider(10), Some(9));
    p.start(10).unwrap();
    assert_eq!(p.next_action(2), None);
    assert_eq!(p.state(9), Ok(policy::State::Registered));
    assert_eq!(p.blocking_provider(10), Some(9));
    p.start(9).unwrap();
    assert_eq!(
        p.next_action(2),
        Some(policy::Action::Launch { id: 9, attempt: 1 })
    );
    assert_eq!(p.next_action(2), None);
    p.ready(9, 1, 2).unwrap();
    assert_eq!(
        p.next_action(2),
        Some(policy::Action::Launch { id: 10, attempt: 1 })
    );
    assert_eq!(p.blocking_provider(10), None);
}

#[test]
fn quatre_entrees_historiques_et_cinq_plafonds_verifies() {
    let mut records = settings::defaults();
    records[2].auto = false;
    records[3].retries = 0;
    let mut bytes = [0; settings::BYTES];
    let n = settings::encode_legacy(&records, &mut bytes).unwrap();
    assert!(settings::historical(&bytes[..n]));
    let old = settings::decode(&bytes[..n]).unwrap();
    assert!(!old[2].auto);
    assert_eq!(old[3].retries, 0);
    assert_eq!(old[4].caps, catalog::JOURNAL_STORE);
    records[2].caps = 0;
    let n = settings::encode_legacy(&records, &mut bytes).unwrap();
    assert!(settings::decode(&bytes[..n]).is_none());
    records[4].caps = 0;
    let n = settings::encode(&records, &mut bytes).unwrap();
    assert!(!settings::historical(&bytes[..n]));
    let new = settings::decode(&bytes[..n]).unwrap();
    assert_eq!((new[2].caps, new[4].caps), (0, 0));
    assert!(!new[2].auto);
    assert_eq!(new[3].retries, 0);
}

#[test]
fn dependance_en_attente_apres_retrait_indispensable() {
    use policy::{Action, Cause, State};
    let records = settings::defaults();
    let mut p = policy::Policy::configured(&records).unwrap();
    while let Some(Action::Launch {id, attempt}) = p.next_action(1) {
        assert_ne!(id, 10);
        if id == 9 {
            p.finished(id, attempt, Cause::SecurityPolicy, 5, 1).unwrap();
        } else {
            p.ready(id, attempt, 1).unwrap();
        }
    }
    assert_eq!(p.state(9), Ok(State::Failed));
    assert_eq!(p.state(10), Ok(State::Waiting));
    assert_eq!(p.blocking_provider(10), Some(9));
    assert_eq!(p.next_action(600), None);
    p.start(9).unwrap();
    assert_eq!(p.next_action(601), Some(Action::Launch {id: 9, attempt: 2}));
    assert_eq!(p.next_action(601), None);
    p.ready(9, 2, 601).unwrap();
    assert_eq!(p.next_action(601), Some(Action::Launch {id: 10, attempt: 1}));
}
