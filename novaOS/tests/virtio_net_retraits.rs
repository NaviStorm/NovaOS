#[path = "../kernel/src/hardware/nvme_domain/traffic/requests.rs"]
mod requests;

use requests::{Key, Owner, Requests};

#[test]
fn refus_avant_acquittement_et_saturation() {
    let mut r = Requests::new();
    let d = Key(3, 1);
    let n = Key(7, 1);
    assert!(r.post(Owner::Nic, n));
    assert!(!r.post(Owner::Nic, Key(9, 2)));
    assert!(!r.collect(Owner::Nic, n));
    assert!(r.pair().is_none());
    assert!(r.post(Owner::Disk, d));
    assert_eq!(r.pair(), Some((d, n)));
    assert!(!r.complete(Owner::Disk, n));
    assert!(!r.complete(Owner::Nic, d));
    assert_eq!(r.pair(), Some((d, n)));
}

#[test]
fn acquittements_individuels_et_reponses_anciennes() {
    for disk_first in [false, true] {
        let mut r = Requests::new();
        for g in 1..=4 {
            let d = Key(3 + 5 * g, g);
            let n = Key(7 + 5 * g, g);
            assert!(r.post(Owner::Disk, d) && r.post(Owner::Nic, n));
            assert!(!r.complete(Owner::Nic, Key(n.0, g - 1)));
            let order = if disk_first {
                [(Owner::Disk, d), (Owner::Nic, n)]
            } else {
                [(Owner::Nic, n), (Owner::Disk, d)]
            };
            for (owner, id) in order {
                assert!(r.complete(owner, id));
                assert!(!r.complete(owner, id));
                assert!(!r.post(owner, id));
                assert!(r.collect(owner, id));
                assert!(!r.collect(owner, id));
            }
        }
    }
}

#[test]
fn quarantaine_conserve_les_deux_identites_sans_recu() {
    let mut r = Requests::new();
    let (d, n) = (Key(3, 1), Key(7, 1));
    assert!(r.post(Owner::Disk, d) && r.post(Owner::Nic, n));
    r.hold_pending();
    for (owner, key) in [(Owner::Disk, d), (Owner::Nic, n)] {
        assert!(r.held(owner, key));
        assert!(!r.complete(owner, key) && !r.collect(owner, key));
        assert!(!r.post(owner, Key(key.0, key.1 + 1)));
    }
    assert!(!r.held(Owner::Disk, n));
}
