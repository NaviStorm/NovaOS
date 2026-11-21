//! Publication de politique typée, uniquement sur la route privée du registre.
//! Le signataire ne voit que les empreintes publiques, jamais le triplet clair.
use crate::{canal,incarnation,recovery_rpc as rpc,runtime as rt};
use ed25519_dalek::{Signer,SigningKey};
use embedded_tls::TlsError;
use novaos_recovery::framing::{Header,HEADER,MAX};
pub const CHANGE:&[u8]=b"NovaAuth/QEMU/politique-compare-publie/1\0";
pub fn serve(key:&SigningKey,port:u64,id:&[u8;32],first:rt::ReceiveV1)->rt::Result<()> {
    let mut request=[0;MAX];
    let h=rpc::receive(port,first,&mut request,||Ok(2000))?;
    let n=usize::from(h.size);
    if h.sequence!=1 || !((matches!((h.operation,n),(30,224)|(31,112)|(32,482)|(34,128))) || (h.operation==33 && (117..=1140).contains(&n))) ||
       request[HEADER..HEADER+16]!=incarnation::SERVER ||
       request[HEADER+16..HEADER+32]!=incarnation::DOMAIN ||
       request[HEADER+32..HEADER+64]!=*id {return Err(rt::DENIED);}
    let size=if h.operation==30 {
        let p=&request[HEADER+64..HEADER+n];
        if p[..16]!=incarnation::SERVER || p[16..32]!=incarnation::DOMAIN ||
           p[32..64]!=*id || p[112..128]==[0;16] || p[128..160]==[0;32] {
            return Err(rt::DENIED);
        }
        let mut signed=[0;CHANGE.len()+160];
        signed[..CHANGE.len()].copy_from_slice(CHANGE);signed[CHANGE.len()..].copy_from_slice(p);
        let signature=key.sign(&signed).to_bytes();
        request[HEADER+n..HEADER+n+64].copy_from_slice(&signature);
        n+64
    } else if h.operation==32 {
        let m=&request[HEADER+64..HEADER+n];
        if m[..8]!=*b"NABACK01" || m[14..30]!=incarnation::SERVER ||
            m[30..46]!=incarnation::DOMAIN {return Err(rt::DENIED);}
        const REGISTER:&[u8]=b"NovaAuth/recovery/register-backup/v1\0";
        let mut signed=[0;REGISTER.len()+32+418];
        signed[..REGISTER.len()].copy_from_slice(REGISTER);
        signed[REGISTER.len()..REGISTER.len()+32].copy_from_slice(id);
        signed[REGISTER.len()+32..].copy_from_slice(m);
        let signature=key.sign(&signed).to_bytes();
        request[HEADER+n..HEADER+n+64].copy_from_slice(&signature);n+64
    } else {n};
    request[..HEADER].copy_from_slice(&Header {size:size as u16,..h}.encode().map_err(|_|rt::BAD_ARGUMENT)?);
    if h.operation==33 && request[HEADER+96..HEADER+100]==[0;4] {
        crate::common::marker(b"NAUTH capture avant");
    }
    let mut response=[0;MAX];
    let mut response_size=0;
    let result=canal::dialogue(key,|tls,until| {
        if tls.write(&request[..HEADER+size])?!=HEADER+size {return Err(TlsError::IoError);}
        tls.flush()?;
        let mut count=0;
        while count<HEADER {
            let n=tls.read(&mut response[count..HEADER])?;
            if n==0 {return Err(TlsError::ConnectionClosed);}count+=n;
        }
        let reply=Header::parse(&response[..HEADER]).map_err(|_|TlsError::InvalidApplicationData)?;
        if reply.sequence!=1 || reply.operation!=(h.operation|0x8000) {return Err(TlsError::InvalidApplicationData);}
        let size=usize::from(reply.size);
        while count<HEADER+size {
            let n=tls.read(&mut response[count..HEADER+size])?;
            if n==0 {return Err(TlsError::ConnectionClosed);}count+=n;
        }
        canal::guard(until).map_err(TlsError::Io)?;
        response_size=size;
        Ok(())
    });
    let size=match result {
        Ok(())=>response_size,
        Err(_)=>{response[HEADER..HEADER+8].copy_from_slice(&rt::REFUSED.to_le_bytes());8}
    };
    if matches!(h.operation,32|34) {crate::common::marker(b"NAUTH capture apres");}
    rpc::send(port,Header {sequence:1,operation:h.operation|0x8000,size:size as u16},
        &response[HEADER..HEADER+size],||Ok(2000))
}
