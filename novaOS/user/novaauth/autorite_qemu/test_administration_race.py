"""Le rendez-vous S7 réel refuse absence et substitution avant SPAWN."""
from pathlib import Path
import os,runpy,subprocess,tempfile,unittest
class Race(unittest.TestCase):
 def test_correlated_event_before_launch(self):
  service=Path(__file__).resolve().parents[1]/'service'
  body=runpy.run_path(str(service/'administration_race_profile.py'))['RACE'].split('    probe(31,0,0,0);')[0]
  code=PREFIX+'fn check(issue:rt::Message)->rt::Result<()> {'+body+'Ok(())}'+TEST
  with tempfile.TemporaryDirectory(prefix='r2-course-',dir=os.environ['NOVAOS_TMPDIR']) as name:
   p=Path(name);(p/'test.rs').write_text(code)
   r=subprocess.run(['/Users/thierry/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustc','--edition=2024','--test',str(p/'test.rs'),'-o',str(p/'test')],capture_output=True,text=True)
   self.assertEqual(r.returncode,0,r.stderr)
   r=subprocess.run([str(p/'test')],capture_output=True,text=True);self.assertEqual(r.returncode,0,r.stdout+r.stderr)
PREFIX=r'''
use std::sync::Mutex;
static REPLY:Mutex<Option<rt::Message>>=Mutex::new(None);
mod rt {
 pub const DENIED:u64=5;
 pub type Result<T>=core::result::Result<T,u64>;
 #[derive(Clone,Copy)]pub struct Message {pub kind:u64,pub tag:u64,pub len:u64,pub payload:[u8;128]}
 pub fn config(n:u64)->u64 {assert_eq!(n,17);n}
 pub fn receive_for(k:u64,t:u64)->Result<Message> {assert_eq!((k,t),(17,1250));super::REPLY.lock().unwrap().take().ok_or(8)}
}
'''
TEST=r'''
#[test]fn events() {
 let mut issue=rt::Message{kind:1,tag:0x4e58523253544152,len:128,payload:[3;128]};issue.payload[..8].copy_from_slice(b"NXR2GO01");
 let mut good=issue;good.payload[..8].copy_from_slice(b"NXR2RN01");
 *REPLY.lock().unwrap()=Some(good);assert_eq!(check(issue),Ok(()));
 assert_eq!(check(issue),Err(8));
 for i in 0..7 {
  let mut bad=good;
  match i {0=>bad.kind=2,1=>bad.tag+=1,2=>bad.len=127,3=>bad.payload[8]^=1,4=>bad.payload[40]^=1,5=>bad.payload[72]^=1,_=>bad.payload[88]^=1}
  *REPLY.lock().unwrap()=Some(bad);assert_eq!(check(issue),Err(5));
 }
 *REPLY.lock().unwrap()=Some(issue);assert_eq!(check(issue),Err(5));
}
'''
if __name__=='__main__':unittest.main()
