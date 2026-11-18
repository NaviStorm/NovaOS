#![cfg(feature = "distribution-lab")]
use ed25519_dalek::{Signer, SigningKey};
use novafs::{Engine, Ids, Mode, device::SimDevice};
use novaos_identity::{distribution::*, lab::Ledger, *};
use sha2::{Digest, Sha256};

// Ces vecteurs publics sont limités au binaire de test hôte.
fn fixture(key: &SigningKey, m: Machine, version: u32, entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut manifest = Vec::from(MAGIC.as_slice());
    manifest.extend([7; 16]);
    manifest.extend(version.to_le_bytes());
    manifest.extend((m as u16).to_le_bytes());
    manifest.extend((entries.len() as u16).to_le_bytes());
    for (name, image) in entries {
        manifest.push(name.len() as u8);
        manifest.extend(name.as_bytes());
        manifest.extend((image.len() as u32).to_le_bytes());
        manifest.extend(Sha256::digest(image));
    }
    let mut bytes = Vec::from((manifest.len() as u32).to_le_bytes());
    bytes.extend(&manifest);
    bytes.extend(key.sign(&manifest).to_bytes());
    for (_, image) in entries {
        bytes.extend(*image);
    }
    bytes
}
fn elf(machine: Machine, value: u8) -> Vec<u8> {
    let mut d = vec![0; 128];
    d[..7].copy_from_slice(b"\x7fELF\x02\x01\x01");
    d[16..18].copy_from_slice(&2u16.to_le_bytes());
    d[18..20].copy_from_slice(&(machine as u16).to_le_bytes());
    d[20..24].copy_from_slice(&1u32.to_le_bytes());
    d[52..54].copy_from_slice(&64u16.to_le_bytes());
    d[127] = value;
    d
}

#[test]
fn paquet_entier_atteste_avant_publication() {
    let key = SigningKey::from_bytes(&[3; 32]);
    let other = SigningKey::from_bytes(&[8; 32]);
    for m in [Machine::Aarch64, Machine::X86_64] {
        let foreign = if m == Machine::X86_64 {
            Machine::Aarch64
        } else {
            Machine::X86_64
        };
        let a = elf(m, 1);
        let b = elf(m, 2);
        let valid = fixture(&key, m, 1, &[("alpha", &a), ("beta", &b)]);
        let v = verify(&key.verifying_key(), m, &valid).unwrap();
        assert_eq!(v.executables()[1].image, b);
        assert_eq!(
            verify(&other.verifying_key(), m, &valid).err(),
            Some(Error::Signature)
        );
        assert_eq!(
            verify(&key.verifying_key(), foreign, &valid).err(),
            Some(Error::Architecture)
        );
        let mut corrupt = valid.clone();
        *corrupt.last_mut().unwrap() ^= 1;
        assert_eq!(
            verify(&key.verifying_key(), m, &corrupt).err(),
            Some(Error::Digest)
        );
        for packet in [
            fixture(&key, m, 1, &[("dup", &a), ("dup", &b)]),
            fixture(&key, m, 1, &[("../cat", &a)]),
            fixture(&key, m, 1, &[("x", &elf(foreign, 3))]),
        ] {
            assert!(verify(&key.verifying_key(), m, &packet).is_err());
        }
        for n in 0..valid.len() {
            assert!(verify(&key.verifying_key(), m, &valid[..n]).is_err());
        }
    }
}

#[test]
fn installation_retrait_reinstallation_et_relecture_p9() {
    let key = SigningKey::from_bytes(&[3; 32]).verifying_key();
    let m = Machine::Aarch64;
    let a = elf(m, 1);
    let b = elf(m, 2);
    let sign = SigningKey::from_bytes(&[3; 32]);
    let v1 = fixture(&sign, m, 1, &[("alpha", &a), ("beta", &b)]);
    let v2 = fixture(&sign, m, 2, &[("alpha", &b)]);
    for sector in [512, 4096] {
        let ids = Ids {
            fs: [9; 16],
            lv: [10; 16],
        };
        let domain = [8; 16];
        let mut auth = Ledger::new(SimDevice::new(32, sector).unwrap());
        let mut disk = provision(
            SimDevice::new(512, sector).unwrap(),
            ids,
            domain,
            [11; 16],
            &mut auth,
        )
        .unwrap();
        let permit = Enrollment::from_provisioner(&mut auth).unwrap();
        let mut reg = Registry::open(&mut disk, &mut auth, domain).unwrap();
        let holder = created_user(
            &reg.enroll(&permit, [1; 16], "alice", [4; 16], "users")
                .unwrap(),
        );
        let grant = reg.grant_attested(holder).unwrap();
        let mut corrupt = v1.clone();
        *corrupt.last_mut().unwrap() ^= 1;
        assert!(
            reg.publish_package(grant, [2; 16], Action::Install, &key, m, &corrupt)
                .is_err()
        );
        assert!(reg.load_package(grant, &key, m, "alpha").is_err());
        assert!(reg.outcome([2; 16]).unwrap().is_none());
        let h = reg
            .publish_package(grant, [2; 16], Action::Install, &key, m, &v1)
            .unwrap();
        assert_eq!(reg.outcome([2; 16]).unwrap(), Some(h));
        let grant = reg.grant_attested(holder).unwrap();
        use novaos_identity::policy::{Image, Origin, READ};
        let alpha = reg.load_package(grant, &key, m, "alpha").unwrap();
        let beta = reg.load_package(grant, &key, m, "beta").unwrap();
        assert_ne!(alpha.application, beta.application);
        assert_eq!(alpha.application.unwrap() >> 63, 1);
        let original = Image {
            installation: alpha.principal.unwrap(),
            digest: alpha.digest,
        };
        let sibling = Image {
            installation: beta.principal.unwrap(),
            digest: beta.digest,
        };
        assert_ne!(original.installation, sibling.installation);
        reg.bind_folder(
            reg.outcome([2; 16]).unwrap().unwrap(),
            holder,
            10,
            b"acl du dossier temoin",
            [10; 16],
        )
        .unwrap();
        let grant = reg.grant_attested(holder).unwrap();
        reg.set_permission(
            grant,
            holder,
            10,
            original.installation,
            Origin::Direct,
            READ,
            [11; 16],
        )
        .unwrap();
        assert_eq!(
            reg.permission(holder, original, holder, 10, Origin::Direct)
                .unwrap()
                .rights,
            READ
        );
        assert!(
            reg.permission(holder, sibling, holder, 10, Origin::Direct)
                .is_err()
        );
        let grant = reg.grant_attested(holder).unwrap();
        reg.set_permission(
            grant,
            holder,
            10,
            original.installation,
            Origin::Direct,
            0,
            [12; 16],
        )
        .unwrap();
        let grant = reg.grant_attested(holder).unwrap();
        let old = reg
            .load_package(grant, &key, m, "beta")
            .unwrap()
            .installation;
        assert_eq!(reg.load_package(grant, &key, m, "beta").unwrap().bytes, b);
        let mut regdisk = Engine::mount_current(disk.into_device(), ids, Mode::ReadWrite).unwrap();
        let mut reg = Registry::open(&mut regdisk, &mut auth, domain).unwrap();
        assert_eq!(reg.load_package(grant, &key, m, "alpha").unwrap().bytes, a);
        reg.publish_package(grant, [3; 16], Action::Update(old), &key, m, &v2)
            .unwrap();
        let grant = reg.grant_attested(holder).unwrap();
        assert!(reg.load_package(grant, &key, m, "beta").is_err());
        let updated = reg.load_package(grant, &key, m, "alpha").unwrap();
        assert_eq!(updated.principal, Some(original.installation));
        assert_eq!(updated.application, alpha.application);
        let current = Image {
            installation: updated.principal.unwrap(),
            digest: updated.digest,
        };
        assert!(
            reg.permission(holder, current, holder, 10, Origin::Direct)
                .is_err()
        );
        assert!(reg.verify_image(original).is_err());
        assert!(reg.verify_image(sibling).is_err());
        assert_eq!(
            reg.load_package(grant, &key, m, "alpha")
                .unwrap()
                .installation,
            old
        );
        assert!(
            reg.publish_package(grant, [4; 16], Action::Update(old), &key, m, &v1)
                .is_err()
        );
        reg.withdraw_package(grant, [4; 16], old).unwrap();
        let grant = reg.grant_attested(holder).unwrap();
        assert!(reg.load_package(grant, &key, m, "alpha").is_err());
        reg.publish_package(grant, [5; 16], Action::Install, &key, m, &v1)
            .unwrap();
        let grant = reg.grant_attested(holder).unwrap();
        let new = reg.load_package(grant, &key, m, "alpha").unwrap();
        assert_ne!(new.principal, Some(original.installation));
        assert_ne!(new.application, alpha.application);
        assert!(
            reg.permission(
                holder,
                Image {
                    installation: new.principal.unwrap(),
                    digest: new.digest
                },
                holder,
                10,
                Origin::Direct
            )
            .is_err()
        );
        assert!(reg.verify_image(original).is_err());
        assert_ne!(
            reg.load_package(grant, &key, m, "alpha")
                .unwrap()
                .installation,
            old
        );
    }
}

struct Interrupted<'a, A> {
    authority: &'a mut A,
    after_decision: bool,
    request: Id,
}

impl<A: Authority> Authority for Interrupted<'_, A> {
    fn current(&self) -> novaos_identity::Result<Head> {
        self.authority.current()
    }
    fn outcome(&self, request: Id) -> novaos_identity::Result<Option<Head>> {
        self.authority.outcome(request)
    }
    fn decide(&mut self, previous: Head, next: Head) -> novaos_identity::Result<()> {
        if next.request != self.request {
            return self.authority.decide(previous, next);
        }
        if self.after_decision {
            self.authority.decide(previous, next)?;
            return Err(Fault::Uncertain);
        }
        Err(Fault::Unavailable)
    }
}

#[test]
fn installation_interrompue_reprise_par_recu_sans_rejeu() {
    let key = SigningKey::from_bytes(&[33; 32]);
    let request = [77; 16];
    let domain = [8; 16];
    let ids = Ids {
        fs: [9; 16],
        lv: [10; 16],
    };
    for m in [Machine::Aarch64, Machine::X86_64] {
        let a = elf(m, 1);
        let b = elf(m, 2);
        let v1 = fixture(&key, m, 1, &[("alpha", &a)]);
        let v2 = fixture(&key, m, 2, &[("alpha", &b), ("beta", &a)]);
        for sector in [512, 4096] {
            for after_decision in [false, true] {
                let mut auth = Ledger::new(SimDevice::new(32, sector).unwrap());
                let mut disk = provision(
                    SimDevice::new(512, sector).unwrap(),
                    ids,
                    domain,
                    [11; 16],
                    &mut auth,
                )
                .unwrap();
                let permit = Enrollment::from_provisioner(&mut auth).unwrap();
                let mut reg = Registry::open(&mut disk, &mut auth, domain).unwrap();
                let owner = created_user(
                    &reg.enroll(&permit, [1; 16], "alice", [4; 16], "base")
                        .unwrap(),
                );
                let grant = reg.grant_attested(owner).unwrap();
                reg.publish_package(
                    grant,
                    [2; 16],
                    Action::Install,
                    &key.verifying_key(),
                    m,
                    &v1,
                )
                .unwrap();
                let grant = reg.grant_attested(owner).unwrap();
                let id = reg
                    .load_package(grant, &key.verifying_key(), m, "alpha")
                    .unwrap()
                    .installation;
                let mut fault = Interrupted {
                    authority: &mut auth,
                    after_decision,
                    request,
                };
                let mut reg = Registry::open(&mut disk, &mut fault, domain).unwrap();
                let outcome = reg.publish_package(
                    grant,
                    request,
                    Action::Update(id),
                    &key.verifying_key(),
                    m,
                    &v2,
                );
                assert!(matches!(
                    (after_decision, outcome),
                    (true, Err(Fault::Uncertain)) | (false, Err(Fault::Unavailable))
                ));
                let mut disk =
                    Engine::mount_current(disk.into_device(), ids, Mode::ReadWrite).unwrap();
                let mut auth = Ledger::new(auth.into_device());
                let reg = Registry::open(&mut disk, &mut auth, domain).unwrap();
                let proof = reg.outcome(request).unwrap();
                let current = reg.grant_attested(owner).unwrap();
                assert_eq!(proof.is_some(), after_decision);
                let alpha = reg
                    .load_package(current, &key.verifying_key(), m, "alpha")
                    .unwrap();
                let beta = reg.load_package(current, &key.verifying_key(), m, "beta");
                assert_eq!(alpha.bytes, if after_decision { &b } else { &a }[..]);
                assert_eq!(alpha.installation, id);
                if after_decision {
                    assert_eq!(beta.unwrap().bytes, a);
                } else {
                    assert!(matches!(beta, Err(Fault::Denied)));
                }
            }
        }
    }
}

#[cfg(feature="recuperation-lab")]
#[test]
fn resolution_isolee_lie_titulaire_installation_tete_et_architecture() {
    use novaos_identity::{recuperation::Triplet,recuperation_view::View};
    fn view(t:Triplet)->View {
        View::read(*t.head(),t.parts().each_ref().map(|p|p.len() as u32),|i,o,b| {
            let p=t.parts()[i];let o=o as usize;let n=b.len().min(p.len()-o);b[..n].copy_from_slice(&p[o..o+n]);Ok(n)
        }).unwrap()
    }
    let signing=SigningKey::from_bytes(&[3;32]);let key=signing.verifying_key();
    for sector in [512,4096] {for machine in [Machine::Aarch64,Machine::X86_64] {
        let mut authority=Ledger::new(SimDevice::new(32,sector).unwrap());let domain=[7;16];
        let mut disk=provision(SimDevice::new(512,sector).unwrap(),Ids{fs:[8;16],lv:[9;16]},domain,[4;16],&mut authority).unwrap();
        let permit=Enrollment::from_provisioner(&mut authority).unwrap();
        let mut r=Registry::open(&mut disk,&mut authority,domain).unwrap();
        let enrolled=r.enroll(&permit,[1;16],"alice",[2;16],"users").unwrap();let owner=created_user(&enrolled);
        let grant=r.grant_attested(owner).unwrap();
        r.mutate(grant,[3;16],Change::Create{name:"bob",credential:[4;16],group:created_group(&enrolled)}).unwrap();
        let image=elf(machine,17);let package=fixture(&signing,machine,1,&[("cut",&image)]);
        let grant=r.grant_attested(owner).unwrap();r.publish_package(grant,[5;16],Action::Install,&key,machine,&package).unwrap();
        let grant=r.grant_attested(owner).unwrap();let loaded=r.load_package(grant,&key,machine,"cut").unwrap();drop(r);
        let v=view(Triplet::capture(&disk,&authority,domain).unwrap());let a=v.resolve_verified_factor([2;16]).unwrap();
        let b=v.resolve_verified_factor([4;16]).unwrap();let d=v.executable(a,&key,machine,"cut").unwrap();
        assert_eq!(&d[8..40],&v.head().digest);assert_eq!(&d[40..56],&owner);
        assert_eq!(&d[56..72],&loaded.principal.unwrap());assert_eq!(&d[152..184],&Sha256::digest(&image)[..]);
        assert_eq!(u64::from_le_bytes(d[120..128].try_into().unwrap()),loaded.object);
        assert!(v.executable(b,&key,machine,"cut").is_err());
        assert!(v.executable(a,&key,machine,"../cut").is_err());
        assert!(v.executable(a,&SigningKey::from_bytes(&[8;32]).verifying_key(),machine,"cut").is_err());
        let foreign=if machine==Machine::Aarch64 {Machine::X86_64}else{Machine::Aarch64};
        assert!(v.executable(a,&key,foreign,"cut").is_err());
        let mut r=Registry::open(&mut disk,&mut authority,domain).unwrap();let g=r.grant_attested(owner).unwrap();
        r.withdraw_package(g,[6;16],loaded.installation).unwrap();drop(r);
        let revoked=view(Triplet::capture(&disk,&authority,domain).unwrap());
        assert!(revoked.executable(a,&key,machine,"cut").is_err());
        let a=revoked.resolve_verified_factor([2;16]).unwrap();assert!(revoked.executable(a,&key,machine,"cut").is_err());
        // L'ancienne vue reste une photographie ; sa fraîcheur n'est jamais implicite.
        assert_ne!(v.head(),revoked.head());
    }}
}
