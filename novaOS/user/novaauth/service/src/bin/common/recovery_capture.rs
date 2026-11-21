//! Capture chiffrée dans le registre ; seuls manifeste et chiffrés quittent R.
use crate::{common,incarnation,recovery_guest::Random,recovery_rpc as rpc,runtime as rt,tete};
use novaos_identity::recuperation::Triplet;
use novaos_recovery::{Capture,Encoder,CHUNK,framing::{HEADER,MAX}};
use ed25519_dalek::{Signature,VerifyingKey};
use zeroize::Zeroizing;
const REGISTER:&[u8]=b"NovaAuth/recovery/register-backup/v1\0";
pub fn capture(plan:&Triplet,epoch:u64,recipients:[[u8;32];2])->rt::Result<()> {
    let h=*plan.head();let id=incarnation::identity();
    let before=tete::query(rt::config(8),&id)?;
    if before[72..80]!=h.generation.to_be_bytes() || before[80..]!=h.digest {return Err(rt::DENIED);}
    let backup=*common::random()?;
    let description=Capture {server:incarnation::SERVER,domain:incarnation::DOMAIN,
        backup,generation:h.generation,key_epoch:epoch,recipients,head:Triplet::head_bytes(&h)};
    let mut encoder=Encoder::new(&description,plan.parts().map(|p|p.len() as u32),&mut Random).map_err(|_|rt::REFUSED)?;
    let mut index=0u32;
    for part in plan.parts() {
        for source in part.chunks(CHUNK) {
            let mut block=Zeroizing::new([0;CHUNK]);
            block[..source.len()].copy_from_slice(source);
            let tag=encoder.seal(&mut block[..source.len()]).map_err(|_|rt::REFUSED)?;
            let mut q=[0;36+CHUNK+16];q[..32].copy_from_slice(&backup);
            q[32..36].copy_from_slice(&index.to_be_bytes());
            q[36..36+source.len()].copy_from_slice(&block[..source.len()]);
            q[36+source.len()..52+source.len()].copy_from_slice(&tag);
            let mut answer=[0;MAX];let mut client=rpc::Client::new(rt::config(8),id);
            let n=client.exchange(33,&q[..52+source.len()],&mut answer)?;
            if n!=8 || common::number(&answer[HEADER..])!=0 {return Err(rt::REFUSED);}
            index+=1;
        }
    }
    let manifest=encoder.finish().map_err(|_|rt::REFUSED)?;
    let after=tete::query(rt::config(8),&id)?;
    if before!=after {return Err(rt::DENIED);}
    let mut answer=[0;MAX];let mut client=rpc::Client::new(rt::config(8),id);
    // Une remise perdue n'est jamais répétée. Le manifeste public identifie
    // l'issue d'enregistrement dans le dépôt indépendant.
    let mut n=client.exchange(32,&manifest,&mut answer).unwrap_or(0);
    if n!=184 || common::number(&answer[HEADER..])!=0 {
        let nonce=*common::random()?;let mut q=[0;64];
        q[..32].copy_from_slice(&novaos_recovery::hash(&manifest));q[32..].copy_from_slice(&nonce);
        let mut reader=rpc::Client::new(rt::config(8),id);
        n=reader.exchange(34,&q,&mut answer).map_err(|_|22u64)?;
        if n!=248 || common::number(&answer[HEADER..])!=0 {return Err(22);}
        const QUERY:&[u8]=b"NovaAuth/recovery/backup-query/v1\0";
        let mut signed=[0;QUERY.len()+32+176];signed[..QUERY.len()].copy_from_slice(QUERY);
        signed[QUERY.len()..QUERY.len()+32].copy_from_slice(&nonce);
        signed[QUERY.len()+32..].copy_from_slice(&answer[HEADER+8..HEADER+184]);
        VerifyingKey::from_bytes(include_bytes!(env!("NOVA_DECISION_PUB"))).map_err(|_|rt::REFUSED)?
            .verify_strict(&signed,&Signature::from_slice(&answer[HEADER+184..HEADER+248]).map_err(|_|rt::REFUSED)?)
            .map_err(|_|rt::DENIED)?;
        n=184;
        common::marker(b"NARV capture issue retrouvee aucune repetition");
    }
    let mut packet=[0;112];packet[..16].copy_from_slice(&incarnation::SERVER);
    packet[16..32].copy_from_slice(&incarnation::DOMAIN);
    packet[32..64].copy_from_slice(&novaos_recovery::hash(&manifest));
    packet[64..80].copy_from_slice(&manifest[78..94]);packet[80..].copy_from_slice(&h.digest);
    if n!=184 || common::number(&answer[HEADER..])!=0 || answer[HEADER+8..HEADER+120]!=packet {
        return Err(22);
    }
    let mut signed=[0;REGISTER.len()+112];signed[..REGISTER.len()].copy_from_slice(REGISTER);
    signed[REGISTER.len()..].copy_from_slice(&packet);
    VerifyingKey::from_bytes(include_bytes!(env!("NOVA_DECISION_PUB"))).map_err(|_|rt::REFUSED)?
        .verify_strict(&signed,&Signature::from_slice(&answer[HEADER+120..HEADER+184]).map_err(|_|rt::REFUSED)?)
        .map_err(|_|rt::DENIED)?;
    Ok(())
}
