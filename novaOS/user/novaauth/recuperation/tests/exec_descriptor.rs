#![cfg(feature="administration-lab")]
use novaos_recovery::{exec_descriptor::*,subject_admission,Error};
use ed25519_dalek::{Signer,SigningKey};
const FIXTURE:&[u8;800]=include_bytes!("fixtures/exec_descriptor_v1.bin");
#[test] fn canonical_shared_python_vector_and_sizes(){
 let d=Descriptor::parse(FIXTURE).unwrap();assert_eq!(d.dependencies().count(),4);
 assert_eq!(MAX_SIZE,800);assert_eq!(d.dependencies().next().unwrap().origin(),1);
 assert_eq!(core::mem::size_of::<Descriptor>(),16);
 assert!(core::mem::size_of::<Assembly>()<=72);
 for i in 0..13 {assert_eq!(fragment(d,i).unwrap().len(),128);}
 assert!(fragment(d,13).is_err());
}
#[test] fn every_truncation_versions_padding_duplicates_refused(){
 for n in 0..800{assert!(Descriptor::parse(&FIXTURE[..n]).is_err());}
 for at in [0,295,504,512,513,515] {let mut p=*FIXTURE;p[at]=255;assert!(Descriptor::parse(&p).is_err());}
 let mut p=*FIXTURE;p[728..].copy_from_slice(&FIXTURE[656..728]);assert!(Descriptor::parse(&p).is_err());
}
#[test] fn ordered_and_reordered_fragments_exact_and_missing_never_succeeds(){
 let d=Descriptor::parse(FIXTURE).unwrap();let mut buffer=[0;MAX_SIZE];
 let mut a=Assembly::new(&mut buffer,d.operation(),d.digest(),800).unwrap();
 assert_eq!(a.descriptor(),Err(Error::Missing));
 for i in (1..13).rev(){assert!(!a.push(&fragment(d,i).unwrap()).unwrap());}
 assert_eq!(a.descriptor(),Err(Error::Missing));assert!(a.push(&fragment(d,0).unwrap()).unwrap());
 assert_eq!(a.descriptor().unwrap().bytes(),FIXTURE);
}
#[test] fn duplicate_conflicting_other_operation_and_digest_refused(){
 let d=Descriptor::parse(FIXTURE).unwrap();let mut buffer=[0;MAX_SIZE];
 let mut a=Assembly::new(&mut buffer,d.operation(),d.digest(),800).unwrap();let p=fragment(d,0).unwrap();
 assert!(!a.push(&p).unwrap());assert!(!a.push(&p).unwrap());
 let mut altered=p;altered[64]^=1;assert_eq!(a.push(&altered),Err(Error::Duplicate));
 for at in [8,24,58,60,62]{let mut altered=p;altered[at]^=1;assert!(a.push(&altered).is_err());}
}
#[test] fn corruption_in_last_fragment_is_not_accepted(){
 let d=Descriptor::parse(FIXTURE).unwrap();let mut buffer=[0;MAX_SIZE];
 let mut a=Assembly::new(&mut buffer,d.operation(),d.digest(),800).unwrap();
 for i in 0..12{a.push(&fragment(d,i).unwrap()).unwrap();}
 let mut p=fragment(d,12).unwrap();p[64]^=1;assert_eq!(a.push(&p),Err(Error::Crypto));
}
#[test] fn registration_receipt_signature_position_and_descriptor_bound(){
 let d=Descriptor::parse(FIXTURE).unwrap();let k=SigningKey::from_bytes(&[9;32]);
 let mut msg=Vec::from(RECEIPT);msg.extend_from_slice(&7u64.to_be_bytes());msg.extend_from_slice(FIXTURE);
 let sig=k.sign(&msg).to_bytes();d.verify_registration(&k.verifying_key(),7,&sig).unwrap();
 assert!(d.verify_registration(&k.verifying_key(),8,&sig).is_err());
 assert!(d.verify_registration(&k.verifying_key(),0,&sig).is_err());
 let mut other=*FIXTURE;other[224]^=1;let other=Descriptor::parse(&other).unwrap();
 assert!(other.verify_registration(&k.verifying_key(),7,&sig).is_err());
}
#[test] fn admission_binding_full_and_initial_head_not_replaced(){
 let d=Descriptor::parse(FIXTURE).unwrap();let mut p=[0;280];p[..8].copy_from_slice(b"NAEXAD01");
 for (a,b,n) in [(8,8,64),(72,112,32),(160,144,32),(240,176,16),(296,192,32),(136,256,16),(152,272,8),(224,240,16)] {p[b..b+n].copy_from_slice(&FIXTURE[a..a+n]);}
 p[72..80].copy_from_slice(&7u64.to_be_bytes());p[80..112].fill(88);p[224..240].fill(99);
 let q=subject_admission::Request::parse(&p).unwrap();d.matches(&q).unwrap();
 for at in [8,24,40,112,144,176,192,240,256,272]{let mut altered=p;altered[at]^=1;
  if let Ok(q)=subject_admission::Request::parse(&altered){assert!(d.matches(&q).is_err());}}
 p[72..80].copy_from_slice(&5u64.to_be_bytes());assert!(d.matches(&subject_admission::Request::parse(&p).unwrap()).is_err());
}
#[test] fn refusal_poisoning_cannot_be_recovered_by_a_later_fragment(){
 let d=Descriptor::parse(FIXTURE).unwrap();let mut buffer=[0;MAX_SIZE];
 let mut a=Assembly::new(&mut buffer,d.operation(),d.digest(),800).unwrap();
 let mut p=fragment(d,0).unwrap();p[8]^=1;assert!(a.push(&p).is_err());
 for i in 0..13{assert!(a.push(&fragment(d,i).unwrap()).is_err());}
 assert!(a.descriptor().is_err());
}
#[test] fn signed_message_scope_and_domains_are_distinct(){
 use novaos_recovery::exec_selective_message as m;
 let q=m::Message::parse(m::REGISTER,FIXTURE).unwrap();
 q.scope(&FIXTURE[8..24].try_into().unwrap(),&FIXTURE[24..40].try_into().unwrap(),&FIXTURE[40..72].try_into().unwrap()).unwrap();
 assert!(q.scope(&[0;16],&[0;16],&[0;32]).is_err());
 let mut out=[0;m::MAX_SIGNED];let n=q.signed(&mut out).unwrap();
 assert_eq!(&out[..n],[REGISTER,FIXTURE].concat());
 assert!(m::Message::parse(m::ADMIT,FIXTURE).is_err());assert!(m::Message::parse(40,FIXTURE).is_err());
}
#[test] fn fresh_lookup_binds_every_historical_byte_without_admitting_it(){
 let key=SigningKey::from_bytes(&[9;32]);let d=Descriptor::parse(FIXTURE).unwrap();
 let mut q=[0;112];q[..64].copy_from_slice(&FIXTURE[8..72]);q[64..80].copy_from_slice(&d.operation());q[80..].fill(42);
 let mut r=Vec::from(q);r.extend_from_slice(&7u64.to_be_bytes());r.extend_from_slice(FIXTURE);r.extend_from_slice(&[81;64]);
 let mut signed=Vec::from(LOOKUP);signed.extend_from_slice(&r);r.extend_from_slice(&key.sign(&signed).to_bytes());
 let (position,found)=verify_lookup(&key.verifying_key(),&q,&r).unwrap();assert_eq!(position,7);assert_eq!(found,d);
 for at in [0,16,32,64,80]{let mut wrong=q;wrong[at]^=1;assert!(verify_lookup(&key.verifying_key(),&wrong,&r).is_err());}
 for at in [112,120+224,120+432,120+800,120+863,r.len()-1] {let mut wrong=r.clone();wrong[at]^=1;assert!(verify_lookup(&key.verifying_key(),&q,&wrong).is_err());}
 for n in 0..r.len(){assert!(verify_lookup(&key.verifying_key(),&q,&r[..n]).is_err());}
 let mut wrong=r.clone();wrong.push(0);assert!(verify_lookup(&key.verifying_key(),&q,&wrong).is_err());
 // L'historique reste consultable après reprise, sans falsifier l'incarnation
 // d'origine : matches() interdira son admission dans la nouvelle incarnation.
 q[32..64].fill(77);r[..112].copy_from_slice(&q);let end=r.len()-64;
 let mut msg=Vec::from(LOOKUP);msg.extend_from_slice(&r[..end]);r[end..].copy_from_slice(&key.sign(&msg).to_bytes());
 assert_eq!(verify_lookup(&key.verifying_key(),&q,&r).unwrap().1.bytes()[40..72],FIXTURE[40..72]);
}
