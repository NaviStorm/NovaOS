//! Audit hors invité, lecture seule : ne constitue jamais une autorité d'activation.
use novafs::{Ids,Mode,Engine,device::FileImage};
use novaos_identity::{*,recuperation::{Triplet,decode_head},recuperation_view::View};
use std::fs::OpenOptions;
struct Expected(Head);
impl Authority for Expected {
 fn current(&self)->Result<Head>{Ok(self.0)}
 fn outcome(&self,_:Id)->Result<Option<Head>>{Err(Fault::Denied)}
 fn decide(&mut self,_:Head,_:Head)->Result<()>{Err(Fault::Denied)}
}
fn main(){
 let a:Vec<_>=std::env::args().collect();
 let sector:u32=a[2].parse().unwrap();
 if a.len()==3 {
   let f=OpenOptions::new().read(true).open(&a[1]).unwrap();
   let fs=Engine::mount_current(FileImage::from_file(f,512,sector).unwrap(),
     Ids{fs:[8;16],lv:[9;16]},Mode::ReadOnly).unwrap();
   let root=fs.lookup_at(fs.root_id(),"\u{1f}NovaAuth-D").unwrap();
   for (name,expected) in [("P",b"ALICE-P\n"),("L",b"ALICE-L\n")] {
     let dir=fs.lookup_at(root.id,name).unwrap();
     let file=fs.lookup_at(dir.id,"registre").unwrap();
     assert_eq!(file.size,8);
     assert_eq!(fs.inspect_id(file.id,0,8).unwrap(),expected);
   }
   println!("audit hôte lecture seule : fichiers P et L intacts, huit octets chacun ; aucun accès invité autorisé par cet audit");
   return;
 }
 let old=std::fs::read(&a[3]).unwrap();
 let oldhead=decode_head(&old[8..152]).unwrap();
 let manifest=std::fs::read(&a[4]).unwrap();
 let newhead=decode_head(&manifest[158..302]).unwrap();
 let f=OpenOptions::new().read(true).open(&a[1]).unwrap();
 let image=FileImage::from_file(f,512,sector).unwrap();
 let fs=Engine::mount_current(image,Ids{fs:[8;16],lv:[9;16]},Mode::ReadOnly).unwrap();
 let triplet=Triplet::capture(&fs,&Expected(oldhead),[7;16]).unwrap();
 assert_eq!(triplet.encode(),old);
 assert!(Triplet::capture(&fs,&Expected(newhead),[7;16]).is_err(),
     "l'instantané contient déjà le triplet révoqué");
 let parts=triplet.parts();
 View::read(oldhead,parts.map(|x|x.len() as u32),|table,offset,out|{
   let source=&parts[table][offset as usize..];let n=out.len().min(source.len());
   out[..n].copy_from_slice(&source[..n]);Ok(n)
 }).unwrap();
 println!("audit hôte lecture seule : ancien triplet exact génération={} ; triplet révoqué génération={} absent ; aucun droit activé",
     oldhead.generation,newhead.generation);
}
