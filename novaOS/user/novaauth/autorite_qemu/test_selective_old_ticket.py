"""Exécuter le code du rendez-vous terminal : pas de succès sans identité exacte."""
from pathlib import Path
import os,runpy,subprocess,tempfile,unittest

class OldDirect(unittest.TestCase):
 def test_terminal_event_bound_and_complete_on_both_native_targets(self):
  service=Path(__file__).resolve().parents[1]/'service'
  body=runpy.run_path(str(service/'selective_old_ticket_profile.py'))['DONE']
  grammar=service.parents[3]/'components/exec-client/src/admin_command.rs'
  source=STUB.replace('GRAMMAR',str(grammar))+body+'\nOk(())}}\n'+TEST
  with tempfile.TemporaryDirectory(prefix='r2-old-direct-',dir=os.environ['NOVAOS_TMPDIR']) as directory:
   p=Path(directory);(p/'test.rs').write_text(source)
   for target in ('aarch64-apple-darwin','x86_64-apple-darwin'):
    subprocess.run(['/Users/thierry/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustc','--edition=2024','--test','--target',target,str(p/'test.rs'),'-o',str(p/target)],check=True,capture_output=True)
    subprocess.run([str(p/target)],check=True,capture_output=True)
STUB=r'''
#[path="GRAMMAR"] mod admin_command;
mod rt {
 pub const DENIED:u64=5;
 pub static SENT:std::sync::Mutex<Vec<(u64,u64,[u8;128],u64)>>=std::sync::Mutex::new(Vec::new());
 pub fn send_for(p:u64,t:u64,q:&[u8;128],d:u64)->Result<(),u64>{SENT.lock().unwrap().push((p,t,*q,d));Ok(())}
}
struct Route{client:[u8;32]}
struct Login{slot:usize,route:Route}
impl Login{fn port(&self,n:usize)->u64{assert_eq!(n,16);77}}
struct Channels{prepared:Option<([u8;16],[u8;32],[u8;32])>,bob_triggered:bool}
impl Channels{fn event(&mut self,login:&Login,peer:&Login,request:&[u8;128],answer:[u8;128],sid:[u8;32])->Result<(),u64>{
'''
TEST=r'''
#[test]fn terminal() {
 let alice=Login{slot:0,route:Route{client:[1;32]}};
 let bob=Login{slot:1,route:Route{client:[2;32]}};
 let sid=[3;32];let operation=[4;16];let digest=[5;32];
 let mut q=[0;128];q[..8].copy_from_slice(b"NXACMD01");
 // Utiliser l'encodage canonique réellement décodé côté vérificateur.
 q[8]=32;q[16..32].copy_from_slice(&operation);q[32..64].copy_from_slice(&digest);
 assert_eq!(admin_command::decode(&q),Ok(admin_command::Command::Confirm{operation,digest}));
 let mut a=[0;128];a[8..24].copy_from_slice(&operation);a[24..32].copy_from_slice(&7u64.to_le_bytes());a[32]=1;
 let mut c=Channels{prepared:Some((operation,digest,sid)),bob_triggered:true};
 assert_eq!(c.event(&alice,&bob,&q,a,sid),Ok(()));
 let sent=rt::SENT.lock().unwrap().pop().unwrap();assert_eq!((sent.0,sent.1,sent.3),(77,0x4e58523253544152,25));
 assert_eq!(&sent.2[..8],b"NXR2DN01");assert_eq!(sent.2[8..40],bob.route.client);assert_eq!(sent.2[40..72],alice.route.client);
 assert_eq!(sent.2[72..88],operation);assert_eq!(sent.2[88..120],digest);assert_eq!(sent.2[120..],[0;8]);
 for offset in [8,32,33,127]{let mut wrong=a;wrong[offset]^=1;assert_eq!(c.event(&alice,&bob,&q,wrong,sid),Err(5));}
 assert_eq!(c.event(&alice,&bob,&q,a,[9;32]),Err(5));
 c.bob_triggered=false;assert_eq!(c.event(&alice,&bob,&q,a,sid),Err(5));
 c.bob_triggered=true;c.prepared=Some((operation,[6;32],sid));assert_eq!(c.event(&alice,&bob,&q,a,sid),Err(5));
 assert!(rt::SENT.lock().unwrap().is_empty());
}
'''
