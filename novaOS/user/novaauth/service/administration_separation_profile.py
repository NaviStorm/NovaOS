"""S3/S5 : références réellement détenues par Alice, sans délégation de droit.

Les messages de sonde transportent des nombres opaques et identifiants de session
sur les routes QEMU existantes. Aucun IPC_GIVE/DUP, nouvelle racine ou décision
administrative n'est ajouté. Les appels testés passent par les contrôles ordinaires.
"""
from pathlib import Path
import runpy


def install(root):
    root=Path(root)
    def edit(name,old,new):
        p=root/name;s=p.read_text()
        if s.count(old)!=1:raise ValueError('ancrage séparation absent/ambigu : '+name)
        p.write_text(s.replace(old,new))
    for name in ('administration_tls_trace.py','administration_transport_512_profile.py','administration_status_profile.py'):
        runpy.run_path(str(root/'novaOS/user/novaauth/service'/name))['install'](root)
    login='components/shell/session_auth_lab.rs'
    edit(login,'    fn request(&mut self, op:', '    pub(super) fn request(&mut self, op:')
    edit(login,'    pub(super) fn ticket_probe(&mut self, args: &[&str]) -> rt::Result<u64> {',
         ALICE+'\n    pub(super) fn ticket_probe(&mut self, args: &[&str]) -> rt::Result<u64> {\n        if let ["separation", name, rest @ ..]=args {return self.separation_probe(name,rest);}')
    p=root/'novaOS/user/novaauth/service/administration_bob_shell.rs'
    p.write_text(BOB)
    verifier='novaOS/user/novaauth/service/src/bin/common/shell_verifier.rs'
    edit(verifier,'    let mut dead = [false;2];','    let mut dead = [false;2];\n    let mut reference_nonce:Option<[u8;16]>=None;\n    let mut references_sent=false;')
    edit(verifier,'            match &m.payload[..8] {','            match &m.payload[..8] {'+VERIFIER)

ALICE=r'''
    fn separation_probe(&mut self,name:&str,args:&[&str])->rt::Result<u64> {
        let key=self.ticket(name,args,5000,1,0)?;
        let result=(|| {
            let mut data=[0;48];data[..32].copy_from_slice(&self.session.ok_or(rt::DENIED)?);
            data[32..40].copy_from_slice(&key.to_le_bytes());
            data[40..48].copy_from_slice(&rt::config(16).to_le_bytes());
            let ack=self.request(b"NXR2REFQ",&data)?;
            let nonce=&ack[8..24];
            if nonce==[0;16] {return Err(rt::DENIED);}
            let done=rt::receive_for(rt::config(17),1000)?;
            if done.kind!=1 || done.tag!=0x4e58523252454644 || done.len!=128
                || &done.payload[..8]!=b"NXR2REFD" || &done.payload[8..24]!=nonce
                || done.payload[24..]!=[0;104] {return Err(rt::DENIED);}
            let launch=rt::call(super::tty::SPAWN,super::tty::EXEC_TICKET,key,0);
            if launch.status!=rt::OK {return Err(launch.status);}
            let (pid,right)=Self::receipt(key)?;
            let drained=self.drain(&[pid],None);
            let (fault,code)=nova_cli::process::wait(right)?;drained?;
            if fault || code!=0 {return Err(rt::REFUSED);}
            super::console::write(super::tty::OUTPUT,"NAR2 Alice ticket intact lancement legitime\n")?;
            Ok(0)
        })();
        let _=rt::call(rt::IPC_CLOSE,key,0,0);result
    }
'''

BOB=r'''//! Sonde QEMU : seules les références numériques d'Alice sont présentées.
use super::{rt,session_auth_lab};
fn mark(bytes:&[u8])->rt::Result<()> {rt::value(rt::call(rt::DEBUG_WRITE,bytes.as_ptr() as u64,bytes.len() as u64,0)).map(|_|())}
pub(super) fn session()->rt::Result<()> {
    while rt::config(16)==0 || rt::config(17)==0 {
        match rt::receive_for(rt::config(1),2) {Err(rt::TIMED_OUT|rt::WOULD_BLOCK)=>(),_=>return Err(rt::CANCELLED)}
    }
    let mut login=session_auth_lab::Login::default();login.authenticate()?;
    mark(b"NAR2 Bob references pret")?;
    let event=rt::receive_for(rt::config(17),15000)?;
    if event.kind!=1 || event.tag!=0x4e58523252454653 || event.len!=128
        || &event.payload[..8]!=b"NXR2REF1" || event.payload[104..]!=[0;24]
        || event.payload[8..40]==[0;32] || event.payload[88..104]==[0;16] {return Err(rt::DENIED);}
    let sid=&event.payload[40..72];
    let ticket=u64::from_le_bytes(event.payload[72..80].try_into().unwrap());
    let channel=u64::from_le_bytes(event.payload[80..88].try_into().unwrap());
    if ticket==0 || channel==0 || sid==[0;32] {return Err(rt::DENIED);}
    let reference=rt::call(super::tty::SPAWN,super::tty::EXEC_TICKET,ticket,0).status;
    if reference!=rt::DENIED {return Err(rt::REFUSED);}
    mark(b"NAR2 S3 ticket Alice statut=5 aucun enfant")?;
    if rt::send_for(channel,1,&[0;128],25).err()!=Some(rt::DENIED) {return Err(rt::REFUSED);}
    mark(b"NAR2 S3 canal Alice statut=5")?;
    if login.request(b"NSHLIVE1",sid).err()!=Some(rt::DENIED) {return Err(rt::REFUSED);}
    mark(b"NAR2 S3 session Alice statut=5")?;
    let mut substituted=[0;120];substituted[..32].copy_from_slice(sid);
    if login.request(b"NSHEXBG1",&substituted).err()!=Some(rt::DENIED) {return Err(rt::REFUSED);}
    mark(b"NAR2 S5 admission session Alice statut=5")?;
    // Les refus ne doivent ni fermer ni remplacer la session légitime de Bob.
    login.authenticate()?;
    mark(b"NAR2 Bob session propre preservee")?;
    login.request(b"NXR2REFD",&event.payload[88..104])?;
    login.close()?;mark(b"NAR2 Bob fermeture acquittee")?;
    loop {match rt::receive_for(rt::config(1),25) {
        Err(rt::TIMED_OUT|rt::WOULD_BLOCK)=>(),
        Ok(m) if m.kind==1 && m.len==1 && m.payload[0]==0=>return Ok(()),
        _=>return Err(rt::DENIED),
    }}
}
'''

VERIFIER=r'''
                b"NXR2REFQ" if login.slot==0 && !references_sent
                    && m.payload[56..].iter().all(|b|*b==0)=> {
                    let sid=m.payload[8..40].try_into().unwrap();login.inspect(sid)?;
                    let peer_sid=peer.live.ok_or(rt::DENIED)?.0;peer.store_seq=login.store_seq;peer.relay_seq=login.relay_seq;
                    let inspected=peer.inspect(peer_sid);
                    login.store_seq=peer.store_seq;login.relay_seq=peer.relay_seq;
                    inspected?;
                    let nonce:[u8;16]=random()?[..16].try_into().unwrap();
                    let mut event=[0;128];event[..8].copy_from_slice(b"NXR2REF1");
                    event[8..40].copy_from_slice(&login.route.client);
                    event[40..72].copy_from_slice(&sid);
                    event[72..88].copy_from_slice(&m.payload[40..56]);
                    event[88..104].copy_from_slice(&nonce);
                    rt::send_for(peer.port(16),0x4e58523252454653,&event,25)?;
                    reference_nonce=Some(nonce);references_sent=true;
                    reply[8..24].copy_from_slice(&nonce);
                }
                b"NXR2REFD" if login.slot==1 && m.payload[24..].iter().all(|b|*b==0)=> {
                    let nonce:[u8;16]=m.payload[8..24].try_into().unwrap();
                    if reference_nonce!=Some(nonce) {return Err(rt::DENIED);}
                    let sid=login.live.ok_or(rt::DENIED)?.0;login.inspect(sid)?;
                    let peer_sid=peer.live.ok_or(rt::DENIED)?.0;peer.store_seq=login.store_seq;peer.relay_seq=login.relay_seq;
                    let inspected=peer.inspect(peer_sid);
                    login.store_seq=peer.store_seq;login.relay_seq=peer.relay_seq;
                    inspected?;
                    let mut done=[0;128];done[..8].copy_from_slice(b"NXR2REFD");
                    done[8..24].copy_from_slice(&nonce);
                    rt::send_for(peer.port(16),0x4e58523252454644,&done,25)?;
                    reference_nonce=None;
                }
'''
