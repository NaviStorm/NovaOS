//! Client de récupération isolé. Aucune clé de déchiffrement ne quitte ce processus.
use crate::{common,incarnation,recovery_rpc as rpc,runtime as rt};
use ed25519_dalek::{Signer,SigningKey,Signature,VerifyingKey};
use novaos_recovery::{self as crypto,Manifest,RecoveryContextV2,SharesV2,framing::{HEADER,MAX}};
use rand_core::{RngCore,CryptoRng};
use zeroize::Zeroizing;

#[path="../../../../src/snapshot_digest.rs"]
mod native;
#[cfg(feature="triplet-guest")]
#[path="../../cipher_cache.rs"]
mod cipher_cache;

pub(super) struct Random;
impl RngCore for Random {
    fn fill_bytes(&mut self,out:&mut [u8]) {for part in out.chunks_mut(32) {
        let bytes=common::random().unwrap_or_else(|_|rt::exit(rt::REFUSED));
        part.copy_from_slice(&bytes[..part.len()]);
    }}
    fn next_u32(&mut self)->u32 {let mut b=[0;4];self.fill_bytes(&mut b);u32::from_le_bytes(b)}
    fn next_u64(&mut self)->u64 {let mut b=[0;8];self.fill_bytes(&mut b);u64::from_le_bytes(b)}
    fn try_fill_bytes(&mut self,b:&mut [u8])->Result<(),rand_core::Error> {self.fill_bytes(b);Ok(())}
}
impl CryptoRng for Random {}
fn checked<'a>(client:&mut rpc::Client,op:u16,body:&[u8],answer:&'a mut [u8;MAX])->rt::Result<&'a [u8]> {
    let n=client.exchange(op,body,answer)?;
    if n<8 {return Err(rt::REFUSED);}
    let status=common::number(&answer[HEADER..]);
    if status!=rt::OK {return Err(status);}
    Ok(&answer[HEADER+8..HEADER+n])
}
fn signature(key:&VerifyingKey,prefix:&[u8],data:&[u8],sig:&[u8])->rt::Result<()> {
    let mut message=[0;1024];
    let n=prefix.len().checked_add(data.len()).filter(|&n|n<=message.len()).ok_or(rt::LIMIT)?;
    message[..prefix.len()].copy_from_slice(prefix);message[prefix.len()..n].copy_from_slice(data);
    key.verify_strict(&message[..n],&Signature::from_slice(sig).map_err(|_|rt::REFUSED)?).map_err(|_|rt::DENIED)
}
fn sign(key:&SigningKey,prefix:&[u8],data:&[u8])->rt::Result<[u8;64]> {
    let mut message=[0;1024];
    let n=prefix.len().checked_add(data.len()).filter(|&n|n<=message.len()).ok_or(rt::LIMIT)?;
    message[..prefix.len()].copy_from_slice(prefix);message[prefix.len()..n].copy_from_slice(data);
    Ok(key.sign(&message[..n]).to_bytes())
}
fn current(client:&mut rpc::Client,key:&VerifyingKey,answer:&mut [u8;MAX])->rt::Result<[u8;112]> {
    let fresh=*common::random()?;
    let result=checked(client,rpc::CURRENT,&fresh,answer)?;
    if result.len()!=176 {return Err(rt::REFUSED);}
    let mut msg=[0;144];msg[..32].copy_from_slice(&fresh);msg[32..].copy_from_slice(&result[..112]);
    signature(key,b"NovaAuth/QEMU/etat-courant/1\0",&msg,&result[112..])?;
    if result[..16]!=incarnation::SERVER || result[16..32]!=incarnation::DOMAIN {return Err(rt::DENIED);}
    Ok(result[..112].try_into().unwrap())
}

#[cfg(feature="vue-guest")]
pub struct Activated {
    pub storage:crate::recovery_store::Storage,
    pub head:[u8;144],
    pub lengths:[u32;3],
    pub epoch:u64,
    pub recipients:[[u8;32];2],
}
#[cfg(not(feature="vue-guest"))]
pub type Activated=();
/// Collecte cryptographique réelle. La vue ne sort qu’après reçu et tête fraîche.
pub fn receive(port:u64,id:&[u8;32])->rt::Result<Activated> {
    #[cfg(feature="triplet-guest")]
    cipher_cache::qualify().map_err(|_|rt::REFUSED)?;
    common::marker(b"NAR2 registre avant");
    let mut client=rpc::Client::new(port,*id);
    let result=receive_inner(&mut client,id);
    // Les secrets de receive_inner sont effacés avant cette fermeture.
    // Une erreur cryptographique n'abandonne pas le signataire en attente.
    let mut answer=Zeroizing::new([0;MAX]);
    let closed=checked(&mut client,rpc::END,&[],&mut answer)
        .and_then(|r| if r.is_empty() {Ok(())} else {Err(rt::REFUSED)});
    if closed.is_err() {common::marker(b"NAR2 fermeture dialogue non confirmee");}
    common::marker(b"NAR2 registre apres");
    result.and_then(|value|closed.map(|_|value))
}
fn receive_inner(client:&mut rpc::Client,id:&[u8;32])->rt::Result<Activated> {
    let decision=VerifyingKey::from_bytes(include_bytes!(env!("NOVA_DECISION_PUB"))).map_err(|_|rt::REFUSED)?;
    let mut answer=Zeroizing::new([0;MAX]);
    let head=current(client,&decision,&mut answer)?;
    #[cfg(feature="triplet-guest")]
    common::marker(b"NAR2 tete courante authentifiee cache-tests=OK");
    #[cfg(not(feature="triplet-guest"))]
    common::marker(b"NAR2 tete courante authentifiee");
    // Le manifeste est public ; les trois objets restent chiffrés hors serveur.
    let result=checked(client,rpc::BACKUP,&head[72..112],&mut answer)?;
    let manifest=Manifest::parse(result).map_err(|_|rt::REFUSED)?;
    if manifest.head()[112..]!=head[80..112] || manifest.head()[88..96]!=head[72..80]
        || manifest.bytes()[14..30]!=incarnation::SERVER || manifest.bytes()[30..46]!=incarnation::DOMAIN
    {return Err(rt::DENIED);}
    let fresh=*common::random()?;
    let mut query=[0;40];query[..8].copy_from_slice(&manifest.epoch().to_be_bytes());query[8..].copy_from_slice(&fresh);
    let result=checked(client,rpc::KEYS,&query,&mut answer)?;
    if result.len()!=432 || result[336..368]!=fresh {return Err(rt::REFUSED);}
    signature(&decision,b"NovaAuth/recovery/receiver-keys-current/v1\0",&result[..368],&result[368..])?;
    signature(&decision,b"NovaAuth/recovery/receiver-keys/v1\0",&result[..272],&result[272..336])?;
    if &result[..8]!=b"NARKEY01" || result[8..24]!=incarnation::SERVER
        || result[24..40]!=incarnation::DOMAIN || result[40..48]!=manifest.epoch().to_be_bytes()
    {return Err(rt::DENIED);}
    let mut keys=[[0;32];6];
    for i in 0..6 {
        keys[i].copy_from_slice(&result[48+32*i..80+32*i]);
        if keys[i]==[0;32] || keys[..i].contains(&keys[i]) {return Err(rt::REFUSED);}
    }
    if crypto::hash(&keys[0])!=manifest.bytes()[94..126] || crypto::hash(&keys[1])!=manifest.bytes()[126..158] {
        return Err(rt::DENIED);
    }
    let mut rng=Random;
    let (g,pg)=crypto::keypair(&mut rng);let (a,pa)=crypto::keypair(&mut rng);
    let possession=SigningKey::from_bytes(&*common::random()?);
    let mut proposal=[0;128];
    proposal[..32].copy_from_slice(&manifest.digest());
    proposal[32..64].copy_from_slice(&pg);proposal[64..96].copy_from_slice(&pa);
    proposal[96..].copy_from_slice(&possession.verifying_key().to_bytes());
    let result=checked(client,rpc::CHALLENGE,&proposal,&mut answer)?;
    let context=RecoveryContextV2::parse(result).map_err(|_|rt::REFUSED)?;
    let c=context.bytes();
    if c[24..40]!=incarnation::SERVER || c[40..56]!=incarnation::DOMAIN || c[88..120]!=*id
        || c[56..88]!=head[32..64] || c[120..128]!=head[64..72] || c[128..136]!=head[72..80]
        || c[136..168]!=head[80..112] || context.manifest()!=manifest.digest()
        || context.epoch()!=manifest.epoch() || context.recipient(1).map_err(|_|rt::REFUSED)?!=pg
        || context.recipient(2).map_err(|_|rt::REFUSED)?!=pa || context.possession()!=possession.verifying_key().to_bytes()
    {return Err(rt::DENIED);}
    let pop=sign(&possession,b"NovaAuth/recovery/possession/v1\0",c)?;
    let mut binding=[0;440];binding[..376].copy_from_slice(c);binding[376..].copy_from_slice(&pop);
    let result=checked(client,rpc::BIND,&binding,&mut answer)?;
    let server=VerifyingKey::from_bytes(include_bytes!(env!("NOVA_QEMU_SERVER_PUB"))).map_err(|_|rt::REFUSED)?;
    signature(&server,b"NovaAuth/recovery/instance-binding/v1\0",c,result)?;
    let mut approval=[0;504];approval[..376].copy_from_slice(c);
    approval[376..440].copy_from_slice(result);approval[440..].copy_from_slice(&pop);
    let result=checked(client,rpc::APPROVE,&approval,&mut answer)?;
    if result.len()!=104 || result[..32]!=context.digest()
        || u64::from_be_bytes(result[32..40].try_into().unwrap())!=u64::from_be_bytes(c[120..128].try_into().unwrap())+1
    {return Err(rt::REFUSED);}
    signature(&decision,b"NovaAuth/recovery/receipt/v1\0",&result[..40],&result[40..])?;
    common::marker(b"NAR2 incarnation et deux recepteurs approuves");
    let senders=[VerifyingKey::from_bytes(&keys[4]).map_err(|_|rt::REFUSED)?,
                 VerifyingKey::from_bytes(&keys[5]).map_err(|_|rt::REFUSED)?];
    let mut collector=SharesV2::new(&context,senders).map_err(|_|rt::REFUSED)?;
    for (i,receiver) in [&g,&a].into_iter().enumerate() {
        let role=(i+1) as u8;
        let delivery=common::random()?[..16].try_into().unwrap();
        let nonce=*common::random()?;
        let scope=context.delivery(delivery,nonce);
        let mut reservation=[0;129];
        reservation[..16].copy_from_slice(&c[8..24]);reservation[16]=role;
        reservation[17..33].copy_from_slice(&delivery);reservation[33..65].copy_from_slice(&nonce);
        let mut data=[0;81];data[..32].copy_from_slice(&context.digest());data[32]=role;
        data[33..49].copy_from_slice(&delivery);data[49..].copy_from_slice(&nonce);
        reservation[65..].copy_from_slice(&sign(&possession,b"NovaAuth/recovery/reserve/v1\0",&data)?);
        let result=checked(client,rpc::RESERVE,&reservation,&mut answer)?;
        if result.len()!=145 || result[..81]!=data {return Err(rt::REFUSED);}
        signature(&decision,b"NovaAuth/recovery/reserve/v1\0",&result[..81],&result[81..])?;
        let request=crypto::seal_request(role,&scope,&keys[2+i],&possession,&mut rng).map_err(|_|rt::REFUSED)?;
        let mut deliver=[0;309];deliver[..16].copy_from_slice(&c[8..24]);deliver[16]=role;deliver[17..].copy_from_slice(&request);

        let lost=match checked(client,rpc::DELIVER,&deliver,&mut answer) {
            Ok(_)=>false,
            Err(22)=>true, // UNCERTAIN : consulter seulement, jamais redemander DELIVER.
            Err(e)=>return Err(e),
        };
        let result=if lost {
            let fresh=*common::random()?;
            let mut query=[0;113];query[..16].copy_from_slice(&c[8..24]);query[16]=role;
            query[17..49].copy_from_slice(&fresh);
            let mut signed=[0;65];signed[..32].copy_from_slice(&context.digest());signed[32]=role;
            signed[33..].copy_from_slice(&fresh);
            query[49..].copy_from_slice(&sign(&possession,b"NovaAuth/recovery/retrieve/v2\0",&signed)?);
            common::marker(b"NAR2 UNCERTAIN consultation sans repetition mutation");
            checked(client,rpc::RETRIEVE,&query,&mut answer)?
        } else {
            let n=u16::from_be_bytes(answer[14..16].try_into().unwrap()) as usize;
            &answer[HEADER+8..HEADER+n]
        };
        if result.len()!=324 {return Err(rt::REFUSED);}
        // La clé de l'autre rôle doit être incapable d'ouvrir cette enveloppe.
        if crypto::open_share(role,&scope,if i==0 {&a}else{&g},&senders[i],result).is_ok() {return Err(rt::REFUSED);}
        if !collector.receive(role,&scope,receiver,result).map_err(|_|rt::REFUSED)? {return Err(rt::REFUSED);}
        let envelope_hash=crypto::hash(result);
        let mut ack=[0;113];ack[..16].copy_from_slice(&c[8..24]);ack[16]=role;
        ack[17..49].copy_from_slice(&envelope_hash);
        let mut signed=[0;49];signed[0]=role;signed[1..17].copy_from_slice(&delivery);signed[17..].copy_from_slice(&envelope_hash);
        ack[49..].copy_from_slice(&sign(&possession,b"NovaAuth/recovery/ack/v1\0",&signed)?);
        if !checked(client,rpc::ACK,&ack,&mut answer)?.is_empty() {return Err(rt::REFUSED);}
        common::marker(if role==1 {b"NAR2 remise G valide autre cle refusee"}else{b"NAR2 remise A valide autre cle refusee"});
    }
    let shares=collector.take().map_err(|_|rt::REFUSED)?;
    let mut decoder=crypto::Decoder::new(manifest.clone(),&shares).map_err(|_|rt::REFUSED)?;
    let chunks=u32::from_be_bytes(manifest.bytes()[378..382].try_into().unwrap());
    let lengths=core::array::from_fn(|i|u32::from_be_bytes(manifest.bytes()[366+4*i..370+4*i].try_into().unwrap()));
    let mut native=native::SnapshotDigest::new(manifest.head(),lengths).map_err(|_|rt::REFUSED)?;
    #[cfg(feature="triplet-guest")]
    let mut cache=cipher_cache::Cache::new(context.digest()).map_err(|_|rt::REFUSED)?;
    let mut total=0usize;
    for index in 0..chunks {
        let mut request=[0;36];request[..32].copy_from_slice(&manifest.digest());request[32..].copy_from_slice(&index.to_be_bytes());
        let result=checked(client,rpc::CIPHERTEXT,&request,&mut answer)?;
        if !(17..=1040).contains(&result.len()) {return Err(rt::REFUSED);}
        let n=result.len()-16;
        let mut clear=Zeroizing::new([0;1024]);clear[..n].copy_from_slice(&result[..n]);
        decoder.open(&mut clear[..n],result[n..].try_into().unwrap()).map_err(|_|rt::REFUSED)?;
        native.push(&clear[..n]).map_err(|_|rt::REFUSED)?;
        #[cfg(feature="triplet-guest")]
        cache.insert(&context.digest(),index,result).map_err(|_|rt::REFUSED)?;
        // Aucun contenu en clair n'est renvoyé au relais ni au terminal.
        total=total.checked_add(n).ok_or(rt::LIMIT)?;
    }
    decoder.finish().map_err(|_|rt::REFUSED)?;
    native.finish().map_err(|_|rt::REFUSED)?;
    #[cfg(not(feature="triplet-guest"))]
    common::marker(b"NAR2 empreinte native triplet conforme tete durable");
    if total==0 {return Err(rt::REFUSED);}
    #[cfg(feature="triplet-guest")]
    {
        // Les ciphertexts vérifiés de cette seule récupération sont bornés à
        // 3 820 octets / huit fragments. Le surplus est retéléchargé normalement.
        cache.seal().map_err(|_|rt::REFUSED)?;
        let used=cache.used();
        let mut marker=*b"NAR2 empreinte native triplet conforme tete durable cache=0000 structure=0000";
        for (at,value) in [(58,used),(73,core::mem::size_of::<cipher_cache::Cache>())] {
            for i in 0..4 {marker[at+i]=b'0'+((value/10usize.pow(3-i as u32))%10) as u8;}
        }
        common::marker(&marker);
        // La tête distante et l'authentification AEAD de chaque fragment sont
        // vérifiées de nouveau avant toute préparation P9.
        let now=current(client,&decision,&mut answer)?;
        if now[32..64]!=*id || now[72..112]!=head[72..112] {return Err(rt::DENIED);}
        let mut storage=crate::recovery_store::Storage::new(c[8..24].try_into().unwrap())?;
        storage.begin(manifest.head(),lengths)?;
        let mut decoder=crypto::Decoder::new(manifest.clone(),&shares).map_err(|_|rt::REFUSED)?;
        let mut table=0usize;let mut offset=0usize;
        for index in 0..chunks {
            let mut request=[0;36];request[..32].copy_from_slice(&manifest.digest());request[32..].copy_from_slice(&index.to_be_bytes());
            let result=match cache.get(&context.digest(),index).map_err(|_|rt::REFUSED)? {
                Some(data)=>data,
                None=>checked(client,rpc::CIPHERTEXT,&request,&mut answer)?,
            };
            if !(17..=1040).contains(&result.len()) {return Err(rt::REFUSED);}
            let n=result.len()-16;let mut clear=Zeroizing::new([0;1024]);
            clear[..n].copy_from_slice(&result[..n]);
            decoder.open(&mut clear[..n],result[n..].try_into().unwrap()).map_err(|_|rt::REFUSED)?;
            storage.append(table,offset,&clear[..n])?;
            offset+=n;
            if offset==lengths[table] as usize {table+=1;offset=0;}
        }
        decoder.finish().map_err(|_|rt::REFUSED)?;
        cache.clear();
        storage.seal()?;
        let mut verifier=native::SnapshotDigest::new(manifest.head(),lengths).map_err(|_|rt::REFUSED)?;
        for (table,length) in lengths.into_iter().enumerate() {
            let mut offset=0usize;let mut data=Zeroizing::new([0;96]);
            while offset<length as usize {
                let n=storage.read(table,offset,&mut data[..])?;
                verifier.push(&data[..n]).map_err(|_|rt::REFUSED)?;offset+=n;
            }
        }
        verifier.finish().map_err(|_|rt::REFUSED)?;
        let now=current(client,&decision,&mut answer)?;
        if now[32..64]!=*id || now[72..112]!=head[72..112] {return Err(rt::DENIED);}
        common::marker(b"NAR2 preparation P9 relue empreinte conforme aucune activation cache nettoye");
        #[cfg(feature="activation-guest")]
        {
            use crypto::activation::{self,Activation,ACTIVATE,QUERY};
            let mut request=[0;112];request[..16].copy_from_slice(&c[8..24]);
            request[16..48].copy_from_slice(&c[136..168]);
            let mut data=[0;64];data[..32].copy_from_slice(&context.digest());data[32..].copy_from_slice(&c[136..168]);
            request[48..].copy_from_slice(&sign(&possession,ACTIVATE,&data)?);
            let mut receipt=[0;128];
            match checked(client,rpc::ACTIVATE,&request,&mut answer) {
                Ok(result)=>{if result.len()!=128 {return Err(rt::REFUSED);}receipt.copy_from_slice(result);}
                Err(22)=>{
                    let fresh=*common::random()?;
                    request[16..48].copy_from_slice(&fresh);
                    let mut query=[0;48];query[..16].copy_from_slice(&c[8..24]);query[16..].copy_from_slice(&fresh);
                    request[48..].copy_from_slice(&sign(&possession,QUERY,&query)?);
                    let result=checked(client,rpc::ISSUE,&request,&mut answer)?;
                    receipt=activation::historical_receipt(&decision,&context,&fresh,result).map_err(|_|rt::DENIED)?;
                    common::marker(b"NAR2 activation UNCERTAIN issue retrouvee sans repetition");
                }
                Err(e)=>return Err(e),
            }
            let fresh=*common::random()?;
            let result=checked(client,rpc::CURRENT,&fresh,&mut answer)?;
            let activation=Activation::verify(&decision,&context,&receipt,&fresh,result).map_err(|_|rt::DENIED)?;
            if activation.head()!=manifest.head()[112..] || activation.incarnation()!=*id {
                return Err(rt::DENIED);
            }
            common::marker(b"NAR2 activation distante recue et tete fraiche validees VFS non raccorde");
            #[cfg(feature="vue-guest")]
            return Ok(Activated {storage,head:manifest.head().try_into().map_err(|_|rt::REFUSED)?,lengths,epoch:manifest.epoch(),recipients:[keys[0],keys[1]]});
        }
    }
    #[cfg(not(feature="vue-guest"))]
    {common::marker(b"NAR2 triplet dechiffre dans registre aucune activation P9");Ok(())}
    #[cfg(feature="vue-guest")]
    Err(rt::REFUSED)
}
