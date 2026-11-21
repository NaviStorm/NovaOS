#![cfg(feature = "administration-lab")]
//! Témoins hôtes ; le Ledger simulé ne qualifie pas l'autorité indépendante.
use ed25519_dalek::{Signer, SigningKey};
use novafs::{Engine, Ids, Mode, device::SimDevice};
use novaos_identity::{
    administration::{Operation, RETIRED_PREFIX, Retired, Selector},
    recuperation::Triplet,
    recuperation_store::{FRAGMENT, Store},
    recuperation_view::View,
    *,
};
const DOMAIN: Id = [41; 16];
const SERVER: Id = [42; 16];
const IDS: Ids = Ids {
    fs: [31; 16],
    lv: [32; 16],
};
struct Fixture {
    fs: Engine<SimDevice>,
    authority: lab::Ledger<SimDevice>,
    admin: Id,
    alice: Id,
    group: Id,
}
impl Fixture {
    fn new(sector: u32) -> Self {
        let mut authority = lab::Ledger::new(SimDevice::new(32, sector).unwrap());
        let mut fs = provision(
            SimDevice::new(1024, sector).unwrap(),
            IDS,
            DOMAIN,
            [5; 16],
            &mut authority,
        )
        .unwrap();
        let e = Enrollment::from_provisioner(&mut authority).unwrap();
        let mut r = Registry::open(&mut fs, &mut authority, DOMAIN).unwrap();
        let h = r.enroll(&e, [1; 16], "admin", [1; 16], "base").unwrap();
        let admin = created_user(&h);
        let group = created_group(&h);
        let h = r
            .mutate(
                r.resolve_verified_factor([1; 16]).unwrap(),
                [2; 16],
                Change::Create {
                    name: "alice",
                    credential: [2; 16],
                    group,
                },
            )
            .unwrap();
        Self {
            fs,
            authority,
            admin,
            alice: created_user(&h),
            group,
        }
    }
    fn view(&self) -> View {
        view(&Triplet::capture(&self.fs, &self.authority, DOMAIN).unwrap())
    }
    fn publish(&mut self, p: &Triplet) {
        let old = self.authority.current().unwrap();
        let head = *p.head();
        let parts = p.parts();
        let mut s = Store::begin(
            &mut self.fs,
            &Triplet::head_bytes(&head),
            head.request,
            parts.map(|p| p.len() as u32),
            DOMAIN,
        )
        .unwrap();
        for (table, part) in parts.iter().enumerate() {
            for (i, bytes) in part.chunks(FRAGMENT).enumerate() {
                s.append(
                    &mut self.fs,
                    head.request,
                    table,
                    (i * FRAGMENT) as u32,
                    bytes,
                )
                .unwrap();
            }
        }
        assert_eq!(s.seal(&self.fs, head.request).unwrap(), head);
        self.authority.decide(old, head).unwrap();
    }
}
fn view(p: &Triplet) -> View {
    let parts = p.parts();
    View::read(*p.head(), parts.map(|p| p.len() as u32), |t, o, b| {
        let start = o as usize;
        let n = b.len().min(parts[t].len() - start);
        b[..n].copy_from_slice(&parts[t][start..start + n]);
        Ok(n)
    })
    .unwrap()
}
fn body(subject: Id, epoch: u64, request: Id, generation: u64, previous: Hash) -> [u8; 128] {
    let mut b = [0; 128];
    b[..16].copy_from_slice(&SERVER);
    b[16..32].copy_from_slice(&DOMAIN);
    b[32..48].copy_from_slice(&subject);
    b[48..56].copy_from_slice(&epoch.to_be_bytes());
    b[56..72].copy_from_slice(&request);
    b[72..80].copy_from_slice(&generation.to_be_bytes());
    b[80..88].copy_from_slice(&9u64.to_be_bytes());
    b[88] = 1;
    b[96..].copy_from_slice(&previous);
    b
}
fn sign(key: &SigningKey, b: &[u8]) -> [u8; 64] {
    let mut m = RETIRED_PREFIX.to_vec();
    m.extend_from_slice(b);
    key.sign(&m).to_bytes()
}
#[test]
fn persistence_reactivation_requires_exact_authenticated_terminal_receipt() {
    for sector in [512, 4096] {
        let mut f = Fixture::new(sector);
        let before = f.view();
        let ag = before.resolve_verified_factor([1; 16]).unwrap();
        let epoch = before
            .authentication_epoch(before.resolve_verified_factor([2; 16]).unwrap())
            .unwrap();
        let disabled = before
            .administration_plan(ag, [3; 16], Operation::Disable { subject: f.alice }, None)
            .unwrap();
        // Plan seul : le compte reste actif dans la tête publiée.
        assert!(f.view().resolve_verified_factor([2; 16]).is_ok());
        f.publish(&disabled);
        let state = f.view();
        assert!(state.resolve_verified_factor([2; 16]).is_err());
        let ag = state.resolve_verified_factor([1; 16]).unwrap();
        assert!(
            state
                .administration_plan(
                    ag,
                    [4; 16],
                    Operation::Reactivate { subject: f.alice },
                    None
                )
                .is_err()
        );
        let key = SigningKey::from_bytes(&[27; 32]);
        let packet = body(
            f.alice,
            epoch,
            [3; 16],
            disabled.head().generation,
            before.head().digest,
        );
        let signature = sign(&key, &packet);
        let proof = Retired::verify(SERVER, &key.verifying_key(), &packet, &signature).unwrap();
        let enabled = state
            .administration_plan(
                ag,
                [4; 16],
                Operation::Reactivate { subject: f.alice },
                Some(&proof),
            )
            .unwrap();
        f.publish(&enabled);
        let fs = Engine::mount_current(f.fs.unmount().unwrap(), IDS, Mode::ReadWrite).unwrap();
        f.fs = fs;
        let current = f.view();
        let grant = current.resolve_verified_factor([2; 16]).unwrap();
        assert!(current.authentication_epoch(grant).unwrap() > epoch);
        let user = current.user_info(grant, None).unwrap();
        assert_eq!(user.id, f.alice);
        assert_eq!(user.name, "alice");
        assert_eq!(user.groups, vec![f.group]);
        assert_eq!(user.status, Status::Active);
        assert!(
            current
                .administration_plan(
                    current.resolve_verified_factor([1; 16]).unwrap(),
                    [5; 16],
                    Operation::Reactivate { subject: f.alice },
                    Some(&proof)
                )
                .is_err()
        );
        assert!(current.user_info(ag, None).is_err());
    }
}
#[test]
fn no_self_promotion_no_admin_disable_and_no_directory_leak() {
    let f = Fixture::new(512);
    let v = f.view();
    let admin = v.resolve_verified_factor([1; 16]).unwrap();
    let alice = v.resolve_verified_factor([2; 16]).unwrap();
    assert!(
        v.administration_plan(
            admin,
            [3; 16],
            Operation::Disable { subject: f.admin },
            None
        )
        .is_err()
    );
    assert!(
        v.administration_plan(
            alice,
            [3; 16],
            Operation::CreateGroup { name: "admin" },
            None
        )
        .is_err()
    );
    assert!(v.user_info(alice, Some(Selector::Id(f.admin))).is_err());
    assert_eq!(
        v.group_info(alice, Selector::Id(f.group)).unwrap().members,
        vec![f.alice]
    );
    assert_eq!(
        v.group_info(admin, Selector::Id(f.group))
            .unwrap()
            .members
            .len(),
        2
    );
    let mut authority = f.authority;
    let mut fs = f.fs;
    let mut r = Registry::open(&mut fs, &mut authority, DOMAIN).unwrap();
    assert!(matches!(
        r.mutate(
            r.resolve_verified_factor([1; 16]).unwrap(),
            [3; 16],
            Change::Delete(f.admin)
        ),
        Err(Fault::Denied)
    ));
}
#[test]
fn ordinary_rename_and_membership_do_not_rotate_authentication_epoch() {
    let mut f = Fixture::new(4096);
    let v = f.view();
    let epoch = v
        .authentication_epoch(v.resolve_verified_factor([2; 16]).unwrap())
        .unwrap();
    let group = v
        .administration_plan(
            v.resolve_verified_factor([1; 16]).unwrap(),
            [3; 16],
            Operation::CreateGroup { name: "g" },
            None,
        )
        .unwrap();
    let g = created_group(group.head());
    f.publish(&group);
    for (id, op) in [
        (
            4,
            Operation::AddMember {
                subject: f.alice,
                group: g,
            },
        ),
        (
            5,
            Operation::RemoveMember {
                subject: f.alice,
                group: g,
            },
        ),
        (
            6,
            Operation::Rename {
                subject: f.alice,
                name: "alice-renommee",
            },
        ),
    ] {
        let v = f.view();
        let p = v
            .administration_plan(
                v.resolve_verified_factor([1; 16]).unwrap(),
                [id; 16],
                op,
                None,
            )
            .unwrap();
        f.publish(&p);
        let v = f.view();
        assert_eq!(
            v.authentication_epoch(v.resolve_verified_factor([2; 16]).unwrap())
                .unwrap(),
            epoch
        );
    }
    assert_eq!(
        f.view()
            .user_info(f.view().resolve_verified_factor([2; 16]).unwrap(), None)
            .unwrap()
            .groups,
        vec![f.group]
    );
}
#[test]
fn invalid_receipts_and_substitution_are_rejected() {
    let mut f = Fixture::new(512);
    let old = f.view();
    let ag = old.resolve_verified_factor([1; 16]).unwrap();
    let p = old
        .administration_plan(ag, [3; 16], Operation::Disable { subject: f.alice }, None)
        .unwrap();
    f.publish(&p);
    let v = f.view();
    let ag = v.resolve_verified_factor([1; 16]).unwrap();
    let key = SigningKey::from_bytes(&[27; 32]);
    let b = body(f.alice, 2, [3; 16], 3, old.head().digest);
    let sig = sign(&key, &b);
    for i in [0, 16, 32, 48, 56, 72, 80, 88, 89, 96, 127] {
        let mut altered = b;
        altered[i] ^= 1;
        assert!(Retired::verify(SERVER, &key.verifying_key(), &altered, &sig).is_err());
    }
    assert!(Retired::verify([4; 16], &key.verifying_key(), &b, &sig).is_err());
    assert!(Retired::verify(SERVER, &key.verifying_key(), &b[..127], &sig).is_err());
    for i in [16, 32, 48, 56, 72, 96] {
        let mut substituted = b;
        substituted[i] ^= 1;
        let proof = Retired::verify(
            SERVER,
            &key.verifying_key(),
            &substituted,
            &sign(&key, &substituted),
        )
        .unwrap();
        assert!(
            v.administration_plan(
                ag,
                [4; 16],
                Operation::Reactivate { subject: f.alice },
                Some(&proof)
            )
            .is_err()
        );
    }
    let mut ongoing = b;
    ongoing[88] = 0;
    assert!(
        Retired::verify(
            SERVER,
            &key.verifying_key(),
            &ongoing,
            &sign(&key, &ongoing)
        )
        .is_err()
    );
}
#[test]
fn prepared_concurrent_plans_do_not_grant_publication_or_replace_current_head() {
    let mut f = Fixture::new(512);
    let v = f.view();
    let ag = v.resolve_verified_factor([1; 16]).unwrap();
    let a = v
        .administration_plan(ag, [3; 16], Operation::CreateGroup { name: "g1" }, None)
        .unwrap();
    let b = v
        .administration_plan(ag, [4; 16], Operation::CreateGroup { name: "g2" }, None)
        .unwrap();
    f.publish(&a);
    assert!(matches!(
        f.authority.decide(v.head(), *b.head()),
        Err(Fault::Conflict)
    ));
    assert_eq!(f.authority.current().unwrap(), *a.head());
    assert!(
        f.view()
            .group_info(
                f.view().resolve_verified_factor([1; 16]).unwrap(),
                Selector::Name("g2")
            )
            .is_err()
    );
}

#[test]
fn attested_channel_never_grants_account_privilege_and_update_requires_reauthorization() {
    use novaos_identity::{
        administration::{AdministrativeAction as Act, Scope},
        policy::{Image, InstallChange},
    };
    for sector in [512, 4096] {
        let mut f = Fixture::new(sector);
        let (admin_image, alice_image) = {
            let previous = f.authority.current().unwrap();
            let mut r = Registry::open(&mut f.fs, &mut f.authority, DOMAIN).unwrap();
            let h = r
                .provision_install(previous, [10; 16], f.admin, InstallChange::New([10; 32]))
                .unwrap();
            let admin = Image {
                installation: policy::installed(&h),
                digest: [10; 32],
            };
            let h = r
                .provision_install(h, [11; 16], f.alice, InstallChange::New([11; 32]))
                .unwrap();
            let alice = Image {
                installation: policy::installed(&h),
                digest: [11; 32],
            };
            let h = r
                .provision_administrative_image(
                    h,
                    [12; 16],
                    f.admin,
                    admin,
                    Some(Scope::parse(15).unwrap()),
                )
                .unwrap();
            r.provision_administrative_image(
                h,
                [13; 16],
                f.alice,
                alice,
                Some(Scope::parse(15).unwrap()),
            )
            .unwrap();
            (admin, alice)
        };
        let state = f.view();
        let a = state.resolve_verified_factor([1; 16]).unwrap();
        let b = state.resolve_verified_factor([2; 16]).unwrap();
        let all = Scope::parse(15).unwrap();
        assert!(
            state
                .authorize_administrative(a, admin_image, all, Act::Users)
                .is_ok()
        );
        assert!(
            state
                .authorize_administrative(b, alice_image, all, Act::SelfRead)
                .is_ok()
        );
        assert!(
            state
                .authorize_administrative(b, alice_image, all, Act::Users)
                .is_err()
        );
        assert!(
            state
                .authorize_administrative(b, admin_image, all, Act::SelfRead)
                .is_err()
        );
        assert!(
            state
                .authorize_administrative(a, admin_image, Scope::parse(1).unwrap(), Act::Users)
                .is_err()
        );
        let changed = Image {
            digest: [12; 32],
            ..admin_image
        };
        assert!(state.administrative_scope(a, changed).is_err());
        {
            let previous = f.authority.current().unwrap();
            let mut r = Registry::open(&mut f.fs, &mut f.authority, DOMAIN).unwrap();
            r.provision_install(
                previous,
                [14; 16],
                f.admin,
                InstallChange::Update(admin_image.installation, changed.digest),
            )
            .unwrap();
        }
        let state = f.view();
        let a = state.resolve_verified_factor([1; 16]).unwrap();
        assert!(state.administrative_scope(a, admin_image).is_err());
        assert!(state.administrative_scope(a, changed).is_err());
        {
            let previous = f.authority.current().unwrap();
            let mut r = Registry::open(&mut f.fs, &mut f.authority, DOMAIN).unwrap();
            let h = r
                .provision_administrative_image(previous, [15; 16], f.admin, changed, Some(all))
                .unwrap();
            r.provision_administrative_image(h, [16; 16], f.admin, changed, None)
                .unwrap();
        }
        let state = f.view();
        let a = state.resolve_verified_factor([1; 16]).unwrap();
        assert!(
            state
                .authorize_administrative(a, changed, all, Act::Users)
                .is_err()
        );
        for bits in [0, 16, 255] {
            assert!(Scope::parse(bits).is_err());
        }
    }
}
