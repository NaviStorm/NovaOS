#![cfg(feature = "recuperation-lab")]
use novafs::{Engine, Ids, Mode, device::SimDevice};
use novaos_identity::{lab::Ledger, recuperation::Triplet, *};

#[test]
fn restaurer_uniquement_le_triplet_de_la_tete_durable() {
    for sector in [512, 4096] {
        let domain = [7; 16];
        let ids = Ids {
            fs: [8; 16],
            lv: [9; 16],
        };
        let disk = SimDevice::new(512, sector).unwrap();
        let media = disk.control();
        let mut a = Ledger::new(SimDevice::new(12, sector).unwrap());
        let mut fs = provision(disk, ids, domain, [4; 16], &mut a).unwrap();
        let e = Enrollment::from_provisioner(&mut a).unwrap();
        let root = Registry::open(&mut fs, &mut a, domain)
            .unwrap()
            .enroll(&e, [1; 16], "admin", [2; 16], "base")
            .unwrap();
        let mut reg = Registry::open(&mut fs, &mut a, domain).unwrap();
        let admin = reg.resolve_verified_factor([2; 16]).unwrap();
        let alice = reg
            .mutate(
                admin,
                [3; 16],
                Change::Create {
                    name: "alice",
                    credential: [4; 16],
                    group: created_group(&root),
                },
            )
            .unwrap();
        let before = media.snapshot();
        let obsolete = Triplet::capture(&fs, &a, domain).unwrap();
        let mut reg = Registry::open(&mut fs, &mut a, domain).unwrap();
        let admin = reg.resolve_verified_factor([2; 16]).unwrap();
        reg.mutate(admin, [5; 16], Change::Disable(created_user(&alice)))
            .unwrap();
        let head = a.current().unwrap();
        let current = Triplet::capture(&fs, &a, domain).unwrap().encode();
        assert!(Triplet::verify(head, &obsolete.encode()).is_err());
        assert!(Triplet::verify(head, &current[..current.len() - 1]).is_err());
        let mut tampered = current.clone();
        *tampered.last_mut().unwrap() ^= 1;
        assert!(Triplet::verify(head, &tampered).is_err());
        let valid = Triplet::verify(head, &current).unwrap();
        // Le vérificateur en flux utilise le digest natif réellement produit
        // par Registry, pas un digest de chiffré choisi par le banc.
        use novaos_identity::recuperation::{decode_head,SnapshotDigest};
        assert_eq!(decode_head(&current[8..152]).unwrap(),head);
        let mut at=152;
        let parts: [Vec<u8>;3]=core::array::from_fn(|_| {
            let n=u32::from_be_bytes(current[at..at+4].try_into().unwrap()) as usize;
            at+=4;let value=current[at..at+n].to_vec();at+=n;value
        });
        let lengths=parts.each_ref().map(|p|p.len() as u32);
        let view=novaos_identity::recuperation_view::View::read(head,lengths,|t,o,b| {
            let o=o as usize;let n=b.len().min(parts[t].len()-o);b[..n].copy_from_slice(&parts[t][o..o+n]);Ok(n)
        }).unwrap();
        assert_eq!(view.head(),head);
        assert!(view.resolve_verified_factor([4;16]).is_err());
        let admin=view.resolve_verified_factor([2;16]).unwrap();assert_eq!(view.groups(admin).unwrap(),1);
        assert!(novaos_identity::recuperation_view::View::read(head,lengths,|_,_,_|Ok(0)).is_err());
        let mut bad=head;bad.digest[0]^=1;
        assert!(novaos_identity::recuperation_view::View::read(bad,lengths,|t,o,b| {
            let o=o as usize;let n=b.len().min(parts[t].len()-o);b[..n].copy_from_slice(&parts[t][o..o+n]);Ok(n)
        }).is_err());

        let mut stream=SnapshotDigest::new(&current[8..152],lengths).unwrap();
        for part in &parts {for chunk in part.chunks(37) {stream.push(chunk).unwrap();}}
        stream.finish().unwrap();
        let mut wrong=current[8..152].to_vec();wrong[112]^=1;
        let mut stream=SnapshotDigest::new(&wrong,lengths).unwrap();
        for part in &parts {stream.push(part).unwrap();}
        assert!(stream.finish().is_err());
        let mut incomplete=SnapshotDigest::new(&current[8..152],lengths).unwrap();
        incomplete.push(&parts[0]).unwrap();
        assert!(incomplete.finish().is_err());
        // Effecteur privé : trois objets imposés, fragments en ordre et
        // relecture après scellement. La décision reste dans l'autorité.
        use novaos_identity::recuperation_store::{Store,Phase,FRAGMENT};
        let mut target=Engine::mount_current(before.device(),ids,Mode::ReadWrite).unwrap();
        let mut store=Store::begin(&mut target,&current[8..152],[60;16],lengths,domain).unwrap();
        assert_eq!(store.phase(),Phase::Preparing);
        let mut chunk=[0;FRAGMENT];
        assert!(store.read(&target,[60;16],0,0,&mut chunk).is_err());
        assert!(store.append(&mut target,[61;16],0,0,&parts[0][..1]).is_err());
        assert!(store.append(&mut target,[60;16],1,0,&parts[0][..1]).is_err());
        assert!(store.seal(&target,[60;16]).is_err());
        for (table,part) in parts.iter().enumerate() {
            for (index,bytes) in part.chunks(FRAGMENT).enumerate() {
                store.append(&mut target,[60;16],table,(index*FRAGMENT) as u32,bytes).unwrap();
            }
            if table<2 {assert!(store.seal(&target,[60;16]).is_err());}
        }
        assert_eq!(store.seal(&target,[60;16]).unwrap(),head);
        assert_eq!(store.phase(),Phase::Sealed);
        assert!(store.append(&mut target,[60;16],2,0,&parts[2][..1]).is_err());
        assert!(store.read(&target,[60;16],3,0,&mut chunk).is_err());
        for (table,part) in parts.iter().enumerate() {
            let mut read=Vec::new();
            while read.len()<part.len() {
                let n=store.read(&target,[60;16],table,read.len() as u32,&mut chunk).unwrap();
                read.extend_from_slice(&chunk[..n]);
            }
            assert_eq!(&read,part);
        }
        let receipt=target.receipt();
        assert_eq!(store.seal(&target,[60;16]).unwrap(),head);
        assert_eq!(target.receipt(),receipt); // réconciliation en lecture seule
        let mut wrong=current[8..152].to_vec();wrong[64..72].copy_from_slice(&1u64.to_be_bytes());
        assert!(Store::begin(&mut target,&wrong,[62;16],lengths,domain).is_err());
        assert_eq!(target.receipt(),receipt);
        let mut target=Engine::mount_current(before.device(),ids,Mode::ReadWrite).unwrap();
        let mut port=novaos_identity::recuperation_port::Port::default();
        let frame=|op:u8,table:usize,offset:usize,bytes:&[u8]| {
            let mut q=[0;128];q[0]=op;q[1]=table as u8;q[8..24].copy_from_slice(&[70;16]);
            q[24..28].copy_from_slice(&(offset as u32).to_le_bytes());
            q[28..30].copy_from_slice(&(bytes.len() as u16).to_le_bytes());
            q[32..32+bytes.len()].copy_from_slice(bytes);q
        };
        let mut description=current[8..152].to_vec();
        for n in lengths {description.extend(n.to_be_bytes());}
        for (i,b) in description.chunks(FRAGMENT).enumerate() {
            port.execute(&mut target,domain,&frame(1,0,i*FRAGMENT,b)).unwrap();
        }
        let mut malformed=frame(2,0,0,&parts[0][..1]);malformed[3]=1;
        assert!(port.execute(&mut target,domain,&malformed).is_err());
        for (table,part) in parts.iter().enumerate() {
            for (i,b) in part.chunks(FRAGMENT).enumerate() {
                let q=frame(2,table,i*FRAGMENT,b);
                port.execute(&mut target,domain,&q).unwrap();
                assert!(port.execute(&mut target,domain,&q).is_err());
            }
        }
        assert_eq!(port.execute(&mut target,domain,&frame(3,0,0,&[])).unwrap()[8],3);
        let q=frame(4,0,0,&[0;FRAGMENT]);
        assert_eq!(&port.execute(&mut target,domain,&q).unwrap()[32..128],&parts[0][..FRAGMENT]);
        assert!(port.execute(&mut target,domain,&frame(4,3,0,&[0;1])).is_err());
        let device=before.device();let ctl=device.control();
        let mut broken=Engine::mount_current(device,ids,Mode::ReadWrite).unwrap();
        let mut store=Store::begin(&mut broken,&current[8..152],[80;16],lengths,domain).unwrap();
        ctl.trace(None,None);
        ctl.inject(novafs::device::SimFault::Flush {ordinal:2,persist_newest:usize::MAX}).unwrap();
        assert!(store.append(&mut broken,[80;16],0,0,&parts[0][..FRAGMENT]).is_err());
        assert_eq!(store.phase(),Phase::Uncertain);
        let events=ctl.events();
        assert!(store.append(&mut broken,[80;16],0,0,&parts[0][..FRAGMENT]).is_err());
        assert_eq!(ctl.events(),events); // aucune nouvelle E/S après UNCERTAIN
        assert!(store.seal(&broken,[80;16]).is_err());
        let mut restored = Engine::mount_current(before.device(), ids, Mode::ReadWrite).unwrap();
        assert!(Registry::open(&mut restored, &mut a, domain).is_err());
        let receipt = restored.receipt();
        assert!(obsolete.restore(&mut restored, &a, domain).is_err());
        assert_eq!(restored.receipt(), receipt);
        valid.restore(&mut restored, &a, domain).unwrap();
        let receipt = restored.receipt();
        valid.restore(&mut restored, &a, domain).unwrap();
        assert_eq!(restored.receipt(), receipt);
        let reg = Registry::open(&mut restored, &mut a, domain).unwrap();
        assert!(reg.resolve_verified_factor([4; 16]).is_err());
        assert!(reg.resolve_verified_factor([2; 16]).is_ok());
        drop(reg);
        assert_eq!(a.current().unwrap(), head);
        // Interruption d'une préparation du triplet. La décision durable ne
        // change pas et une reprise explicite réécrit la sauvegarde vérifiée.
        for object in head.objects {
            restored.prepare_write_id(object, (head.generation % 2) * 16384, &[0; 60])
                .unwrap().commit().unwrap();
            assert!(Registry::open(&mut restored, &mut a, domain).is_err());
            valid.restore(&mut restored, &a, domain).unwrap();
            let resumed = Registry::open(&mut restored, &mut a, domain).unwrap();
            assert!(resumed.resolve_verified_factor([4;16]).is_err());
            assert!(resumed.resolve_verified_factor([2;16]).is_ok());
            assert_eq!(a.current().unwrap(), head);
        }
    }
}
