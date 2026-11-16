#![cfg(feature = "politiques-lab")]
use novafs::{Ids, device::SimDevice};
use novaos_identity::{lab::Ledger, policy::*, *};

#[test]
fn provenances_et_incarnations_independantes() {
    for sector in [512, 4096] {
        let mut authority = Ledger::new(SimDevice::new(32, sector).unwrap());
        let domain = [9; 16];
        let mut disk = provision(
            SimDevice::new(512, sector).unwrap(),
            Ids {
                fs: [5; 16],
                lv: [6; 16],
            },
            domain,
            [7; 16],
            &mut authority,
        )
        .unwrap();
        let provisioner = Enrollment::from_provisioner(&mut authority).unwrap();
        let mut r = Registry::open(&mut disk, &mut authority, domain).unwrap();
        let first = r
            .enroll(&provisioner, [1; 16], "admin", [21; 16], "base")
            .unwrap();
        let admin = created_user(&first);
        let alice = created_user(
            &r.mutate(
                r.grant_attested(admin).unwrap(),
                [2; 16],
                Change::Create {
                    name: "alice",
                    credential: [22; 16],
                    group: created_group(&first),
                },
            )
            .unwrap(),
        );
        let h = r
            .mutate(
                r.grant_attested(admin).unwrap(),
                [3; 16],
                Change::AddGroup("atelier"),
            )
            .unwrap();
        let group = created_group(&h);
        let h = r
            .provision_install(h, [4; 16], alice, InstallChange::New([42; 32]))
            .unwrap();
        let image = Image {
            installation: installed(&h),
            digest: [42; 32],
        };
        r.bind_folder(h, alice, 17, b"acl", [5; 16]).unwrap();
        let stale = r.grant_attested(alice).unwrap();
        r.set_permission(
            stale,
            alice,
            17,
            image.installation,
            Origin::Direct,
            READ,
            [6; 16],
        )
        .unwrap();
        assert!(matches!(
            r.set_permission(
                stale,
                alice,
                17,
                image.installation,
                Origin::Direct,
                WRITE,
                [7; 16]
            ),
            Err(Fault::Conflict)
        ));
        assert!(r.outcome([7; 16]).unwrap().is_none());
        r.set_permission(
            r.grant_attested(alice).unwrap(),
            alice,
            17,
            image.installation,
            Origin::Group(group),
            WRITE,
            [8; 16],
        )
        .unwrap();
        let direct = r
            .permission(alice, image, alice, 17, Origin::Direct)
            .unwrap();
        r.mutate(
            r.grant_attested(admin).unwrap(),
            [9; 16],
            Change::AddMember(alice, group),
        )
        .unwrap();
        let old = r
            .permission(alice, image, alice, 17, Origin::Group(group))
            .unwrap();
        r.mutate(
            r.grant_attested(admin).unwrap(),
            [10; 16],
            Change::RemoveMember(alice, group),
        )
        .unwrap();
        assert!(
            r.permission(alice, image, alice, 17, Origin::Group(group))
                .is_err()
        );
        r.mutate(
            r.grant_attested(admin).unwrap(),
            [11; 16],
            Change::AddMember(alice, group),
        )
        .unwrap();
        let fresh = r
            .permission(alice, image, alice, 17, Origin::Group(group))
            .unwrap();
        assert_ne!(old.membership_revision, fresh.membership_revision);
        assert_eq!(
            r.permission(alice, image, alice, 17, Origin::Direct)
                .unwrap(),
            direct
        );
        r.mutate(
            r.grant_attested(admin).unwrap(),
            [12; 16],
            Change::Rename(alice, "renommee"),
        )
        .unwrap();
        assert_eq!(
            r.permission(alice, image, alice, 17, Origin::Direct)
                .unwrap(),
            direct
        );
        r.mutate(
            r.grant_attested(admin).unwrap(),
            [13; 16],
            Change::Disable(alice),
        )
        .unwrap();
        assert!(r.verify_image(image).is_err());
        assert!(
            r.permission(alice, image, alice, 17, Origin::Direct)
                .is_err()
        );
        r.mutate(
            r.grant_attested(admin).unwrap(),
            [14; 16],
            Change::Delete(alice),
        )
        .unwrap();
        let next = created_user(
            &r.mutate(
                r.grant_attested(admin).unwrap(),
                [15; 16],
                Change::Create {
                    name: "renommee",
                    credential: [23; 16],
                    group: created_group(&first),
                },
            )
            .unwrap(),
        );
        assert_ne!(next, alice);
        assert!(
            r.permission(next, image, alice, 17, Origin::Direct)
                .is_err()
        );
        assert!(r.verify_image(image).is_err());
    }
}
