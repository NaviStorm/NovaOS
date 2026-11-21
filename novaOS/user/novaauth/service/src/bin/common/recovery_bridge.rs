//! Le signataire ne traite que des octets publics ou déjà scellés pour R/G/A.
use crate::{canal, common, incarnation, recovery_rpc as rpc, runtime as rt};
use ed25519_dalek::{Signer,SigningKey,Signature,VerifyingKey};
use embedded_tls::TlsError;
use novaos_recovery::{RecoveryContextV2,framing::{Header,MAX,HEADER,TAG}};

fn bound(until:u64)->rt::Result<u64> {canal::wait_ticks(until).map_err(|_|rt::TIMED_OUT)}
fn io(_:u64)->TlsError {TlsError::IoError}
pub fn serve(key:&SigningKey,port:u64,id:&[u8;32],first:rt::ReceiveV1)->rt::Result<()> {
    let mut initial=Some(first);
    let mut last=0u32;
    let mut bound_context=None;
    canal::dialogue(key,|tls,until| {
        // Deux passes sur trois objets de 16 Kio, plus 32 échanges de contrôle.
        // Borne de messages uniquement : l’échéance TLS de 10 s reste absolue.
        for _ in 0..(2*3*16+32) {
            let first=match initial.take() {
                Some(m)=>m,
                None=>rt::receive_for(port,bound(until).map_err(io)?).map_err(io)?
            };
            let mut request=zeroize::Zeroizing::new([0;MAX]);
            let h=rpc::receive(port,first,&mut request,||bound(until)).map_err(io)?;
            let n=usize::from(h.size);
            if h.sequence!=last+1 || n<64 || request[HEADER..HEADER+16]!=incarnation::SERVER
                || request[HEADER+16..HEADER+32]!=incarnation::DOMAIN || request[HEADER+32..HEADER+64]!=*id
            {return Err(TlsError::InvalidApplicationData);}
            last=h.sequence;
            let mut answer=zeroize::Zeroizing::new([0;MAX]);
            let size=match h.operation {
                rpc::END if n==64 => {
                    rpc::send(port,Header {sequence:h.sequence,operation:h.operation|0x8000,size:8},
                        &rt::OK.to_le_bytes(),||bound(until)).map_err(io)?;
                    return Ok(());
                }
                rpc::BIND if n==64+376+64 => {
                    let bytes=&request[HEADER+64..HEADER+64+376];
                    let context=RecoveryContextV2::parse(bytes).map_err(|_|TlsError::InvalidApplicationData)?;
                    if bytes[88..120]!=*id || bytes[24..40]!=incarnation::SERVER || bytes[40..56]!=incarnation::DOMAIN
                        || bound_context.is_some() {return Err(TlsError::InvalidApplicationData);}
                    const PREFIX_POS:&[u8]=b"NovaAuth/recovery/possession/v1\0";
                    let mut proof=[0;PREFIX_POS.len()+376];
                    proof[..PREFIX_POS.len()].copy_from_slice(PREFIX_POS);proof[PREFIX_POS.len()..].copy_from_slice(bytes);
                    VerifyingKey::from_bytes(&context.possession()).map_err(|_|TlsError::InvalidApplicationData)?.verify_strict(&proof,&Signature::from_slice(&request[HEADER+64+376..HEADER+n])
                        .map_err(|_|TlsError::InvalidApplicationData)?).map_err(|_|TlsError::InvalidSignature)?;
                    const PREFIX:&[u8]=b"NovaAuth/recovery/instance-binding/v1\0";
                    let mut signed=[0;PREFIX.len()+376];signed[..PREFIX.len()].copy_from_slice(PREFIX);
                    signed[PREFIX.len()..].copy_from_slice(bytes);
                    bound_context=Some(context.digest());
                    answer[HEADER..HEADER+8].copy_from_slice(&rt::OK.to_le_bytes());
                    answer[HEADER+8..HEADER+72].copy_from_slice(&key.sign(&signed).to_bytes());
                    72
                }
                rpc::CURRENT..=rpc::RETRIEVE => {
                    if tls.write(&request[..HEADER+n])?!=HEADER+n {return Err(TlsError::IoError);}
                    tls.flush()?;
                    let mut p=0;
                    while p<HEADER {
                        let count=tls.read(&mut answer[p..HEADER])?;
                        if count==0 {return Err(TlsError::ConnectionClosed);} p+=count;
                    }
                    let reply=Header::parse(&answer[..HEADER]).map_err(|_|TlsError::InvalidApplicationData)?;
                    if reply.sequence!=h.sequence || reply.operation!=(h.operation|0x8000) {return Err(TlsError::InvalidApplicationData);}
                    let size=usize::from(reply.size);
                    while p<HEADER+size {
                        let count=tls.read(&mut answer[p..HEADER+size])?;
                        if count==0 {return Err(TlsError::ConnectionClosed);} p+=count;
                    }
                    size
                }
                _=>return Err(TlsError::InvalidApplicationData)
            };
            bound(until).map_err(io)?;
            rpc::send(port,Header {sequence:h.sequence,operation:h.operation|0x8000,size:size as u16},
                &answer[HEADER..HEADER+size],||bound(until)).map_err(io)?;
        }
        Err(TlsError::InvalidApplicationData)
    }).map_err(|_|rt::REFUSED)?;
    common::marker(b"NAR2 dialogue ferme delai global inchange");
    Ok(())
}
pub fn is_request(m:&rt::ReceiveV1)->bool {m.kind==1 && m.tag==TAG && m.len==128}
