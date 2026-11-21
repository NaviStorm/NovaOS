#![cfg(feature="recuperation-lab")]
use novafs::{Ids,device::SimDevice};
use novaos_identity::{*,lab::Ledger,recuperation::Triplet,recuperation_view::{View,seal_digest}};
fn view(triplet:Triplet)->View {
    let data=triplet.encode();let head=*triplet.head();let mut pos=152;
    let parts:[Vec<u8>;3]=core::array::from_fn(|_| {
        let n=u32::from_be_bytes(data[pos..pos+4].try_into().unwrap()) as usize;
        pos+=4;let out=data[pos..pos+n].to_vec();pos+=n;out
    });
    View::read(head,parts.each_ref().map(|p|p.len() as u32),|t,o,b|{
        let o=o as usize;let n=b.len().min(parts[t].len()-o);b[..n].copy_from_slice(&parts[t][o..o+n]);Ok(n)
    }).unwrap()
}

fn frame(op:u8,id:Id,table:usize,offset:usize,data:&[u8])->[u8;128] {
    let mut q=[0;128];q[0]=op;q[1]=table as u8;q[8..24].copy_from_slice(&id);
    q[24..28].copy_from_slice(&(offset as u32).to_le_bytes());
    q[28..30].copy_from_slice(&(data.len() as u16).to_le_bytes());q[32..32+data.len()].copy_from_slice(data);q
}
fn prepare<D:novafs::Device>(p:&mut recuperation_port::Port,fs:&mut novafs::Engine<D>,triplet:&Triplet,id:Id) {
    let mut descriptor=Triplet::head_bytes(triplet.head()).to_vec();
    for part in triplet.parts() {descriptor.extend((part.len() as u32).to_be_bytes());}
    for (i,c) in descriptor.chunks(96).enumerate() {p.execute(fs,[7;16],&frame(1,id,0,i*96,c)).unwrap();}
    for (t,part) in triplet.parts().iter().enumerate() {
        for (i,c) in part.chunks(96).enumerate() {p.execute(fs,[7;16],&frame(2,id,t,i*96,c)).unwrap();}
    }
    p.execute(fs,[7;16],&frame(3,id,0,0,&[])).unwrap();
}

#[test]
fn retrait_dun_dossier_preserve_le_droit_independant_et_isole_bob() {
    for sector in [512,4096] {
        let ids=Ids {fs:[8;16],lv:[9;16]};let domain=[7;16];
        let mut a=Ledger::new(SimDevice::new(12,sector).unwrap());
        let mut fs=provision(SimDevice::new(512,sector).unwrap(),ids,domain,[4;16],&mut a).unwrap();
        let e=Enrollment::from_provisioner(&mut a).unwrap();
        let mut reg=Registry::open(&mut fs,&mut a,domain).unwrap();
        let alice=reg.enroll(&e,[1;16],"alice",[2;16],"base").unwrap();
        let grant=reg.resolve_verified_factor([2;16]).unwrap();
        let bob=reg.mutate(grant,[3;16],Change::Create {name:"bob",credential:[4;16],group:created_group(&alice)}).unwrap();
        reg.bind_folder(bob,created_user(&alice),100,b"ACL lecture P",[5;16]).unwrap();
        drop(reg);
        let head=a.current().unwrap();
        let mut reg=Registry::open(&mut fs,&mut a,domain).unwrap();
        reg.bind_folder(head,created_user(&alice),101,b"ACL lecture L",[6;16]).unwrap();
        let stale=reg.resolve_verified_factor([2;16]).unwrap();
        let bob=reg.resolve_verified_factor([4;16]).unwrap();
        assert!(reg.revoke_folder(bob,100,seal_digest(b"ACL lecture P"),[7;16]).is_err());
        assert!(reg.revoke_folder(stale,100,seal_digest(b"ACL substituee"),[8;16]).is_err());
        drop(reg);
        let old=view(Triplet::capture(&fs,&a,domain).unwrap());
        let owner=old.resolve_verified_factor([2;16]).unwrap();
        assert_eq!(old.folder(owner,100).unwrap(),seal_digest(b"ACL lecture P"));
        assert_eq!(old.folder(owner,101).unwrap(),seal_digest(b"ACL lecture L"));
        let bob=old.resolve_verified_factor([4;16]).unwrap();
        assert!(old.folder(bob,100).is_err());assert!(old.folder(bob,101).is_err());
        let plan=old.revoke_plan(owner,100,[9;16]).unwrap();
        assert!(old.revoke_plan(bob,100,[9;16]).is_err());
        assert!(old.revoke_plan(owner,100,[0;16]).is_err());
        // Préparer ne change pas la vue active.
        assert!(old.folder(owner,100).is_ok());
        let initial=Triplet::capture(&fs,&a,domain).unwrap();
        let mut port=recuperation_port::Port::default();
        prepare(&mut port,&mut fs,&initial,[70;16]);
        let before=fs.receipt();
        assert!(port.execute(&mut fs,domain,&frame(6,[70;16],0,0,&[0;32])).is_err());
        assert_eq!(before,fs.receipt());
        port.execute(&mut fs,domain,&frame(6,[70;16],0,0,&initial.head().digest)).unwrap();
        assert!(port.execute(&mut fs,domain,&frame(6,[70;16],0,0,&initial.head().digest)).is_err());
        assert!(port.execute(&mut fs,domain,&frame(4,[70;16],0,0,&[0;8])).is_err());
        prepare(&mut port,&mut fs,&plan,plan.head().request);
        // Préparation inactive : la tête durable désigne encore les droits anciens.
        assert_eq!(a.current().unwrap(),*initial.head());
        let mut reg=Registry::open(&mut fs,&mut a,domain).unwrap();
        let grant=reg.resolve_verified_factor([2;16]).unwrap();
        assert!(reg.verify_folder(grant.subject(),100,b"ACL lecture P").is_ok());
        let next=reg.revoke_folder(grant,100,seal_digest(b"ACL lecture P"),[9;16]).unwrap();
        assert!(reg.revoke_folder(grant,101,seal_digest(b"ACL lecture L"),[10;16]).is_err());
        drop(reg);
        let current=view(Triplet::capture(&fs,&a,domain).unwrap());
        assert_eq!(current.head(),next);
        assert_eq!(*plan.head(),next);
        assert!(current.folder(stale,101).is_err());
        let owner=current.resolve_verified_factor([2;16]).unwrap();
        assert!(current.folder(owner,100).is_err());
        assert_eq!(current.folder(owner,101).unwrap(),seal_digest(b"ACL lecture L"));
    }
}
