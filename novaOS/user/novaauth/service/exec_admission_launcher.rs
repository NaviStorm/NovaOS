//! Profil QEMU EXEC : attendre le registre récupéré avant la connexion du vrai nsh.
use super::*;
#[path="shell_launcher.rs"]
mod relay;
pub(super) fn launch()->rt::Result<u64> {
    let mut registry=[0;16];
    for id in [17,22,23] {
        let mut frame=[0;16];frame[0]=id;frame[4]=1;
        let right=syscall(proc::START,&mut frame)?;
        if id==22 {registry=frame;registry[0]=right;}
    }
    let ready=rt::receive_for(registry[1],15000)?;
    if ready.kind!=1 || ready.tag!=20 || ready.len!=1 || ready.payload[0]!=0 {
        return Err(rt::REFUSED);
    }
    let mut last=0;
    for id in [18,21] {
        let mut frame=[0;16];frame[0]=id;frame[4]=1;
        last=syscall(proc::START,&mut frame)?;
    }
    let message=b"NAEXEC registre pret puis connexion du vrai nsh";
    rt::value(rt::call(rt::DEBUG_WRITE,message.as_ptr() as u64,message.len() as u64,0))?;
    Ok(last)
}
pub(super) fn poll(right:&mut u64)->rt::Result<()> {relay::poll(right)}
