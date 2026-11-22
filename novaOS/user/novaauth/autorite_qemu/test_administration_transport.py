"""Route de transport réelle extraite du profil, avec effecteur bloc témoin.
Ces tests hôtes vérifient les bornes et la propagation ; ils ne qualifient pas TLS.
"""
from pathlib import Path
import os
import runpy
import subprocess
import tempfile
import unittest

SERVICE=Path(__file__).resolve().parents[1]/'service'

class Transport(unittest.TestCase):
    def test_routes_bounds_bytes_and_native_failure(self):
        patch=(SERVICE/'transport-incarnation.patch').read_text()
        additions='\n'.join(line[1:] for line in patch.splitlines() if line.startswith('+') and not line.startswith('+++'))
        start=additions.index('    pub(crate) fn authority_cell(')
        # Le corps de cette méthode précède le contexte suivant du patch.
        body=additions[start:]
        level=0;end=None
        for at,c in enumerate(body):
            if c=='{':level+=1
            elif c=='}':
                level-=1
                if level==0:end=at+1;break
        body=body[:end]
        replacement=runpy.run_path(str(SERVICE/'administration_tls_trace.py'))['BULK']
        old='        block.transfer(sector, page * (4096 / size) as u64, write)'
        self.assertEqual(body.count(old),1)
        source=STUB+'\nimpl Device {\n'+body.replace(old,replacement)+'\n}\n'+TESTS
        root=Path(os.environ['NOVAOS_TMPDIR'])
        with tempfile.TemporaryDirectory(prefix='r2-transport-',dir=root) as directory:
            p=Path(directory);(p/'test.rs').write_text(source)
            rustc=Path('/Users/thierry/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustc')
            subprocess.run([str(rustc),'--edition=2024','--test',str(p/'test.rs'),'-o',str(p/'test')],check=True,capture_output=True)
            subprocess.run([str(p/'test'),'--nocapture'],check=True,capture_output=True)

STUB=r'''
use std::cell::{Cell,RefCell};
mod runtime {
 use std::sync::atomic::{AtomicU64,Ordering};
 pub const REFUSED:u64=12;pub const BAD_ARGUMENT:u64=3;pub const WOULD_BLOCK:u64=7;
 pub type Result<T>=core::result::Result<T,u64>;
 pub static MODE:AtomicU64=AtomicU64::new(0x4e4133);
 pub static CAP:AtomicU64=AtomicU64::new(91);
 pub fn config(n:usize)->u64 {match n {13=>MODE.load(Ordering::Relaxed),10=>CAP.load(Ordering::Relaxed),_=>panic!()}}
}
struct Geometry(usize);impl Geometry {fn sector_size(&self)->usize {self.0}}
#[derive(Default)]struct Block {calls:Vec<(&'static str,u64,u64,usize,bool)>,error:Option<u64>}
impl Block {
 fn transfer(&mut self,b:&mut [u8],lba:u64,write:bool)->runtime::Result<()> {
 self.calls.push(("sector",lba,0,b.len(),write));if let Some(e)=self.error {return Err(e)}
 if !write {b.fill(0x45)};Ok(())}
 fn read_page(&mut self,lba:u64,cap:u64,b:&mut [u8;4096])->runtime::Result<()> {
 self.calls.push(("page",lba,cap,4096,false));if let Some(e)=self.error {return Err(e)}b.fill(0x45);Ok(())}
 fn write_page(&mut self,lba:u64,cap:u64,b:&[u8;4096])->runtime::Result<()> {
 self.calls.push(("page",lba,cap,b.len(),true));assert!(b.iter().all(|b|*b==0x45));self.error.map_or(Ok(()),Err)}
}
struct Device {failed:Cell<bool>,count:u64,geometry:Geometry,session:RefCell<Block>}
'''
TESTS=r'''
#[test]fn routing() {
 use std::sync::atomic::Ordering;
 let mut d=Device{failed:Cell::new(false),count:514,geometry:Geometry(4096),session:RefCell::new(Block::default())};
 let mut page=[0;4096];assert_eq!(d.authority_cell(&mut page,false),Ok(()));assert_eq!(page,[0x45;4096]);
 assert_eq!(d.authority_cell(&mut page,true),Ok(()));
 assert_eq!(d.session.borrow().calls,[("page",513,91,4096,false),("page",512,91,4096,true)]);
 d.session.borrow_mut().error=Some(10);assert_eq!(d.authority_cell(&mut page,false),Err(10));
 d.session.borrow_mut().error=None;runtime::CAP.store(0,Ordering::Relaxed);
 assert_eq!(d.authority_cell(&mut page,false),Ok(()));assert_eq!(d.session.borrow().calls.last().unwrap().0,"sector");
 runtime::CAP.store(91,Ordering::Relaxed);d.geometry=Geometry(512);
 assert_eq!(d.authority_cell(&mut page[..512],false),Ok(()));
 assert_eq!(*d.session.borrow().calls.last().unwrap(),("sector",4104,0,512,false));
 let before=d.session.borrow().calls.len();
 assert_eq!(d.authority_cell(&mut page,false),Err(3));
 d.failed.set(true);assert_eq!(d.authority_cell(&mut page[..512],false),Err(12));d.failed.set(false);
 d.count=512;assert_eq!(d.authority_cell(&mut page[..512],false),Err(12));d.count=514;
 runtime::MODE.store(0,Ordering::Relaxed);assert_eq!(d.authority_cell(&mut page[..512],false),Err(12));
 assert_eq!(before,d.session.borrow().calls.len());
 runtime::MODE.store(0x4e4133,Ordering::Relaxed);
 let _lock=d.session.borrow_mut();assert_eq!(d.authority_cell(&mut page[..512],false),Err(7));
}
'''
