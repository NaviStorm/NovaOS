"""Contrôle natif du parseur et du verrou à usage unique ; appels noyau simulés.

Complément des preuves QEMU, jamais un substitut au refus natif dans l'invité.
"""
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT=Path(__file__).resolve().parents[4]
HARNESS=r'''extern crate alloc;
use std::sync::atomic::{AtomicU64,Ordering::SeqCst};
static CALLS:AtomicU64=AtomicU64::new(0);
static KEY:AtomicU64=AtomicU64::new(0);
mod rt {
    pub type Result<T>=core::result::Result<T,u64>;
    pub const DENIED:u64=5;pub const BAD_ARGUMENT:u64=3;
    pub struct Reply {pub status:u64,pub value:u64}
    pub fn call(selector:u64,variant:u64,key:u64,reserved:u64)->Reply {
        assert_eq!((selector,variant,reserved),(super::tty::SPAWN,super::tty::EXEC_TICKET,0));
        super::CALLS.fetch_add(1,super::SeqCst);super::KEY.store(key,super::SeqCst);
        Reply{status:5,value:0}
    }
}
mod tty {pub const OUTPUT:u64=1;pub const SPAWN:u64=99;pub const EXEC_TICKET:u64=123;}
mod console {pub fn write(_:u64,_:&str)->super::rt::Result<()> {Ok(())}}
#[path="@SOURCE@"] mod probe;
fn main() {
    let args:Vec<String>=std::env::args().collect();
    let result=probe::intercept(&args[1]);
    match args[2].as_str() {
        "valid"=>{assert_eq!(result,Ok(Some(5)));assert_eq!(CALLS.load(SeqCst),1);
            assert_eq!(KEY.load(SeqCst),args[3].parse::<u64>().unwrap());},
        "invalid"=>{assert_eq!(result,Err(3));assert_eq!(CALLS.load(SeqCst),0);},
        "ordinary"=>{assert_eq!(result,Ok(None));assert_eq!(CALLS.load(SeqCst),0);},
        _=>panic!("mode inconnu"),
    }
    let calls=CALLS.load(SeqCst);
    assert_eq!(probe::intercept("ticket-reference 42"),Ok(Some(5)));
    assert_eq!(CALLS.load(SeqCst),calls);
}
'''
class Probe(unittest.TestCase):
    def test_parser_and_single_use(self):
        with tempfile.TemporaryDirectory(dir=os.environ["NOVAOS_TMPDIR"]) as tmp:
            tmp=Path(tmp);source=tmp/"probe.rs";binary=tmp/"probe"
            source.write_text(HARNESS.replace("@SOURCE@",str(ROOT/"components/shell/exec_reference_lab.rs")))
            subprocess.run(["rustup","run","stable","rustc","--edition=2024",str(source),"-o",str(binary)],check=True)
            cases=[("ticket-reference 0","valid","0"),("ticket-reference 18446744073709551615","valid","18446744073709551615"),
                ("ticket-reference 00042","valid","42"),("ticket-reference","invalid",""),
                ("ticket-reference -1","invalid",""),("ticket-reference +1","invalid",""),
                ("ticket-reference 18446744073709551616","invalid",""),
                ("ticket-reference 1; true","invalid",""),("ticket-reference 1 2","invalid",""),
                ("session reauthentifier","ordinary",""),("true","ordinary",""),("","ordinary","")]
            for line,mode,key in cases:
                with self.subTest(line=line):subprocess.run([str(binary),line,mode,key],check=True)
if __name__=="__main__":unittest.main()
