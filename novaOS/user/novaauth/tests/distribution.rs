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

#[cfg(feature = "administration-lab")]
#[test]
fn provisionnement_explicite_bob_sans_promotion_ni_heritage() {
    use novaos_identity::{recuperation::Triplet, recuperation_view::View};
    let signer = SigningKey::from_bytes(&[3; 32]);
    let key = signer.verifying_key();
    for sector in [512, 4096] { for machine in [Machine::Aarch64, Machine::X86_64] {
        let mut authority = Ledger::new(SimDevice::new(32, sector).unwrap());
        let domain = [7; 16];
        let mut disk = provision(SimDevice::new(512, sector).unwrap(), Ids { fs:[8;16], lv:[9;16] }, domain, [4;16], &mut authority).unwrap();
        let permit = Enrollment::from_provisioner(&mut authority).unwrap();
        let mut r = Registry::open(&mut disk, &mut authority, domain).unwrap();
        let enrolled = r.enroll(&permit,[1;16],"alice",[2;16],"users").unwrap();
        let alice = created_user(&enrolled);
        let grant = r.grant_attested(alice).unwrap();
        let bob = created_user(&r.mutate(grant,[3;16],Change::Create{name:"bob",credential:[4;16],group:created_group(&enrolled)}).unwrap());
        let bytes = elf(machine,17);
        let only_alice = fixture(&signer,machine,1,&[("alice-seule",&bytes)]);
        let for_bob = fixture(&signer,machine,1,&[("bob-seul",&bytes)]);
        let grant = r.grant_attested(alice).unwrap();
        r.publish_package(grant,[5;16],Action::Install,&key,machine,&only_alice).unwrap();
        let ga = r.grant_attested(alice).unwrap();
        let gb = r.grant_attested(bob).unwrap();
        let before = ga.generation_digest();
        // Un nom ou les octets d'Alice ne suffisent pas ; Bob n'est pas provisionneur.
        assert!(matches!(r.load_package(gb,&key,machine,"alice-seule"),Err(Fault::Denied)));
        assert!(matches!(r.publish_package(gb,[6;16],Action::Install,&key,machine,&for_bob),Err(Fault::Denied)));
        for target in [alice,bob] {
            assert!(matches!(r.provision_package_for_user(gb,target,[6;16],&key,machine,&for_bob),Err(Fault::Denied)));
        }
        assert!(matches!(r.provision_package_for_user(ga,[99;16],[6;16],&key,machine,&for_bob),Err(Fault::Denied)));
        let foreign = SigningKey::from_bytes(&[8;32]);
        let unauthorized = fixture(&foreign,machine,1,&[("bob-seul",&bytes)]);
        assert!(r.provision_package_for_user(ga,bob,[6;16],&key,machine,&unauthorized).is_err());
        let mut altered = for_bob.clone();
        let sig = 4 + u32::from_le_bytes(altered[..4].try_into().unwrap()) as usize;
        altered[sig] ^= 1;
        assert!(r.provision_package_for_user(ga,bob,[6;16],&key,machine,&altered).is_err());
        assert_eq!(r.grant_attested(alice).unwrap().generation_digest(),before);
        assert!(r.outcome([6;16]).unwrap().is_none());
        r.provision_package_for_user(ga,bob,[6;16],&key,machine,&for_bob).unwrap();
        // Une répétition avec un destinataire différent ne remplace pas la décision.
        let ga = r.grant_attested(alice).unwrap();
        let gb = r.grant_attested(bob).unwrap();
        assert!(matches!(r.provision_package_for_user(ga,alice,[6;16],&key,machine,&for_bob),Err(Fault::Conflict)));
        assert_eq!(r.load_package(gb,&key,machine,"bob-seul").unwrap().bytes,bytes);
        assert!(matches!(r.load_package(ga,&key,machine,"bob-seul"),Err(Fault::Denied)));
        assert!(matches!(r.load_package(gb,&key,machine,"alice-seule"),Err(Fault::Denied)));
        assert!(matches!(r.publish_package(gb,[7;16],Action::Install,&key,machine,&for_bob),Err(Fault::Denied)));
        drop(r);
        let triplet = Triplet::capture(&disk,&authority,domain).unwrap();
        let view = View::read(*triplet.head(),triplet.parts().each_ref().map(|p|p.len() as u32),|i,o,b| {
            let p=triplet.parts()[i];let o=o as usize;let n=b.len().min(p.len()-o);b[..n].copy_from_slice(&p[o..o+n]);Ok(n)
        }).unwrap();
        let a = view.resolve_verified_factor([2;16]).unwrap();
        let b = view.resolve_verified_factor([4;16]).unwrap();
        let ai=view.user_info(a,None).unwrap(); let bi=view.user_info(b,None).unwrap();
        assert!(ai.administrator); assert!(!bi.administrator);
        assert_eq!(ai.groups,bi.groups); // aucun nouveau groupe attribué au destinataire
        let ar=view.executable(a,&key,machine,"alice-seule").unwrap();
        let br=view.executable(b,&key,machine,"bob-seul").unwrap();
        assert_eq!(&ar[40..56],&alice);assert_eq!(&br[40..56],&bob);
        assert_ne!(&ar[56..72],&br[56..72]);
        assert!(matches!(view.executable(b,&key,machine,"alice-seule"),Err(Fault::Denied)));
        assert!(matches!(view.executable(a,&key,machine,"bob-seul"),Err(Fault::Denied)));
    }}
}

#[cfg(feature="administration-lab")]
#[test]
fn selective_exec_revalidates_own_state_and_actual_required_permissions() {
 use novaos_identity::{recuperation::Triplet,recuperation_view::View,policy::*,execution_selective::RequiredPermission};
 fn view(t:&Triplet)->View{
  let parts=t.parts();View::read(*t.head(),parts.map(|p|p.len() as u32),|i,o,b|{
   let o=o as usize;let n=b.len().min(parts[i].len()-o);b[..n].copy_from_slice(&parts[i][o..o+n]);Ok(n)
  }).unwrap()
 }
 let sign=SigningKey::from_bytes(&[3;32]);let key=sign.verifying_key();
 for sector in [512,4096] {for machine in [Machine::Aarch64,Machine::X86_64] {
  let mut authority=Ledger::new(SimDevice::new(32,sector).unwrap());let domain=[8;16];
  let mut fs=provision(SimDevice::new(1024,sector).unwrap(),Ids{fs:[9;16],lv:[10;16]},domain,[11;16],&mut authority).unwrap();
  let enrol=Enrollment::from_provisioner(&mut authority).unwrap();
  let mut r=Registry::open(&mut fs,&mut authority,domain).unwrap();
  let holder=created_user(&r.enroll(&enrol,[1;16],"alice",[4;16],"admins").unwrap());
  let mut groups=Vec::new();
  for (op,name) in [(2,"g"),(3,"h")] {let head=r.mutate(r.grant_attested(holder).unwrap(),[op;16],Change::AddGroup(name)).unwrap();groups.push(created_group(&head));}
  for (op,group) in [(4,groups[0]),(5,groups[1])] {r.mutate(r.grant_attested(holder).unwrap(),[op;16],Change::AddMember(holder,group)).unwrap();}
  let packet=fixture(&sign,machine,1,&[("alpha",&elf(machine,1))]);
  let installed_head=r.publish_package(r.grant_attested(holder).unwrap(),[6;16],Action::Install,&key,machine,&packet).unwrap();
  let loaded=r.load_package(r.grant_attested(holder).unwrap(),&key,machine,"alpha").unwrap();
  let image=Image{installation:loaded.principal.unwrap(),digest:loaded.digest};
  r.bind_folder(installed_head,holder,17,b"scope reel requis par le producteur",[7;16]).unwrap();
  for (op,origin) in [(8,Origin::Direct),(9,Origin::Group(groups[0])),(10,Origin::Group(groups[1]))] {
   r.set_permission(r.grant_attested(holder).unwrap(),holder,17,image.installation,origin,READ,[op;16]).unwrap();
  }
  drop(r);let before=view(&Triplet::capture(&fs,&authority,domain).unwrap());let grant=before.resolve_verified_factor([4;16]).unwrap();
  let epoch=before.authentication_epoch(grant).unwrap();let own=before.execution_identity(grant,image,&key,machine).unwrap();
  let needs:[RequiredPermission;3]=[Origin::Direct,Origin::Group(groups[0]),Origin::Group(groups[1])].map(|origin|{
   let p=before.permission(grant,image,holder,17,origin).unwrap();RequiredPermission{owner:holder,object:17,origin,rights:READ,
    membership_revision:p.membership_revision,rule_revision:p.rule_revision,image_revision:p.image_revision,
    seal:before.folder(grant,17).unwrap()}
  });
  for need in needs {before.revalidate_execution_dependencies(grant,epoch,image,own,&key,machine,&[need]).unwrap();}
  let wrong_seal=RequiredPermission{seal:[42;32],..needs[2]};
  assert!(matches!(before.revalidate_execution_dependencies(grant,epoch,image,own,&key,machine,&[wrong_seal]),Err(Fault::Denied)));
  let unbound=view(&before.revoke_plan(grant,17,[11;16]).unwrap());
  let current=unbound.resolve_verified_factor([4;16]).unwrap();
  // Le retrait du sceau laisse les règles enregistrées. Celles-ci ne
  // suffisent plus à revalider la portée du ticket.
  assert!(unbound.permission(current,image,holder,17,needs[2].origin).is_ok());
  assert!(matches!(unbound.revalidate_execution_dependencies(current,epoch,image,own,&key,machine,&[needs[2]]),Err(Fault::Denied)));
  let removal=before.prepare_membership_removal(grant,[12;16],holder,groups[0]).unwrap();let after=view(&removal.triplet);
  let fresh=after.resolve_verified_factor([4;16]).unwrap();assert_ne!(before.head(),after.head());
  after.revalidate_execution_dependencies(fresh,epoch,image,own,&key,machine,&[]).unwrap();
  for set in [vec![needs[0]],vec![needs[2]],vec![needs[0],needs[2]]] {after.revalidate_execution_dependencies(fresh,epoch,image,own,&key,machine,&set).unwrap();}
  for set in [vec![needs[1]],vec![needs[0],needs[1]],vec![needs[1],needs[2]]] {assert!(after.revalidate_execution_dependencies(fresh,epoch,image,own,&key,machine,&set).is_err());}
  assert!(after.revalidate_execution_dependencies(grant,epoch,image,own,&key,machine,&[]).is_err());
  assert!(after.revalidate_execution_dependencies(fresh,epoch+1,image,own,&key,machine,&[]).is_err());
  for changed in [ExecutionIdentity{version:own.version+1,..own},ExecutionIdentity{revision:own.revision+1,..own},
   ExecutionIdentity{administration:1,..own},ExecutionIdentity{offset:own.offset+1,..own},ExecutionIdentity{image_revision:own.image_revision+1,..own}] {
   assert!(after.revalidate_execution_dependencies(fresh,epoch,image,changed,&key,machine,&[]).is_err());
  }
  let added=after.administration_plan(fresh,[13;16],novaos_identity::administration::Operation::AddMember{subject:holder,group:groups[0]},None).unwrap();
  let reopened=view(&added);let g=reopened.resolve_verified_factor([4;16]).unwrap();
  assert!(reopened.revalidate_execution_dependencies(g,epoch,image,own,&key,machine,&[needs[1]]).is_err());
  reopened.revalidate_execution_dependencies(g,epoch,image,own,&key,machine,&[needs[0],needs[2]]).unwrap();
 }}
}
