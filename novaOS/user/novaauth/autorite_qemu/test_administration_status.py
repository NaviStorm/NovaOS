"""Marqueur de statut QEMU : octets exacts, PID, refus et échec de DEBUG."""
from pathlib import Path
import os,re,runpy,subprocess,tempfile,unittest
from administration_events import observed_command_status_pattern

SERVICE=Path(__file__).resolve().parents[1]/'service'
ROOT=SERVICE.parents[3]

class Status(unittest.TestCase):
    def test_debug_record_preserves_pid_and_status(self):
        for pid in (5,11):
            for status in (0,12,20):
                raw=f'DEBUG pid={pid} NXR2_1_{status}__FIN\r\n'.encode()
                m=re.search(observed_command_status_pattern('NXR2_1_'),raw)
                self.assertEqual((int(m[1]),int(m[2])),(pid,status))
        for raw in (b'echo NXR2_1_$?__FIN',b'NXR2_1_0__FIN',
                    b'DEBUGNXR2_ pid=1_0__FIN11 NAR2 Bob ferm\r\neture acquittee\r\n',
                    b'DEBUG pid=5 NXR2_0_0__FIN\r\n'):
            self.assertIsNone(re.search(observed_command_status_pattern('NXR2_1_'),raw))

    def test_text_cannot_substitute_kernel_pid(self):
        raw=b'DEBUG pid=11 DEBUG pid=5 NXR2_1_0__FIN\r\n'
        self.assertIsNone(re.search(observed_command_status_pattern('NXR2_1_'),raw))
        raw=b'DEBUG pid=11 NXR2_1_0__FIN\r\n'
        self.assertEqual(re.search(observed_command_status_pattern('NXR2_1_'),raw)[1],b'11')

    def test_generated_echo_uses_existing_debug_and_propagates_failure(self):
        profile=runpy.run_path(str(SERVICE/'administration_status_profile.py'))
        with tempfile.TemporaryDirectory(dir=os.environ['NOVAOS_TMPDIR'],prefix='r2-status-') as name:
            root=Path(name);p=root/'components/shell/script.rs';p.parent.mkdir(parents=True)
            original=(ROOT/'components/shell/script.rs').read_text();p.write_text(original)
            profile['install'](root)
            with self.assertRaises(ValueError):profile['install'](root)
            patched=p.read_text();start=patched.index('            "echo" => {');end=patched.index('            "test" | "["',start)
            body=patched[start:end]
            source=STUB+'\nfn echo(args:Vec<String>)->Result<u64,u64> {Ok(match "echo" {\n'+body+'_=>unreachable!()})}\n'+profile['HELPER']+TEST
            (root/'test.rs').write_text(source)
            subprocess.run(['/Users/thierry/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustc','--edition=2024','--test',str(root/'test.rs'),'-o',str(root/'test')],check=True,capture_output=True)
            subprocess.run([str(root/'test')],check=True,capture_output=True)
            self.assertEqual((ROOT/'components/shell/script.rs').read_text(),original)

STUB=r'''
use std::sync::Mutex;
static TRACE:Mutex<Vec<Vec<u8>>>=Mutex::new(Vec::new());
static CONSOLE:Mutex<String>=Mutex::new(String::new());
static FAILURE:std::sync::atomic::AtomicBool=std::sync::atomic::AtomicBool::new(false);
mod rt {
 pub const DEBUG_WRITE:u64=3;
 pub fn call(op:u64,p:u64,n:u64,z:u64)->Result<u64,u64> {
  assert_eq!((op,z),(DEBUG_WRITE,0));
  if super::FAILURE.load(std::sync::atomic::Ordering::Relaxed) {return Err(6)}
  super::TRACE.lock().unwrap().push(unsafe{std::slice::from_raw_parts(p as *const u8,n as usize)}.to_vec());Ok(n)
 }
 pub fn value(r:Result<u64,u64>)->Result<u64,u64>{r}
}
mod tty {pub const OUTPUT:u64=2;}
mod console {
 pub fn printable(p:u64,s:&str)->Result<(),u64>{write(p,s)}
 pub fn write(p:u64,s:&str)->Result<(),u64>{assert_eq!(p,2);super::CONSOLE.lock().unwrap().push_str(s);Ok(())}
}
'''
TEST=r'''
#[test]fn exact_behavior() {
 assert_eq!(echo(vec!["NXR2_1_12__FIN".into()]),Ok(0));
 assert_eq!(*TRACE.lock().unwrap(),vec![b"NXR2_1_12__FIN".to_vec()]);assert!(CONSOLE.lock().unwrap().is_empty());
 for s in ["NXR2_1_$?__FIN","NXR2_1_0__FI","NXR2__0__FIN","NXR2_1___FIN","private text"] {
  assert!(!r2_status_marker(s));assert_eq!(echo(vec![s.into()]),Ok(0));
 }
 assert!(!r2_status_marker(&format!("NXR2_{}_0__FIN","1".repeat(64))));
 assert_eq!(TRACE.lock().unwrap().len(),1);
 assert_eq!(echo(vec!["NXR2_1_0__FIN".into(),"extra".into()]),Ok(0));assert_eq!(TRACE.lock().unwrap().len(),1);
 FAILURE.store(true,std::sync::atomic::Ordering::Relaxed);
 assert_eq!(echo(vec!["NXR2_2_0__FIN".into()]),Err(6));assert_eq!(TRACE.lock().unwrap().len(),1);
}
'''
if __name__=='__main__':unittest.main()
