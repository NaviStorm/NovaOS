//! Liaison décisionnelle 17 -> R -> autorité fraîche. Aucun octet de fichier.
use crate::{common::*,runtime as rt};
pub fn attest(sequence:u64,reference:[u8;16],head:[u8;32],object:u64)->rt::Result<[u8;32]> {
    let port=unsafe {(&*(0x401000 as *const core::sync::atomic::AtomicU64).add(21))
        .load(core::sync::atomic::Ordering::Acquire)};
    if port==0 || object==0 {return Err(rt::DENIED);}
    let mut q=[0;128];q[..8].copy_from_slice(b"NA5FOLD1");
    q[8..24].copy_from_slice(&reference);q[24..32].copy_from_slice(&object.to_le_bytes());
    let answer=rpc(port,port,sequence,&q)?;
    if number(&answer)!=rt::OK || answer[40..72]!=head || answer[72..]!=[0;56] {
        return Err(rt::DENIED);
    }
    Ok(answer[8..40].try_into().unwrap())
}

pub fn mutate(sequence:u64,reference:[u8;16],object:u64,operation:[u8;16],query:bool)->rt::Result<()> {
    let port=unsafe {(&*(0x401000 as *const core::sync::atomic::AtomicU64).add(21))
        .load(core::sync::atomic::Ordering::Acquire)};
    if port==0 || operation==[0;16] {return Err(rt::DENIED);}
    let mut q=[0;128];q[..8].copy_from_slice(if query {b"NA5ISSU1"} else {b"NA5DROP1"});
    q[8..24].copy_from_slice(&reference);q[24..32].copy_from_slice(&object.to_le_bytes());
    q[32..48].copy_from_slice(&operation);
    let r=rpc(port,port,sequence,&q).map_err(|_|22u64)?;
    match number(&r) {0=>Ok(()),status=>Err(status)}
}
