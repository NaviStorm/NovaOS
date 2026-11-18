//! Signataire de qualification : messages publics du seul registre désigné.
use crate::{canal,incarnation,recovery_rpc as rpc,runtime as rt};
use ed25519_dalek::{Signer,SigningKey};
use embedded_tls::TlsError;
use novaos_recovery::{exec_admission as exec,framing::{Header,HEADER,MAX}};

pub fn serve(key:&SigningKey,port:u64,id:&[u8;32],first:rt::ReceiveV1)->rt::Result<()> {
    let mut request=[0;MAX];let h=rpc::receive(port,first,&mut request,||Ok(2000))?;
    let n=usize::from(h.size);
    if h.sequence!=1 || !matches!((h.operation,n),(40,320)|(41,353)|(42,352)) ||
        request[HEADER..HEADER+16]!=incarnation::SERVER ||
        request[HEADER+16..HEADER+32]!=incarnation::DOMAIN || request[HEADER+32..HEADER+64]!=*id {
        return Err(rt::DENIED);
    }
    let body=&request[HEADER+64..HEADER+n];
    let q=exec::Request::parse(&body[..exec::SIZE]).map_err(|_|rt::BAD_ARGUMENT)?;
    q.scope(&incarnation::SERVER,&incarnation::DOMAIN,id).map_err(|_|rt::DENIED)?;
    let mut signed=[0;512];
    let size=match h.operation {
        40=>{
            signed[..exec::ADMIT.len()].copy_from_slice(exec::ADMIT);
            signed[exec::ADMIT.len()..exec::ADMIT.len()+exec::SIZE].copy_from_slice(q.bytes());
            exec::ADMIT.len()+exec::SIZE
        }
        41=>{
            let data=q.finish(body[exec::SIZE],body[exec::SIZE+1..].try_into().map_err(|_|rt::BAD_ARGUMENT)?)
                .map_err(|_|rt::BAD_ARGUMENT)?;
            signed[..exec::FINISH.len()].copy_from_slice(exec::FINISH);
            signed[exec::FINISH.len()..exec::FINISH.len()+65].copy_from_slice(&data);
            exec::FINISH.len()+65
        }
        42=>{
            if body[exec::SIZE..]==[0;32] {return Err(rt::BAD_ARGUMENT);}
            signed[..exec::QUERY.len()].copy_from_slice(exec::QUERY);
            signed[exec::QUERY.len()..exec::QUERY.len()+body.len()].copy_from_slice(body);
            exec::QUERY.len()+body.len()
        }
        _=>return Err(rt::DENIED),
    };
    request[HEADER+n..HEADER+n+64].copy_from_slice(&key.sign(&signed[..size]).to_bytes());
    request[..HEADER].copy_from_slice(&Header {size:(n+64) as u16,..h}.encode().map_err(|_|rt::BAD_ARGUMENT)?);
    let mut response=[0;MAX];let mut response_size=0;
    let result=canal::dialogue(key,|tls,until| {
        if tls.write(&request[..HEADER+n+64])?!=HEADER+n+64 {return Err(TlsError::IoError);}
        tls.flush()?;
        let mut count=0;
        while count<HEADER {
            let n=tls.read(&mut response[count..HEADER])?;
            if n==0 {return Err(TlsError::ConnectionClosed);}count+=n;
        }
        let reply=Header::parse(&response[..HEADER]).map_err(|_|TlsError::InvalidApplicationData)?;
        if reply.sequence!=1 || reply.operation!=h.operation|0x8000 {return Err(TlsError::InvalidApplicationData);}
        let size=usize::from(reply.size);
        while count<HEADER+size {
            let n=tls.read(&mut response[count..HEADER+size])?;
            if n==0 {return Err(TlsError::ConnectionClosed);}count+=n;
        }
        canal::guard(until).map_err(TlsError::Io)?;response_size=size;Ok(())
    });
    let size=match result {
        Ok(())=>response_size,
        // Aucun nouvel envoi automatique. Même une panne d'émission est inconnue
        // pour le demandeur jusqu'à consultation de l'ordre durable.
        Err(_)=>{response[HEADER..HEADER+8].copy_from_slice(&22u64.to_le_bytes());8}
    };
    rpc::send(port,Header {sequence:1,operation:h.operation|0x8000,size:size as u16},
        &response[HEADER..HEADER+size],||Ok(2000))
}
