//! Jeu de données isolé pour qualifier l'effecteur triplet, pas la recette VFS.
use novafs::{Ids,Mode,Engine,device::FileImage};
use novaos_identity::{*,lab::Ledger,recuperation::Triplet};
use sha2::{Digest,Sha256};
use std::{fs::{File,OpenOptions},path::Path,io::Write};
fn main() {
    let a:Vec<_>=std::env::args().collect();
    let root=Path::new(&a[1]);let sector:u32=a[2].parse().unwrap();
    let credential=if let Some(path)=a.get(3) {
        let bytes=std::fs::read(path).unwrap();
        let public=if bytes.len()==32 {bytes} else {
            let text=std::str::from_utf8(&bytes).unwrap().trim();assert_eq!(text.len(),64);
            (0..32).map(|i|u8::from_str_radix(&text[2*i..2*i+2],16).unwrap()).collect()
        };
        Sha256::digest([b"NovaAuth/Ed25519/credential/v1".as_slice(),&[7;16],&public].concat())[..16].try_into().unwrap()
    } else {[2;16]};
    let image=|name:&str,blocks:u64| {
        let f=OpenOptions::new().read(true).write(true).create_new(true).open(root.join(name)).unwrap();
        f.set_len(blocks*4096).unwrap();FileImage::from_file(f,blocks,sector).unwrap()
    };
    let mut authority=Ledger::new(image("autorite-initiale.raw",12));
    let ids=Ids {fs:[8;16],lv:[9;16]};let domain=[7;16];
    let mut fs=provision(image("identites.p9",512),ids,domain,[4;16],&mut authority).unwrap();
    let permit=Enrollment::from_provisioner(&mut authority).unwrap();
    let mut reg=Registry::open(&mut fs,&mut authority,domain).unwrap();
    let admin=reg.enroll(&permit,[1;16],"admin",credential,"base").unwrap();
    let owner=reg.resolve_verified_factor(credential).unwrap();
    let alice=reg.mutate(owner,[3;16],Change::Create {name:"alice",credential:[4;16],group:created_group(&admin)}).unwrap();
    drop(reg);fs.unmount().unwrap();
    std::fs::copy(root.join("identites.p9"),root.join("ancien.p9")).unwrap();
    let f=OpenOptions::new().read(true).write(true).open(root.join("identites.p9")).unwrap();
    let mut fs=Engine::mount_current(FileImage::from_file(f,512,sector).unwrap(),ids,Mode::ReadWrite).unwrap();
    let mut reg=Registry::open(&mut fs,&mut authority,domain).unwrap();
    let owner=reg.resolve_verified_factor(credential).unwrap();
    reg.mutate(owner,[5;16],Change::Disable(created_user(&alice))).unwrap();
    drop(reg);
    let triplet=Triplet::capture(&fs,&authority,domain).unwrap().encode();
    let mut out=OpenOptions::new().write(true).create_new(true).open(root.join("triplet.bin")).unwrap();
    out.write_all(&triplet).unwrap();out.sync_all().unwrap();
    fs.unmount().unwrap();
    let _=File::open(root).unwrap().sync_all();
}
