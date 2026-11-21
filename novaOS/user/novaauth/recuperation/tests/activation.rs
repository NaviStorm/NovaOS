use ed25519_dalek::{Signer,SigningKey};
use novaos_recovery::{RecoveryContextV2,activation::*};
fn signed(key:&SigningKey,prefix:&[u8],data:&[u8])->Vec<u8> {
 let mut out=data.to_vec();out.extend_from_slice(&key.sign(&[prefix,data].concat()).to_bytes());out
}
fn setup()->(SigningKey,RecoveryContextV2,Vec<u8>,Vec<u8>) {
 let key=SigningKey::from_bytes(&[3;32]);let mut c=[1;376];
 c[..8].copy_from_slice(b"NARECV02");c[88..120].fill(2);c[248..280].fill(3);c[280..312].fill(4);
 c[312..344].copy_from_slice(key.verifying_key().as_bytes());
 for (at,n) in [(120,0u64),(128,13),(200,2),(240,900)] {c[at..at+8].copy_from_slice(&n.to_be_bytes());}
 let context=RecoveryContextV2::parse(&c).unwrap();
 let receipt=signed(&key,ACTIVATE,&[&context.digest()[..],&c[136..168]].concat());
 let state=[&c[24..56],&c[88..120],&1u64.to_be_bytes(),&c[128..168]].concat();
 let message=[&[7;32][..],&state].concat();
 let sig=key.sign(&[b"NovaAuth/QEMU/etat-courant/1\0".as_slice(),&message].concat());
 let current=[state.as_slice(),&sig.to_bytes()].concat();
 (key,context,receipt,current)
}
#[test] fn recu_historique_ne_reactive_pas_droit_retire() {
 let (key,c,r,now)=setup();let k=key.verifying_key();
 assert_eq!(Activation::verify(&k,&c,&r,&[7;32],&now).unwrap().generation(),13);
 // Un ancien reçu garde sa signature ; la tête courante authentique a avancé.
 for at in [0,16,32,64,72,80] {
  let mut state=now[..112].to_vec();state[at]^=1;
  let bytes=[&[7;32][..],&state].concat();
  let sig=key.sign(&[b"NovaAuth/QEMU/etat-courant/1\0".as_slice(),&bytes].concat());
  let changed=[state.as_slice(),&sig.to_bytes()].concat();
  assert!(Activation::verify(&k,&c,&r,&[7;32],&changed).is_err(),"{at}");
 }
 for at in 0..r.len() {let mut bad=r.clone();bad[at]^=1;assert!(Activation::verify(&k,&c,&bad,&[7;32],&now).is_err());}
 assert!(Activation::verify(&k,&c,&r,&[8;32],&now).is_err());
 assert!(Activation::verify(&k,&c,&r,&[7;32],&[]).is_err()); // autorité indisponible
}
#[test] fn issue_perdue_consultation_distincte_de_activation() {
 let (key,c,r,now)=setup();let k=key.verifying_key();
 let content=[&c.digest()[..],&[9;32],&[9],b"activated",&r].concat();
 let answer=signed(&key,QUERY,&content);assert_eq!(answer.len(),266);
 let old=historical_receipt(&k,&c,&[9;32],&answer).unwrap();
 assert_eq!(old.as_slice(),r);
 assert!(Activation::verify(&k,&c,&old,&[7;32],&[]).is_err());
 Activation::verify(&k,&c,&old,&[7;32],&now).unwrap();
 assert!(historical_receipt(&k,&c,&[10;32],&answer).is_err());
 for at in 0..answer.len() {let mut bad=answer.clone();bad[at]^=1;assert!(historical_receipt(&k,&c,&[9;32],&bad).is_err());}
}
