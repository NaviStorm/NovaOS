//! Préparation de données synthétiques pour les lectures VFS QEMU.
//! Les mutations réalisées ici restent des témoins hôtes, pas des publications invitées.
use novafs::{Ids,Mode,Engine,Kind,device::FileImage};
use novaos_identity::{*,lab::Ledger,recuperation::Triplet};
use sha2::{Digest,Sha256};
use std::{fs::OpenOptions,path::Path,io::Write};
fn public(path:&str)->[u8;32] {
    let bytes=std::fs::read(path).unwrap();
    if bytes.len()==32 {return bytes.try_into().unwrap();}
    let text=std::str::from_utf8(&bytes).unwrap().trim();assert_eq!(text.len(),64);
    core::array::from_fn(|i|u8::from_str_radix(&text[2*i..2*i+2],16).unwrap())
}
fn reference(key:[u8;32])->Id {
    Sha256::digest([b"NovaAuth/Ed25519/credential/v1".as_slice(),&[7;16],&key].concat())[..16].try_into().unwrap()
}
fn save(path:&Path,data:&[u8]) {
    let mut f=OpenOptions::new().write(true).create_new(true).open(path).unwrap();
    f.write_all(data).unwrap();f.sync_all().unwrap();
}
fn main() {
    let args:Vec<_>=std::env::args().collect();
    let root=Path::new(&args[1]);std::fs::create_dir(root).unwrap();let sector:u32=args[2].parse().unwrap();
    let alice_ref=reference(public(&args[3]));let bob_ref=reference(public(&args[4]));
    let image=|name:&str,blocks:u64| {
        let f=OpenOptions::new().read(true).write(true).create_new(true).open(root.join(name)).unwrap();
        f.set_len(blocks*4096).unwrap();FileImage::from_file(f,blocks,sector).unwrap()
    };
    let mut authority=Ledger::new(image("autorite-initiale.raw",12));
    let ids=Ids {fs:[8;16],lv:[9;16]};let domain=[7;16];
    let mut fs=provision(image("identites.p9",512),ids,domain,[4;16],&mut authority).unwrap();
    let proof=Enrollment::from_provisioner(&mut authority).unwrap();
    let mut reg=Registry::open(&mut fs,&mut authority,domain).unwrap();
    let admin=reg.enroll(&proof,[1;16],"admin",[2;16],"base").unwrap();
    let group=created_group(&admin);
    let grant=reg.resolve_verified_factor([2;16]).unwrap();
    let alice=reg.mutate(grant,[3;16],Change::Create {name:"alice",credential:alice_ref,group}).unwrap();
    let owner=created_user(&alice);
    let grant=reg.resolve_verified_factor([2;16]).unwrap();
    reg.mutate(grant,[5;16],Change::Create {name:"bob",credential:bob_ref,group}).unwrap();
    drop(reg);
    let parent=fs.create_at(fs.root_id(),"\u{1f}NovaAuth-D",Kind::Directory).unwrap();
    let mut folders=Vec::new();
    for (name,data) in [("P",b"ALICE-P\n"),("L",b"ALICE-L\n")] {
        let dir=fs.create_at(parent.id,name,Kind::Directory).unwrap();
        let file=fs.create_at(dir.id,"registre",Kind::File).unwrap();
        fs.prepare_write_id(file.id,0,data).unwrap().commit().unwrap();
        let protected=fs.create_at(dir.id,"prive",Kind::Directory).unwrap();
        let secret=fs.create_at(protected.id,"temoin",Kind::File).unwrap();
        fs.prepare_write_id(secret.id,0,b"PROTEGE\n").unwrap().commit().unwrap();
        let policy=fs.create_at(dir.id,"politique",Kind::File).unwrap();
        let mut seal=[0;104];seal[..8].copy_from_slice(b"NADIR001");
        seal[8..24].copy_from_slice(&owner);seal[24..40].copy_from_slice(&ids.fs);seal[40..56].copy_from_slice(&ids.lv);
        for (i,id) in [dir.id,file.id,protected.id,secret.id,policy.id].into_iter().enumerate() {
            seal[56+8*i..64+8*i].copy_from_slice(&id.to_le_bytes());
        }
        seal[96]=1;seal[97]=1;
        fs.prepare_write_id(policy.id,0,&seal).unwrap().commit().unwrap();
        let head=authority.current().unwrap();
        let mut reg=Registry::open(&mut fs,&mut authority,domain).unwrap();
        reg.bind_folder(head,owner,dir.id,&seal,[10+folders.len() as u8;16]).unwrap();
        folders.push((dir.id,seal));
    }
    let original=Triplet::capture(&fs,&authority,domain).unwrap();
    save(&root.join("triplet-autorise.bin"),&original.encode());
    save(&root.join("dossiers.bin"),&folders.iter().flat_map(|(id,_)|id.to_le_bytes()).collect::<Vec<_>>());
    fs.unmount().unwrap();
    std::fs::copy(root.join("identites.p9"),root.join("ancien.p9")).unwrap();
    std::fs::copy(root.join("autorite-initiale.raw"),root.join("ancienne-autorite.raw")).unwrap();
    let file=OpenOptions::new().read(true).write(true).open(root.join("identites.p9")).unwrap();
    let mut fs=Engine::mount_current(FileImage::from_file(file,512,sector).unwrap(),ids,Mode::ReadWrite).unwrap();
    let mut reg=Registry::open(&mut fs,&mut authority,domain).unwrap();
    let grant=reg.resolve_verified_factor(alice_ref).unwrap();
    reg.revoke_folder(grant,folders[0].0,Sha256::digest(folders[0].1).into(),[20;16]).unwrap();
    drop(reg);
    let revoked=Triplet::capture(&fs,&authority,domain).unwrap();
    save(&root.join("triplet-revoque.bin"),&revoked.encode());
    assert_eq!(fs.lookup_at(parent.id,"P").unwrap().id,folders[0].0);
    fs.unmount().unwrap();
    println!("préparation synthétique hôte génération={} puis={} P={} L={}; aucune preuve VFS invitée",
        original.head().generation,revoked.head().generation,folders[0].0,folders[1].0);
}
