"""Sonde invitée réelle sous doublures : événement absent/substitué != succès."""
from pathlib import Path
import os,runpy,subprocess,tempfile,unittest
SERVICE=Path(__file__).resolve().parents[1]/'service'
class Separation(unittest.TestCase):
    def test_real_guest_probe(self):
        body=runpy.run_path(str(SERVICE/'administration_separation_profile.py'))['BOB']
        code=STUB+'\nmod bob {\n'+body+'\n}\n'+TEST
        with tempfile.TemporaryDirectory(prefix='r2-separation-',dir=os.environ['NOVAOS_TMPDIR']) as name:
            p=Path(name);(p/'test.rs').write_text(code)
            r=subprocess.run(['/Users/thierry/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustc','--edition=2024','--test',str(p/'test.rs'),'-o',str(p/'test')],capture_output=True,text=True)
            self.assertEqual(r.returncode,0,r.stderr)
            r=subprocess.run([str(p/'test')],capture_output=True,text=True)
            self.assertEqual(r.returncode,0,r.stdout+r.stderr)
STUB=r'''
use std::sync::{Mutex,atomic::{AtomicU64,Ordering}};
static QUEUE:Mutex<std::collections::VecDeque<rt::Message>>=Mutex::new(std::collections::VecDeque::new());
static ATTEMPTS:AtomicU64=AtomicU64::new(0);
mod tty {pub const SPAWN:u64=20;pub const EXEC_TICKET:u64=2;}
mod rt {
 pub const DENIED:u64=5;pub const TIMED_OUT:u64=8;pub const WOULD_BLOCK:u64=9;pub const CANCELLED:u64=7;pub const REFUSED:u64=12;pub const DEBUG_WRITE:u64=40;
 pub type Result<T>=core::result::Result<T,u64>;
 #[derive(Clone,Copy)]pub struct Message{pub kind:u64,pub tag:u64,pub len:u64,pub payload:[u8;128]}
 pub struct Reply{pub status:u64,pub value:u64}
 pub fn config(n:usize)->u64{n as u64}
 pub fn receive_for(_:u64,ticks:u64)->Result<Message>{assert!(ticks<=15000);super::QUEUE.lock().unwrap().pop_front().ok_or(8)}
 pub fn value(r:Reply)->Result<u64>{if r.status==0 {Ok(r.value)}else{Err(r.status)}}
 pub fn call(op:u64,a:u64,b:u64,c:u64)->Reply {
  if op==20 {assert_eq!((a,b,c),(2,100,0));super::ATTEMPTS.fetch_add(1,super::Ordering::Relaxed);}
  Reply{status:if op==DEBUG_WRITE {0}else{5},value:0}
 }
 pub fn send_for(key:u64,tag:u64,payload:&[u8],ticks:u64)->Result<()> {assert_eq!((key,tag,ticks),(101,1,25));assert_eq!(payload,[0;128]);super::ATTEMPTS.fetch_add(1,super::Ordering::Relaxed);Err(5)}
}
mod session_auth_lab {
 #[derive(Default)]pub struct Login;
 impl Login {
  pub fn authenticate(&mut self)->Result<(),u64>{Ok(())}
  pub fn close(&mut self)->Result<(),u64>{Ok(())}
  pub fn request(&mut self,op:&[u8;8],value:&[u8])->Result<[u8;128],u64> {
   match op {b"NSHLIVE1"=>{assert_eq!(value,[3;32]);},b"NSHEXBG1"=>{assert_eq!(&value[..32],[3;32]);},b"NXR2REFD"=>{assert_eq!(value,[4;16]);return Ok([0;128]);},_=>panic!("unexpected op")}
   super::ATTEMPTS.fetch_add(1,super::Ordering::Relaxed);Err(5)
  }
 }
}
fn event()->rt::Message {let mut p=[0;128];p[..8].copy_from_slice(b"NXR2REF1");p[8..40].fill(2);p[40..72].fill(3);p[72..80].copy_from_slice(&100u64.to_le_bytes());p[80..88].copy_from_slice(&101u64.to_le_bytes());p[88..104].fill(4);rt::Message{kind:1,tag:0x4e58523252454653,len:128,payload:p}}
fn run(events:Vec<rt::Message>)->(Result<(),u64>,u64) {ATTEMPTS.store(0,Ordering::Relaxed);*QUEUE.lock().unwrap()=events.into();(bob::session(),ATTEMPTS.load(Ordering::Relaxed))}
'''
TEST=r'''
#[test]fn events() {
 let good=event();let stop=rt::Message{kind:1,tag:0,len:1,payload:[0;128]};
 assert_eq!(run(vec![good,stop]),(Ok(()),4));
 assert_eq!(run(vec![]),(Err(8),0));
 for field in 0..6 {let mut e=good;match field {0=>e.tag+=1,1=>e.len=127,2=>e.payload[0]^=1,3=>e.payload[104]=1,4=>e.payload[8..40].fill(0),_=>e.payload[88..104].fill(0)}assert_eq!(run(vec![e,stop]),(Err(5),0));}
}
'''
if __name__=='__main__':unittest.main()
