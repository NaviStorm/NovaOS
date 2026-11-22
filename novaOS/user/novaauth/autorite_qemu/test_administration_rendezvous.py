"""Exécute le corps Rust du rendez-vous avec horloge et pairs contrôlés.
Un événement absent, une autre session ou un délai expiré ne libère jamais OK.
"""
from pathlib import Path
import os,runpy,subprocess,tempfile,unittest

class Rendezvous(unittest.TestCase):
    def test_bounded_reply_waits_for_current_peer_and_rechecks_alice(self):
        service=Path(__file__).resolve().parents[1]/'service'
        body=runpy.run_path(str(service/'administration_rendezvous_profile.py'))['PENDING']
        source=STUB+'\nfn poll(lane:&mut Lane,login:&mut Login,peer:&Peer,key:u64) { for _ in 0..1 {\n'+body+'\n}}\n'+TEST
        with tempfile.TemporaryDirectory(prefix='r2-rendezvous-',dir=os.environ['NOVAOS_TMPDIR']) as directory:
            p=Path(directory);(p/'test.rs').write_text(source)
            subprocess.run(['/Users/thierry/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustc','--edition=2024','--test',str(p/'test.rs'),'-o',str(p/'test')],check=True,capture_output=True)
            subprocess.run([str(p/'test')],check=True,capture_output=True)
STUB=r'''
use std::sync::{Mutex,atomic::{AtomicU64,Ordering}};
static NOW:AtomicU64=AtomicU64::new(100);
static ANSWERS:Mutex<Vec<(u64,u64,[u8;128])>>=Mutex::new(Vec::new());
fn now()->Result<u64,u64> {Ok(NOW.load(Ordering::Relaxed))}
mod rt {pub const TIMED_OUT:u64=9;}
struct Lane {pending:Option<(u64,[u8;128],[u8;32],u64)>}
struct Login {live:bool,checks:usize}
impl Login {fn inspected_binding(&mut self,sid:[u8;32])->Result<(),u64> {
 self.checks+=1;if self.live && sid==[1;32] {Ok(())} else {Err(10)}
}}
struct Peer {r2_ready:Option<[u8;32]>,live:Option<([u8;32],())>}
fn send_answer(key:u64,tag:u64,answer:&[u8;128]) {ANSWERS.lock().unwrap().push((key,tag,*answer));}
fn pending()->Lane {Lane{pending:Some((41,[0;128],[1;32],200))}}
fn status()->u64 {u64::from_le_bytes(ANSWERS.lock().unwrap().last().unwrap().2[..8].try_into().unwrap())}
'''
TEST=r'''
#[test]fn events() {
 let mut lane=pending();let mut alice=Login{live:true,checks:0};
 let mut bob=Peer{r2_ready:None,live:Some(([2;32],()))};
 poll(&mut lane,&mut alice,&bob,7);assert!(lane.pending.is_some());assert!(ANSWERS.lock().unwrap().is_empty());
 bob.r2_ready=Some([3;32]);poll(&mut lane,&mut alice,&bob,7);assert!(lane.pending.is_some());
 bob.r2_ready=Some([2;32]);poll(&mut lane,&mut alice,&bob,7);assert!(lane.pending.is_none());assert_eq!(status(),0);assert_eq!(alice.checks,1);
 poll(&mut lane,&mut alice,&bob,7);assert_eq!(ANSWERS.lock().unwrap().len(),1);
 lane=pending();alice.live=false;poll(&mut lane,&mut alice,&bob,7);assert_eq!(status(),10);
 lane=pending();alice.live=true;bob.r2_ready=None;NOW.store(200,Ordering::Relaxed);
 poll(&mut lane,&mut alice,&bob,7);assert_eq!(status(),9);assert!(lane.pending.is_none());
 lane=pending();bob.r2_ready=Some([2;32]);poll(&mut lane,&mut alice,&bob,7);assert_eq!(status(),9);
 assert!(ANSWERS.lock().unwrap().iter().all(|(key,tag,_)|*key==7&&*tag==41));
}
'''
