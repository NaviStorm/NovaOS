#![cfg(feature="administration-lab")]
use novaos_recovery::{membership::*,Error};
use ed25519_dalek::{SigningKey,Signer};
fn fixture()->(SigningKey,Binding,[u8;32],[u8;312]) {
    let key=SigningKey::from_bytes(&[87;32]);
    let binding=Binding{server:[1;16],domain:[2;16],incarnation:[3;32],operation:[4;16],subject:[5;16],group:[6;16],revision:3,generation:8,previous:[7;32],published:[8;32],expected:7};
    let fresh=[9;32];let mut p=[0;312];p[..8].copy_from_slice(b"NAMBRE01");
    for (offset,b) in [(8,binding.server.as_slice()),(24,&binding.domain),(40,&binding.incarnation),(72,&binding.operation),(88,&binding.subject),(104,&binding.group),(136,&binding.previous),(168,&binding.published),(216,&fresh)] {p[offset..offset+b.len()].copy_from_slice(b);}
    p[120..128].copy_from_slice(&3u64.to_be_bytes());p[128..136].copy_from_slice(&8u64.to_be_bytes());p[200..208].copy_from_slice(&16u64.to_be_bytes());p[208..211].copy_from_slice(&[2,7,7]);
    sign(&key,&mut p);(key,binding,fresh,p)
}
fn sign(k:&SigningKey,p:&mut[u8;312]) {let mut b=PREFIX.to_vec();b.extend_from_slice(&p[..248]);p[248..].copy_from_slice(&k.sign(&b).to_bytes());}
#[test]fn terminal_requires_signature_challenge_exact_origin_and_p9_head() {
    let(k,b,f,p)=fixture();let r=Receipt::verify(&k.verifying_key(),&p,&b,&f).unwrap();
    assert!(r.completed(8,&b.published,8,&b.published).unwrap());
    assert!(r.completed(7,&b.previous,8,&b.published).is_err());
    assert!(r.completed(8,&[1;32],8,&[1;32]).is_err());
    for offset in [0,8,24,40,72,88,104,120,128,136,168,200,208,209,210,211,216,248,311] {let mut q=p;q[offset]^=1;assert!(Receipt::verify(&k.verifying_key(),&q,&b,&f).is_err(),"offset {offset}");}
    assert!(Receipt::verify(&k.verifying_key(),&p,&b,&[8;32]).is_err());
    for n in [0,247,248,311] {assert!(Receipt::verify(&k.verifying_key(),&p[..n],&b,&f).is_err());}
}
#[test]fn valid_signature_does_not_turn_pending_or_partial_ack_into_completion() {
    let(k,b,f,mut p)=fixture();p[208]=1;p[210]=3;sign(&k,&mut p);
    let r=Receipt::verify(&k.verifying_key(),&p,&b,&f).unwrap();assert!(!r.completed(8,&b.published,8,&b.published).unwrap());
    p[208]=2;sign(&k,&mut p);assert!(Receipt::verify(&k.verifying_key(),&p,&b,&f).is_err());
    p[208]=0;p[210]=0;sign(&k,&mut p);assert_eq!(Receipt::verify(&k.verifying_key(),&p,&b,&f).unwrap().state(),State::Reserved);
}
#[test]
fn signed_reservation_on_its_original_head_is_incomplete() {
    let (key, binding, challenge, mut packet) = fixture();
    packet[208] = 0;
    packet[210] = 0;
    sign(&key, &mut packet);
    let receipt = Receipt::verify(&key.verifying_key(), &packet, &binding, &challenge).unwrap();
    assert_eq!(receipt.completed(7, &binding.previous, 7, &binding.previous), Ok(false));
    assert!(receipt.completed(7, &[99; 32], 7, &[99; 32]).is_err());
    assert!(receipt.completed(6, &binding.previous, 6, &binding.previous).is_err());
    assert!(receipt.completed(7, &binding.previous, 8, &binding.published).is_err());
}

#[test]fn every_instance_must_ack_and_capacity_is_bounded() {
    let mut inventory=Inventory::new();for i in 1..=32 {inventory.expect(if i==32 {4}else{1},[i;32],i as u64).unwrap();}
    assert_eq!(inventory.expect(2,[33;32],33),Err(Error::Limit));
    assert_eq!(inventory.acknowledge_verified(1,[1;32],2),Err(Error::Scope));
    for i in 1..32 {assert!(inventory.acknowledge_verified(1,[i;32],i as u64).unwrap());}
    assert_eq!(inventory.masks(),(5,1));assert!(!inventory.acknowledge_verified(1,[1;32],1).unwrap());
    inventory.acknowledge_verified(4,[32;32],32).unwrap();assert_eq!(inventory.masks(),(5,5));
    println!("membership inventory={} receipt={} total={} bytes",core::mem::size_of::<Inventory>(),RECEIPT,core::mem::size_of::<Inventory>()+BODY+256);
}

#[test]
fn python_receipt_and_requests_match_native_canonical_encoding() {
    use ed25519_dalek::{VerifyingKey,Signature};
    let vectors:Vec<Vec<u8>>=include_str!("vectors/membership.txt").lines().map(|s|s.as_bytes().chunks_exact(2).map(|b|u8::from_str_radix(core::str::from_utf8(b).unwrap(),16).unwrap()).collect()).collect();
    let key=VerifyingKey::from_bytes(vectors[0].as_slice().try_into().unwrap()).unwrap();
    let binding=Binding{server:[b'S';16],domain:[b'D';16],incarnation:[b'I';32],operation:[b'M';16],subject:[b'A';16],group:[b'G';16],revision:3,generation:7,previous:[b'H';32],published:[b'N';32],expected:7};
    let r=Receipt::verify(&key,&vectors[1],&binding,&[b'F';32]).unwrap();assert_eq!(r.state(),State::Retired);assert_eq!(r.position(),19);
    for (i,op) in (54..=60).enumerate() {
        let bytes=&vectors[2+2*i];let signature=Signature::from_slice(&vectors[3+2*i]).unwrap();
        let q=Message::parse(op,bytes).unwrap();q.scope(&binding.server,&binding.domain,&binding.incarnation).unwrap();
        assert!(q.scope(&binding.server,&binding.domain,&[1;32]).is_err());
        let mut signed=[0;MAX_SIGNED];let n=q.signed(&mut signed);key.verify_strict(&signed[..n],&signature).unwrap();
        for len in 0..bytes.len() {assert!(Message::parse(op,&bytes[..len]).is_err());}
        let mut trailing=bytes.clone();trailing.push(0);assert!(Message::parse(op,&trailing).is_err());
    }
    let mut max=vectors[2][..225].to_vec();max[224]=32;
    for i in 1..=32 {max.push(1);max.extend_from_slice(&[i;32]);}
    assert_eq!(max.len(),MAX_REQUEST);Message::parse(RESERVE,&max).unwrap();
    max[224]=33;assert!(Message::parse(RESERVE,&max).is_err());
}

#[test]
fn restart_must_authenticate_even_an_empty_pending_list() {
    let(key,b,f,_)=fixture();let mut request=[0;144];request[..16].copy_from_slice(&b.server);request[16..32].copy_from_slice(&b.domain);request[32..64].copy_from_slice(&b.incarnation);request[72..80].copy_from_slice(&7u64.to_be_bytes());request[80..112].copy_from_slice(&b.previous);request[112..].copy_from_slice(&f);
    fn reply(key:&SigningKey,data:&[u8])->Vec<u8>{let mut signed=b"NovaAuth/QEMU/membership/pending/v1\0".to_vec();signed.extend_from_slice(data);let mut out=data.to_vec();out.extend_from_slice(&key.sign(&signed).to_bytes());out}
    let mut body=request.to_vec();body.push(0);let empty=reply(&key,&body);
    assert!(Reservations::verify(&key.verifying_key(),&request,&empty).unwrap().entry().is_none());
    body[144]=1;body.extend_from_slice(&b.operation);body.extend_from_slice(&b.subject);body.extend_from_slice(&b.group);body.extend_from_slice(&b.revision.to_be_bytes());body.push(0);
    let response=reply(&key,&body);let pending=Reservations::verify(&key.verifying_key(),&request,&response).unwrap();
    assert!(pending.blocks_group(&b.subject,&b.group,b.revision));assert!(!pending.blocks_group(&b.subject,&[99;16],b.revision));
    assert!(!pending.blocks_group(&[99;16],&b.group,b.revision));assert!(!pending.blocks_group(&b.subject,&b.group,b.revision+1));
    let mut altered=empty;altered[145]^=1;assert!(Reservations::verify(&key.verifying_key(),&request,&altered).is_err());
    request[112]^=1;assert!(Reservations::verify(&key.verifying_key(),&request,&response).is_err());
}

#[test]fn lost_response_preserves_individual_outcome_and_never_invents_cancellation() {
    use ed25519_dalek::VerifyingKey;
    let v:Vec<Vec<u8>>=include_str!("vectors/membership.txt").lines().map(|s|s.as_bytes().chunks_exact(2).map(|b|u8::from_str_radix(core::str::from_utf8(b).unwrap(),16).unwrap()).collect()).collect();
    let key=VerifyingKey::from_bytes(v[0].as_slice().try_into().unwrap()).unwrap();
    assert_eq!(operation_outcome(&key,&v[16],&v[8],&v[17]).unwrap(),OperationOutcome::Completed([b'C';32]));
    let signer=SigningKey::from_bytes(&[87;32]);
    fn seal(key:&SigningKey,r:&mut[u8]){let mut b=b"NovaAuth/QEMU/membership/query/v1\0".to_vec();b.extend_from_slice(&r[..458]);r[458..].copy_from_slice(&key.sign(&b).to_bytes());}
    let mut r=v[17].clone();r[417]=0;r[418..458].fill(0);seal(&signer,&mut r);
    assert_eq!(operation_outcome(&key,&v[16],&v[8],&r).unwrap(),OperationOutcome::Pending);
    r[417]=2;seal(&signer,&mut r);assert!(operation_outcome(&key,&v[16],&v[8],&r).is_err());
    r[450..458].copy_from_slice(&19u64.to_be_bytes());seal(&signer,&mut r);
    assert_eq!(operation_outcome(&key,&v[16],&v[8],&r).unwrap(),OperationOutcome::Cancelled);
    let mut query=v[16].clone();query[128]^=1;assert!(operation_outcome(&key,&query,&v[8],&r).is_err());
}


#[test]
fn history_reconstructs_only_signed_original_and_distinguishes_unfinished() {
    let (key,b,f,receipt)=fixture();let mut original=vec![0;324];
    original[..16].copy_from_slice(&b.server);original[16..32].copy_from_slice(&b.domain);
    original[32..64].copy_from_slice(&b.incarnation);original[64..72].copy_from_slice(&1u64.to_be_bytes());
    original[72..80].copy_from_slice(&7u64.to_be_bytes());original[80..112].copy_from_slice(&b.previous);
    original[112..128].copy_from_slice(&b.operation);original[128..160].copy_from_slice(&b.published);
    original[160..176].copy_from_slice(&[44;16]);original[176..184].copy_from_slice(&1u64.to_be_bytes());
    original[184..200].copy_from_slice(&b.subject);original[200..216].copy_from_slice(&b.group);
    original[216..224].copy_from_slice(&b.revision.to_be_bytes());original[224]=3;
    for(i,k)in [1u8,2,4].iter().enumerate(){original[225+33*i]=*k;original[226+33*i..258+33*i].fill(*k);}
    let mut query=vec![0;160];query[..112].copy_from_slice(&original[..112]);
    query[32..64].fill(99); // nouvelle incarnation de consultation, pas ancien effecteur
    query[72..80].copy_from_slice(&8u64.to_be_bytes());query[80..112].copy_from_slice(&b.published);
    query[112..128].copy_from_slice(&b.operation);query[128..].copy_from_slice(&f);
    let wrap=|r:&[u8]| {let mut body=b"NAMBHI01".to_vec();body.extend_from_slice(&query);body.extend_from_slice(&324u16.to_be_bytes());body.extend_from_slice(&original);body.extend_from_slice(r);
        let mut signed=b"NovaAuth/QEMU/membership/history/v1\0".to_vec();signed.extend_from_slice(&body);body.extend_from_slice(&key.sign(&signed).to_bytes());body};
    let encoded=wrap(&receipt);let h=History::verify(&key.verifying_key(),&query,&encoded).unwrap();
    assert_eq!(h.principal(),[44;16]);assert_eq!(h.original(),original);assert_eq!(h.binding(),&b);
    assert!(h.receipt().completed(8,&b.published,8,&b.published).unwrap());
    for i in [0,8,40,128,168,170,330,494,800,869] {let mut bad=encoded.clone();bad[i]^=1;assert!(History::verify(&key.verifying_key(),&query,&bad).is_err(),"offset {i}");}
    for n in [0,169,493,805,869] {assert!(History::verify(&key.verifying_key(),&query,&encoded[..n]).is_err());}
    let mut other=query.clone();other[128]^=1;assert!(History::verify(&key.verifying_key(),&other,&encoded).is_err());
    // Borne applicative réelle : 32 effecteurs canoniques, signature valide.
    let mut largest=original[..225].to_vec();largest[224]=32;
    for i in 1u8..=32 {largest.push(if i<=30{1}else if i==31{2}else{4});largest.extend_from_slice(&[i;32]);}
    let seal_history=|request:&[u8]| {let mut body=b"NAMBHI01".to_vec();body.extend_from_slice(&query);body.extend_from_slice(&(request.len() as u16).to_be_bytes());body.extend_from_slice(request);body.extend_from_slice(&receipt);
        let mut signed=b"NovaAuth/QEMU/membership/history/v1\0".to_vec();signed.extend_from_slice(&body);body.extend_from_slice(&key.sign(&signed).to_bytes());body};
    let maximum=seal_history(&largest);assert_eq!(maximum.len(),1827);
    assert_eq!(History::verify(&key.verifying_key(),&query,&maximum).unwrap().original(),largest);
    let mut extra=maximum.clone();extra.push(0);assert!(History::verify(&key.verifying_key(),&query,&extra).is_err());
    largest[224]=33;largest.push(4);largest.extend_from_slice(&[33;32]);
    assert!(History::verify(&key.verifying_key(),&query,&seal_history(&largest)).is_err());
    let mut pending=receipt;pending[208]=1;pending[210]=3;sign(&key,&mut pending);let pending=wrap(&pending);
    assert!(!History::verify(&key.verifying_key(),&query,&pending).unwrap().receipt().completed(8,&b.published,8,&b.published).unwrap());
}

#[test] fn recovered_individual_request_must_match_digest_signature_and_fresh_query() {
    use ed25519_dalek::VerifyingKey;
    let v:Vec<Vec<u8>>=include_str!("vectors/membership.txt").lines().map(|s|s.as_bytes().chunks_exact(2).map(|b|u8::from_str_radix(core::str::from_utf8(b).unwrap(),16).unwrap()).collect()).collect();
    let key=VerifyingKey::from_bytes(v[0].as_slice().try_into().unwrap()).unwrap();
    let digest=novaos_recovery::hash(&v[8]);
    let (state,original)=recovered_operation(&key,&v[16],&digest,&v[17]).unwrap();
    assert_eq!(state,OperationOutcome::Completed([b'C';32]));assert_eq!(original,v[8]);
    assert!(recovered_operation(&key,&v[16],&[1;32],&v[17]).is_err());
    let mut query=v[16].clone();query[128]^=1;
    assert!(recovered_operation(&key,&query,&digest,&v[17]).is_err());
    for at in [168,280,353,409,417,450,521] {let mut bad=v[17].clone();bad[at]^=1;assert!(recovered_operation(&key,&v[16],&digest,&bad).is_err());}
}
