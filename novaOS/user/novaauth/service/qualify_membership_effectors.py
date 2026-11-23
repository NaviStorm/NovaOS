"""Génère un harnais natif à partir des types et méthodes Directory réels.
Directory conserve ses types et méthodes réels ; le retour IPC est contrôlé.
L’inventaire EXEC utilise des structures minimales, sans ordonnanceur.
Ce harnais ne constitue pas un essai invité ni une preuve de concurrence noyau.
"""
from pathlib import Path
import argparse, json, hashlib, re

def prepare(source, destination):
    source=Path(source).resolve();destination=Path(destination)
    statuses={'OK':0,'BAD_ARGUMENT':3,'DENIED':5,'LIMIT':6,'WOULD_BLOCK':7,'CANCELLED':9,'REVOKED':10,'CLOSED':11,'REFUSED':12}
    abi=(source/'novaOS/abi/native.rs').read_text()
    for name,value in statuses.items():
        match=re.search(r'pub const '+name+r': u64 = (\d+);',abi)
        if match is None or int(match[1])!=value:raise ValueError('statut natif divergent : '+name)

    destination.mkdir(parents=True,exist_ok=False)
    p=source/'novaOS/user/novafs/files/auth_directory.rs'
    text=p.read_text().split("impl Session<'_, '_> {",1)[0]
    text=text[text.index('const READ:'):]
    baseline=text
    text=text.replace('    #[cfg(feature = "politiques-lab")]\n    permission:', '    permission:')
    text=text.replace('            #[cfg(feature = "politiques-lab")]\n            permission:', '            permission:')
    text=text.replace('    #[cfg(feature = "politiques-lab")]\n    sleeper:', '    sleeper:')
    text=text.replace('    next: u64,','    membership: Option<GroupClosure>,\n    next: u64,')
    runtime='''#![allow(dead_code)]
use novaos_identity::policy::{Origin,Permission};
mod rt {
    pub type Result<T>=core::result::Result<T,u64>;
    pub const OK:u64=0; pub const DENIED:u64=5; pub const LIMIT:u64=6;
    pub const REVOKED:u64=10;pub const CLOSED:u64=11; pub const REFUSED:u64=12;
    pub const BAD_ARGUMENT:u64=3;pub const WOULD_BLOCK:u64=7;pub const CANCELLED:u64=9;
    std::thread_local! {pub static STATUS:core::cell::Cell<u64>=const{core::cell::Cell::new(0)};
        pub static SENDS:core::cell::Cell<u64>=const{core::cell::Cell::new(0)};}
    pub fn send_frame(_:u64,_:u64,b:&[u8;128],_:u64,_:bool)->Result<()> {
        assert_eq!(&b[120..],b"DIRREPLY");assert_eq!(&b[..8],&CANCELLED.to_le_bytes());
        SENDS.with(|s|s.set(s.get()+1));match STATUS.with(|s|s.get()) {0=>Ok(()),e=>Err(e)}
    }
}
'''
    vfs=source/'novaOS/user/novaauth/service/membership_vfs.rs'
    tests=source/'novaOS/user/novaauth/service/membership_vfs_tests.rs'
    # Chemins de source absolus : le test exerce le fichier effectivement publié.
    body=vfs.read_text().replace('//!','//').replace('use novaos_identity::policy::{Origin, Permission};','')
    (destination/'src').mkdir()
    (destination/'src/lib.rs').write_text(runtime+'mod directory {\nuse super::*;\n'+text+'\n'+body+'\n'+tests.read_text()+'\n}\n')
    with (destination/'src/lib.rs').open('a') as out:
        out.write('\nmod baseline { use super::*;\n'+baseline+'\n}\n')
        out.write("""
#[test] fn incremental_control_budget_is_measured_without_counting_legacy_state_twice() {
    let old=core::mem::size_of::<baseline::Directory>();
    let new=core::mem::size_of::<directory::Directory>();
    let added=new.checked_sub(old).unwrap();
    let subtotal=added+core::mem::size_of::<novaos_recovery::membership::Inventory>()
        +novaos_recovery::membership::BODY+256;
    println!("Directory before={} after={} delta={} measured_control_subtotal={}",old,new,added,subtotal);
    assert!(subtotal<=4096);
}
""")
    manifest='''[package]
name="membership-effectors-native"
version="0.0.0"
edition="2024"
[workspace]
[features]
politiques-lab=[]
[dependencies]
'''
    manifest+=f'novaos-identity={{path={json.dumps(str(source/"novaOS/user/novaauth"))}, features=["administration-lab"]}}\n'
    manifest+=f'novaos-recovery={{path={json.dumps(str(source/"novaOS/user/novaauth/recuperation"))}, features=["administration-lab"]}}\n'
    (destination/'Cargo.toml').write_text(manifest)
    execution=source/'novaOS/user/novaauth/service/exec_membership_lab.rs'
    exec_tests=source/'novaOS/user/novaauth/service/membership_exec_tests.rs'
    prelude="""
mod execution {
    type Pid=u64;type Res<T>=Result<T,u64>;
    const WOULD_BLOCK:u64=7;const LIMIT:u64=6;
    struct Binding{holder:[u8;16],domain:[u8;16],installation:[u8;16]}
    struct Ticket{issuer:Pid,binding:Binding,scope:[u64;3],remote:Option<()>,unchanged_state:u64}
    struct Process{exec:Vec<Ticket>}struct Processes{slots:Vec<Option<Process>>}struct Core{processes:Processes}
"""
    with (destination/'src/lib.rs').open('a') as out:
        out.write(prelude+execution.read_text().replace('//!','//')+'\n'+exec_tests.read_text()+'\n}\n')

    (destination/'sources.json').write_text(json.dumps({str(x.relative_to(source)):hashlib.sha256(x.read_bytes()).hexdigest() for x in (p,vfs,tests,execution,exec_tests,Path(__file__).resolve(),source/"novaOS/abi/native.rs")},indent=2)+'\n')
if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('source',type=Path);p.add_argument('destination',type=Path);a=p.parse_args();prepare(a.source,a.destination)
