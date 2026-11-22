"""Deux rendez-vous privés : authentification unique, émission corrélée ensuite."""
from pathlib import Path
import os,runpy,subprocess,tempfile,unittest
SERVICE=Path(__file__).resolve().parents[1]/'service'
class Staging(unittest.TestCase):
    def test_real_bob_stage_rejects_substitution_replay_and_missing_event(self):
        body=runpy.run_path(str(SERVICE/'administration_live_ticket_profile.py'))['INITIAL']
        code=STUB+'\nfn run()->Result<(),u64>{\n'+body+'\nOk(())}\n'+TEST
        with tempfile.TemporaryDirectory(prefix='r2-staging-',dir=os.environ['NOVAOS_TMPDIR']) as name:
            p=Path(name);(p/'test.rs').write_text(code)
            r=subprocess.run(['/Users/thierry/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustc','--edition=2024','--test',str(p/'test.rs'),'-o',str(p/'test')],capture_output=True,text=True)
            self.assertEqual(r.returncode,0,r.stderr)
            r=subprocess.run([str(p/'test')],capture_output=True,text=True);self.assertEqual(r.returncode,0,r.stdout+r.stderr)
STUB=r'''
use std::sync::{Mutex,atomic::{AtomicU64,Ordering}};
static AUTH:AtomicU64=AtomicU64::new(0);
static QUEUE:Mutex<std::collections::VecDeque<rt::Message>>=Mutex::new(std::collections::VecDeque::new());
fn mark(_: &[u8])->Result<(),u64>{Ok(())}
mod rt {
 pub const DENIED:u64=5;pub type Result<T>=core::result::Result<T,u64>;
 #[derive(Clone,Copy)]pub struct Message{pub kind:u64,pub tag:u64,pub len:u64,pub payload:[u8;128]}
 pub fn config(n:usize)->u64{assert_eq!(n,17);17}
 pub fn receive_for(key:u64,ticks:u64)->Result<Message>{assert_eq!((key,ticks),(17,15000));super::QUEUE.lock().unwrap().pop_front().ok_or(9)}
}
mod session_auth_lab {
 #[derive(Default)]pub struct Login{pub identity:[u8;32]}
 impl Login {pub fn authenticate(&mut self)->Result<(),u64>{super::AUTH.fetch_add(1,super::Ordering::Relaxed);self.identity=[2;32];Ok(())}}
}
fn event(magic:&[u8;8])->rt::Message {
 let mut payload=[0;128];payload[..8].copy_from_slice(magic);payload[8..40].fill(2);payload[40..72].fill(3);payload[72..88].fill(4);payload[88..120].fill(5);
 rt::Message{kind:1,tag:0x4e58523253544152,len:128,payload}
}
fn exercise(events:Vec<rt::Message>)->(Result<(),u64>,u64) {
 AUTH.store(0,Ordering::Relaxed);*QUEUE.lock().unwrap()=events.into();(run(),AUTH.load(Ordering::Relaxed))
}
'''
TEST=r'''
#[test]fn stages() {
 let auth=event(b"NXR2AU01");let issue=event(b"NXR2GO01");
 assert_eq!(exercise(vec![auth,issue]),(Ok(()),1));
 assert_eq!(exercise(vec![auth,auth]),(Err(5),1));
 assert_eq!(exercise(vec![issue,issue]),(Err(5),0));
 assert_eq!(exercise(vec![auth]),(Err(9),1));
 for at in [8,40,72,88,120] {
  let mut changed=issue;changed.payload[at]^=1;assert_eq!(exercise(vec![auth,changed]),(Err(5),1));
 }
 let mut changed=issue;changed.tag+=1;assert_eq!(exercise(vec![auth,changed]),(Err(5),1));
 let mut changed=auth;changed.payload[8..40].fill(9);assert_eq!(exercise(vec![changed,issue]),(Err(5),1));
 let mut changed=auth;changed.payload[120]=1;assert_eq!(exercise(vec![changed,issue]),(Err(5),0));
}
'''
