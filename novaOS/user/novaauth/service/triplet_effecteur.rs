//! Raccord privé de récupération, inclus exclusivement par le profil QEMU.
use super::*;
use novaos_identity::{Fault,recuperation_port::Port};
const TAG:u64=u64::from_le_bytes(*b"NATRX001");
pub(super) fn poll(session:&mut Session<'_, '_>,port:&mut Port,last:&mut u64)->rt::Result<()> {
    let channel=exec_files_lab::config(464);
    if channel==0 {return Ok(());}
    let msg=match rt::receive_for(channel,0) {
        Ok(m)=>m,
        Err(rt::WOULD_BLOCK)=>return Ok(()),
        Err(rt::DENIED|rt::CLOSED|rt::REVOKED)=>return Ok(()),
        Err(e)=>return Err(e)
    };
    let mut response=[0;128];
    let result=(|| {
        if msg.kind!=1 || msg.len!=128 || msg.tag<=*last || msg.tag<TAG ||
            rt::config(13)!=0x4e4133 {return Err(rt::DENIED);}
        *last=msg.tag;
        if !session.writable || session.failed.is_some() || session.pending.is_some() ||
            session.temporary.any_pending() {return Err(rt::REFUSED);}
        session.mount_volume()?;
        let fs=session.fs.as_mut().ok_or(rt::CLOSED)?;
        response=port.execute(fs,[7;16],&msg.payload).map_err(|e|match e {
            Fault::Storage(e)=>error(e),
            Fault::Uncertain=>w::UNCERTAIN,
            Fault::Limit=>rt::LIMIT,
            Fault::Denied=>rt::DENIED,
            Fault::BadInput=>rt::BAD_ARGUMENT,
            _=>rt::REFUSED
        })?;
        Ok(())
    })();
    if let Err(code)=result {response=[0;128];response[..8].copy_from_slice(&code.to_le_bytes());}
    // Rendez-vous borné : aucun dépôt asynchrone sur cette conversation duplex.
    // La perte de réponse ne répète pas execute. Le pair doit interroger l'issue.
    match rt::send_for(channel,msg.tag,&response,25) {
        Ok(())|Err(rt::DENIED|rt::CLOSED|rt::REVOKED|rt::TIMED_OUT)=>Ok(()),
        Err(e)=>Err(e)
    }
}
