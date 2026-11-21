//! Supervision du témoin VFS ; aucune clé privée ni capacité de lecture.
use super::*;
fn start(id:u64)->rt::Result<Frame> {
    let mut w=[0;16];w[0]=id;w[4]=1;let right=syscall(proc::START,&mut w)?;w[0]=right;Ok(w)
}
fn log(s:&[u8]) {rt::call(rt::DEBUG_WRITE,s.as_ptr() as u64,s.len() as u64,0);}
fn command(p:&Frame,tag:u64,op:u8,object:u64)->rt::Result<[u8;128]> {
    let mut q=[0;128];q[0]=op;q[8..16].copy_from_slice(&object.to_le_bytes());
    rt::send_for(p[2],tag,&q,5000)?;
    let r=rt::receive_for(p[1],15000)?;
    if r.kind!=1 || r.len!=128 || r.tag!=tag {return Err(rt::REFUSED);}
    Ok(r.payload)
}
fn report(label:&[u8],reply:&[u8;128])->rt::Result<()> {
    // Seulement les statuts, identités publiques et huit octets synthétiques.
    let mut line=[0;200];line[..label.len()].copy_from_slice(label);
    const HEX:&[u8;16]=b"0123456789abcdef";
    let mut pos=label.len();
    let n=if label.windows(16).any(|x|x==b"authentification") {64} else {24};
    for b in &reply[..n] {line[pos]=HEX[(b>>4) as usize];line[pos+1]=HEX[(b&15) as usize];pos+=2;}
    log(&line[..pos]);Ok(())
}
pub(super) fn launch()->rt::Result<u64> {
    start(17)?;start(20)?;let registre=start(22)?;start(23)?;
    let alice=start(18)?;let bob=start(19)?;
    let ready=rt::receive_for(registre[1],15000)?;
    if ready.kind!=1 || ready.tag!=20 || ready.len!=1 {return Err(rt::REFUSED);}
    let ids=include_bytes!(env!("NOVA_VFS_OBJECTS"));
    let p=u64::from_le_bytes(ids[..8].try_into().unwrap());
    let l=u64::from_le_bytes(ids[8..16].try_into().unwrap());
    report(b"NARV Alice authentification ",&command(&alice,1,1,0)?)?;
    if ready.payload[0]!=0 {
        report(b"NARV Alice P quarantaine ",&command(&alice,2,2,p)?)?;
        log(b"NARV quarantaine terminee");return Ok(0);
    }
    report(b"NARV Bob authentification ",&command(&bob,1,1,0)?)?;
    report(b"NARV Alice P ",&command(&alice,2,2,p)?)?;
    report(b"NARV Bob P ",&command(&bob,2,2,p)?)?;
    report(b"NARV Alice L ",&command(&alice,3,2,l)?)?;
    report(b"NARV Alice fermeture ",&command(&alice,4,3,0)?)?;
    report(b"NARV Alice nouvelle authentification ",&command(&alice,5,1,0)?)?;
    report(b"NARV Alice nouvelle lecture L avant P ",&command(&alice,6,2,l)?)?;
    let last_private=command(&alice,7,2,p)?;
    report(b"NARV Alice nouvelle lecture P ",&last_private)?;
    report(b"NARV Alice nouvelle lecture L ",&command(&alice,8,2,l)?)?;
    if last_private[..8]==[0;8] {
        // Retrait explicitement demandé par Alice dans ce seul scénario QEMU.
        let result=command(&alice,9,4,p)?;
        report(b"NARV Alice retrait ",&result)?;
        if result[..8]==22u64.to_le_bytes() {
            report(b"NARV Alice reconciliation ",&command(&alice,10,5,0)?)?;
        }
        report(b"NARV Alice authentification apres retrait ",&command(&alice,11,1,0)?)?;
        report(b"NARV Alice P apres retrait ",&command(&alice,12,2,p)?)?;
        report(b"NARV Alice L apres retrait ",&command(&alice,13,2,l)?)?;
        let mut capture=[0;128];capture[..8].copy_from_slice(b"NARCAP01");
        rt::send_for(registre[2],21,&capture,5000)?;
        let reply=rt::receive_for(registre[1],15000)?;
        if reply.kind!=1 || reply.tag!=21 || reply.len!=128 {return Err(rt::REFUSED);}
        report(b"NARV Alice sauvegarde ",&reply.payload)?;
    }
    log(b"NARV parcours termine");Ok(0)
}
pub(super) fn poll(_right:&mut u64)->rt::Result<()> {Ok(())}
