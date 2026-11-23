"""Réutiliser les canaux VFS et NovaAuth existants ; aucune racine ajoutée."""
from pathlib import Path

def install(root):
 root=Path(root)
 def edit(path,old,new):
  p=root/path;s=p.read_text()
  if s.count(old)!=1:raise ValueError('ancrage IO selectif '+path+': '+old[:60])
  p.write_text(s.replace(old,new))
 edit('novaOS/kernel/src/sched/process/ipc/service/shell_auth_lab.rs',
      '    config(s, verifier, 11+offset, &shell);',
      '''    config(s, verifier, 11+offset, &shell);
    if !second {
        use sha2::{Digest,Sha256};
        let image=Sha256::digest(include_bytes!(concat!(env!("OUT_DIR"),"/shell.elf")));
        let words:[u64;4]=core::array::from_fn(|i|u64::from_le_bytes(image[8*i..8*i+8].try_into().unwrap()));
        config(s, verifier, 44, &words);
        config(s, m.fs, 472, &shell);
    }''')
 path='novaOS/user/novafs/files/auth_directory.rs'
 for name in ('Holder','Lease'):
  edit(path,'struct '+name+' {','pub(super) struct '+name+' {')
 for field in ('subject: Principal','session: SessionId','port: u64','holder: Holder','key: u64'):
  # Ces champs appartiennent à plusieurs structures : visibilité locale au parent,
  # uniquement dans l'export ; aucune API externe.
  p=root/path;s=p.read_text().replace('    '+field+',','    pub(super) '+field+',');p.write_text(s)
 edit(path,'    parent: u64,','    object: u64,\n    parent: u64,')
 edit(path,'            parent,\n            rights,','            object:0,\n            parent,\n            rights,')
 edit(path,'    leases: [Option<Lease>; 12],','    pub(super) leases: [Option<Lease>; 12],')
 edit(path,'    fn check(&self,','    pub(super) fn check(&self,')
 edit(path,'let belongs = |h: Holder| h.port == rx && sid.is_none_or(|s| s == h.session);',
      'let belongs = |h: Holder| (h.port == rx || (lane==0 && h.port==rt::config(4))) && sid.is_none_or(|s| s == h.session);')

 p=root/'novaOS/user/novaauth/service/membership_vfs_body.rs';s=p.read_text()
 for n in ('membership_grant','membership_permission','membership_admit'):s=s.replace('    fn '+n+'(', '    pub(super) fn '+n+'(')
 p.write_text(s)
 # Champ object uniquement dans cet export.
 p=root/path;s=p.read_text().replace('    object: u64,','    pub(super) object: u64,');p.write_text(s)
 path='novaOS/user/novafs/files.rs'
 edit(path,'mod recuperation_vfs;','mod recuperation_vfs;\n#[path="../novaauth/service/membership_io_vfs.rs"]\nmod membership_io;')
 edit(path,"struct Session<'a, 'b> {","struct Session<'a, 'b> {\n    membership_io: membership_io::Exchange,")
 edit(path,'    let mut s = Session {','    let mut s = Session {\n        membership_io: membership_io::Exchange::default(),')
 edit(path,'        let mut grant = None;', '''        if m.kind==1 && m.len==128 && &m.payload[..8]==b"NXMGDAT1" {
            if let Err(code)=s.membership_io.enqueue(i,&mut last[i],m.tag,m.payload) {
                let mut denied=[0;128];denied[..8].copy_from_slice(b"NXMGOUT1");denied[8..16].copy_from_slice(&code.to_le_bytes());
                match rt::send_frame(ports[i].1,m.tag,&denied,0,false) {
                    Ok(_) | Err(rt::CLOSED|rt::REVOKED|rt::WOULD_BLOCK)=>{},Err(e)=>return Err(e),
                }
            }
            continue;
        }
        let mut grant = None;''')
 path='novaOS/user/novaauth/service/triplet_effecteur.rs'
 edit(path,'        if matches!(&msg.payload[..8],b"NXMGVFS1"|b"NXMGEND1") {','''        if matches!(&msg.payload[..8],b"NXMGCTX2"|b"NXMGGO01"|b"NXMGDEN1"|b"NXMGCAN1"|b"NXMGRES1"|b"NXMGREL1") {
            response=membership_io::control(session,&msg.payload)?;return Ok(());
        }
        if matches!(&msg.payload[..8],b"NXMGVFS1"|b"NXMGEND1") {''')
 # Réserver la 16e trace au reçu terminal : vérification cryptographique inchangée.
 edit('novaOS/user/novaauth/service/src/bin/common/tete.rs',
      '    common::marker(b"NAUTH tete signature et portee verifiees");','')
 path='novaOS/user/novaauth/service/src/bin/common/recovery_view.rs'
 edit(path,'    let mut sequence=0;','    let mut sequence=0;\n    let mut membership_io=crate::membership_io::Io::default();')
 edit(path,'            sequence=m.tag;','''            sequence=m.tag;
            if &m.payload[..8]==b"NXRTACK2" && administration.selective_in_progress() && membership_io.pending() {
                // RESERVE est déjà durable ; aucune publication/ACK de retrait
                // ne précède la consultation et l'issue individuelle du VFS.
                membership_io.reconcile(&mut recovered.storage)?;
                common::marker(b"NARM Q2 drainage VFS apres reservation avant fermeture");
            }
            if crate::membership_io::Io::accepts(&m.payload) {
                return membership_io.request(&state,&mut recovered.storage,&m.payload,&mut answer);
            }''')
 edit('novaOS/user/novaauth/service/src/bin/common/administration_channel.rs','fn port() -> rt::Result<u64> {','pub(super) fn port() -> rt::Result<u64> {')
 path='novaOS/user/novaauth/service/src/bin/common/shell_verifier.rs'
 edit(path,'            match &m.payload[..8] {','''            match &m.payload[..8] {
                b"NSHMGRC1" => {
                    if login.slot!=0 || m.payload[40..]!=[0;88]{return Err(rt::DENIED);}
                    let sid=m.payload[8..40].try_into().unwrap();login.inspected_binding(sid)?;
                    let mut q=[0;128];q[..8].copy_from_slice(b"NXMGREC1");
                    login.store_seq=login.store_seq.checked_add(1).ok_or(rt::LIMIT)?;
                    let port=administrative::port()?;let a=rpc(port,port,login.store_seq,&q)?;
                    if number(&a)!=0{return Err(number(&a));}reply[8..16].copy_from_slice(&a[8..16]);
                }
                b"NSHMGVF1"|b"NSHMGVH1" => {
                    if login.slot!=0{return Err(rt::DENIED);}
                    let sid=m.payload[8..40].try_into().unwrap();login.inspected_binding(sid)?;
                    let reference=login.live.filter(|(s,_)|s==&sid).ok_or(rt::REVOKED)?.1;
                    let mut ctx=[0;128];ctx[..8].copy_from_slice(b"NXMGCTX1");ctx[8..24].copy_from_slice(&reference);ctx[24..56].copy_from_slice(&sid);ctx[56..88].copy_from_slice(&login.route.client);
                    for i in 0..4 {let v=unsafe {(&*(0x401000 as *const core::sync::atomic::AtomicU64).add(44+i)).load(core::sync::atomic::Ordering::Acquire)};ctx[88+8*i..96+8*i].copy_from_slice(&v.to_le_bytes());}
                    let mut q=m.payload;q[..8].copy_from_slice(if &m.payload[..8]==b"NSHMGVH1"{b"NXMGHLD1"}else{b"NXMGOP01"});
                    for request in [&ctx,&q] {
                        login.store_seq=login.store_seq.checked_add(1).ok_or(rt::LIMIT)?;
                        let port=administrative::port()?;let a=rpc(port,port,login.store_seq,request)?;
                        if number(&a)!=0{return Err(number(&a));}
                        reply[8..16].copy_from_slice(&a[8..16]);
                    }
                }''')
 path='novaOS/user/storage/fileapi.rs'
 edit(path,'    pub fn request(&mut self, q: w::Frame)', '''    pub fn membership_begin(&mut self,q:&w::Frame)->rt::Result<u64> {
        if self.disconnected || &q[..8]!=b"NXMGDAT1" {return Err(rt::DENIED);}
        self.tag=self.tag.checked_add(1).ok_or(rt::LIMIT)?;
        rt::send_for(self.tx,self.tag,q,5000)?;Ok(self.tag)
    }
    pub fn membership_end(&mut self,tag:u64)->rt::Result<w::Frame> {
        let r=rt::receive_for(self.rx,5000)?;
        if r.kind!=1 || r.tag!=tag || r.len!=128 || &r.payload[..8]!=b"NXMGOUT1"{return Err(rt::REFUSED);}
        Ok(r.payload)
    }
    pub fn request(&mut self, q: w::Frame)''')
 path='components/shell/session_auth_lab.rs'
 edit(path,'    tag: u64,','    tag: u64,\n    membership_keys:[u64;3],')
 with (root/path).open('a') as f:f.write((root/'novaOS/user/novaauth/service/membership_shell.rs').read_text())
 edit('components/shell/script.rs','            "ticket-test" => self.login.ticket_probe(&argv)?,','            "ticket-test" => self.login.ticket_probe(&argv)?,\n            "membership-test" => self.login.membership_probe(self.fs,&argv)?,')
