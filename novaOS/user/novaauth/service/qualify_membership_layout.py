"""Mesure du contrôle additionnel ; hors piles TLS et allocations du moteur P9."""
from pathlib import Path
import runpy,re

def prepare(source,destination):
 source=Path(source);destination=Path(destination)
 runpy.run_path(str(source/'novaOS/user/novaauth/service/qualify_membership_transaction.py'))['prepare'](source,destination)
 io=(source/'novaOS/user/novaauth/service/src/bin/common/membership_io.rs').read_text()
 vfs=(source/'novaOS/user/novaauth/service/membership_io_vfs.rs').read_text()
 io=re.search(r'pub struct Io \{[^}]+\}',io).group()
 vfs=re.search(r'pub\(super\) struct Exchange \{[^}]+\}',vfs).group().replace('pub(super)','pub').replace('Frame','[u8;128]')
 with(destination/'src/lib.rs').open('a')as f:
  f.write('\n'+io+'\n'+vfs+'\n'+r"""
#[test] fn global_incremental_control_layout() {
 let pending=core::mem::size_of::<Option<coordinator::Selective>>();
 let complete=core::mem::size_of::<Option<([u8;16],novaos_recovery::membership::Binding)>>();
 let io=core::mem::size_of::<Io>();let exchange=core::mem::size_of::<Exchange>();
 // Douze baux VFS reçoivent chacun l'objet u64 ; trois handles nsh u64.
 let objects=12*core::mem::size_of::<u64>();let handles=3*core::mem::size_of::<u64>();
 let inherited=2960+48; // Option<GroupSource> : barrière précédente
let total=inherited+pending+complete+io+exchange+objects+handles;
 println!("inherited={} pending={} complete={} io={} exchange={} objects={} handles={} subtotal={}",inherited,pending,complete,io,exchange,objects,handles,total);
 assert!(total<=4096);
}
""")
if __name__=='__main__':
 import argparse
 p=argparse.ArgumentParser();p.add_argument('source',type=Path);p.add_argument('destination',type=Path);a=p.parse_args();prepare(a.source,a.destination)
