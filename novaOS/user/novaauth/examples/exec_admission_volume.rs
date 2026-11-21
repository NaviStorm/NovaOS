//! Volume synthétique hors ligne : une installation attestée pour les tickets QEMU.
//! Ne constitue ni une authentification ni une preuve de lancement dans l'invité.
use novafs::{Ids,Kind,device::FileImage};
use novaos_identity::{*,lab::Ledger,recuperation::Triplet,distribution::{Action,Machine},policy::{READ,Origin}};
use sha2::{Digest,Sha256};
use std::{fs::{self,OpenOptions},path::Path,io::Write};
fn save(path:&Path,data:&[u8]) {let mut f=OpenOptions::new().create_new(true).write(true).open(path).unwrap();f.write_all(data).unwrap();f.sync_all().unwrap();}
fn main() {
 let a:Vec<_>=std::env::args().collect();let root=Path::new(&a[1]);fs::create_dir(root).unwrap();
 let sector=a[2].parse().unwrap();let machine=match a[3].as_str(){"aarch64"=>Machine::Aarch64,"x86_64"=>Machine::X86_64,_=>panic!("ISA")};
 let public=fs::read(&a[4]).unwrap();let public=if public.len()==32 {public} else {
  let h=std::str::from_utf8(&public).unwrap().trim();assert_eq!(h.len(),64);(0..32).map(|i|u8::from_str_radix(&h[i*2..i*2+2],16).unwrap()).collect()
 };
 let reference:Id=Sha256::digest([b"NovaAuth/Ed25519/credential/v1".as_slice(),&[7;16],&public].concat())[..16].try_into().unwrap();
 let image=|name:&str,blocks:u64| {let f=OpenOptions::new().create_new(true).read(true).write(true).open(root.join(name)).unwrap();f.set_len(blocks*4096).unwrap();FileImage::from_file(f,blocks,sector).unwrap()};
 let mut ledger=Ledger::new(image("autorite.raw",12));let domain=[7;16];
 let mut fs=provision(image("identites.p9",512),Ids{fs:[8;16],lv:[9;16]},domain,[4;16],&mut ledger).unwrap();
 let permit=Enrollment::from_provisioner(&mut ledger).unwrap();
 let mut reg=Registry::open(&mut fs,&mut ledger,domain).unwrap();
 let owner=created_user(&reg.enroll(&permit,[1;16],"alice",reference,"base").unwrap());drop(reg);
 let root_id=fs.root_id();let work=fs.create_at(root_id,"travail",Kind::Directory).unwrap().id;
 let file=fs.create_at(work,"donnees.tsv",Kind::File).unwrap().id;
 fs.prepare_write_id(file,0,b"A\tB\tC\nD\tE\tF\n").unwrap().commit().unwrap();
 let system=fs.lookup_at(root_id,"system").unwrap().id;fs.create_at(system,"bin",Kind::Directory).unwrap();
 let mut seal=Vec::from(*b"NXScope1");seal.extend_from_slice(&owner);seal.extend_from_slice(&work.to_le_bytes());seal.extend_from_slice(&file.to_le_bytes());
 let scope=fs.create_at(root_id,"\u{1f}NXScope",Kind::File).unwrap().id;
 fs.prepare_write_id(scope,0,&seal).unwrap().commit().unwrap();
 let head=ledger.current().unwrap();let mut reg=Registry::open(&mut fs,&mut ledger,domain).unwrap();
 reg.bind_folder(head,owner,work,&seal,[152;16]).unwrap();
 let key=distribution::public_key(&fs::read(&a[5]).unwrap().try_into().unwrap()).unwrap();
 for (id,name,path) in [(150,"cut",&a[6]),(151,"cat",&a[7])] {
  let grant=reg.grant_attested(owner).unwrap();reg.publish_package(grant,[id;16],Action::Install,&key,machine,&fs::read(path).unwrap()).unwrap();
  let grant=reg.grant_attested(owner).unwrap();let app=reg.load_package(grant,&key,machine,name).unwrap().principal.unwrap();
  reg.set_permission(grant,owner,work,app,Origin::Direct,READ,[id+3;16]).unwrap();
 }
 let mode=a.get(8).map(String::as_str).unwrap_or("nominal");
 let grant=reg.grant_attested(owner).unwrap();
 let cut=reg.load_package(grant,&key,machine,"cut").unwrap();
 if mode=="revoque" {reg.withdraw_package(grant,[160;16],cut.installation).unwrap();}
 assert!(matches!(mode,"nominal"|"revoque"|"modifie"));
 drop(reg);
 if mode=="modifie" {
  let altered=[cut.bytes[0]^1];
  fs.prepare_write_id(cut.object,cut.offset,&altered).unwrap().commit().unwrap();
 }
 let triplet=Triplet::capture(&fs,&ledger,domain).unwrap();
 save(&root.join("triplet.bin"),&triplet.encode());fs.unmount().unwrap();
 println!("fixture hors ligne : deux installations attestées, génération={}, secteur={sector}",triplet.head().generation);
}
