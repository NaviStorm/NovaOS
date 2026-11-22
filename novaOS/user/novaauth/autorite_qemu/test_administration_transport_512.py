"""Bornes réelles du transport R2/512, sans simuler une décision NovaAuth."""
import os,runpy,subprocess,tempfile,unittest
from pathlib import Path
from test_administration_transport import SERVICE,STUB
class Page512(unittest.TestCase):
 def test_actual_generated_body(self):
  patch=(SERVICE/'transport-incarnation.patch').read_text()
  body='\n'.join(x[1:] for x in patch.splitlines() if x.startswith('+') and not x.startswith('+++'))
  body=body[body.index('    pub(crate) fn authority_cell('):];level=0
  for i,c in enumerate(body):
   if c=='{':level+=1
   elif c=='}':
    level-=1
    if level==0:body=body[:i+1];break
  bulk=runpy.run_path(str(SERVICE/'administration_tls_trace.py'))['BULK']
  added=runpy.run_path(str(SERVICE/'administration_transport_512_profile.py'))['PAGE512']
  # Exercer sur Darwin ARM la même branche qui sera compilée pour x86 invité.
  added=added.replace('        #[cfg(target_arch="x86_64")]\n','')
  body=body.replace('        block.transfer(sector, page * (4096 / size) as u64, write)',bulk.replace('        if size==4096',added+'        if size==4096'))
  stub=STUB.replace('error:Option<u64>','error:Option<u64>,no_page:bool,bad_padding:bool')
  stub=stub.replace('impl Block {','impl Block { fn r2_has_page(&self)->bool {!self.no_page}')
  stub=stub.replace('b.fill(0x45);Ok(())}', 'b.fill(0);b[..512].fill(0x45);if self.bad_padding {b[4095]=1};Ok(())}')
  stub=stub.replace('assert!(b.iter().all(|b|*b==0x45));','assert!(b[..512].iter().all(|b|*b==0x45));assert!(b[512..].iter().all(|b|*b==0));')
  source=stub+'\nimpl Device {\n'+body+'\n}\n'+TESTS
  with tempfile.TemporaryDirectory(prefix='r2-page512-',dir=os.environ['NOVAOS_TMPDIR']) as d:
   p=Path(d);(p/'test.rs').write_text(source)
   subprocess.run(['/Users/thierry/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustc','--edition=2024','--test',str(p/'test.rs'),'-o',str(p/'test')],check=True,capture_output=True)
   subprocess.run([str(p/'test'),'--nocapture'],check=True,capture_output=True)
TESTS=r'''
#[test]fn page512() {
 use std::sync::atomic::Ordering;
 let mut d=Device{failed:Cell::new(false),count:514,geometry:Geometry(512),session:RefCell::new(Block::default())};
 let mut b=[0;512];assert_eq!(d.authority_cell(&mut b,false),Ok(()));assert_eq!(b,[0x45;512]);
 assert_eq!(d.authority_cell(&mut b,true),Ok(()));
 assert_eq!(d.session.borrow().calls,[("page",4104,91,4096,false),("page",4096,91,4096,true)]);
 d.session.borrow_mut().bad_padding=true;b.fill(0x73);
 assert_eq!(d.authority_cell(&mut b,false),Err(3));assert_eq!(b,[0x73;512]);
 d.session.borrow_mut().bad_padding=false;d.session.borrow_mut().error=Some(10);
 assert_eq!(d.authority_cell(&mut b,false),Err(10));assert_eq!(b,[0x73;512]);
 d.session.borrow_mut().error=None;d.session.borrow_mut().no_page=true;
 assert_eq!(d.authority_cell(&mut b,false),Ok(()));
 assert_eq!(*d.session.borrow().calls.last().unwrap(),("sector",4104,0,512,false));
 d.session.borrow_mut().no_page=false;runtime::CAP.store(0,Ordering::Relaxed);
 assert_eq!(d.authority_cell(&mut b,true),Ok(()));assert_eq!(d.session.borrow().calls.last().unwrap().0,"sector");
 runtime::CAP.store(91,Ordering::Relaxed);
 let before=d.session.borrow().calls.len();
 assert_eq!(d.authority_cell(&mut b[..511],false),Err(3));
 d.failed.set(true);assert_eq!(d.authority_cell(&mut b,false),Err(12));d.failed.set(false);
 d.count=512;assert_eq!(d.authority_cell(&mut b,false),Err(12));d.count=514;
 runtime::MODE.store(0,Ordering::Relaxed);assert_eq!(d.authority_cell(&mut b,false),Err(12));
 assert_eq!(before,d.session.borrow().calls.len());runtime::MODE.store(0x4e4133,Ordering::Relaxed);
 let _lock=d.session.borrow_mut();assert_eq!(d.authority_cell(&mut b,false),Err(7));
}
'''
