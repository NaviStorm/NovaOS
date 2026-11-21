use novaos_recovery::exec_admission::*;
use ed25519_dalek::{SigningKey,Signer};
fn fixture()->Request {
    let mut b=[b'X';SIZE];b[..8].copy_from_slice(b"NAEXEC01");b[72..80].copy_from_slice(&6u64.to_be_bytes());
    Request::parse(&b).unwrap()
}
fn signed(k:&SigningKey,prefix:&[u8],data:&[u8])->Vec<u8> {
    let proof=k.sign(&[prefix,data].concat());[data,proof.to_bytes().as_slice()].concat()
}
#[test]fn canonical_and_bound() {
    let q=fixture();assert_eq!(q.bytes().len(),256);
    assert!(q.scope(&[b'X';16],&[b'X';16],&[b'X';32]).is_ok());
    assert!(q.scope(&[b'Y';16],&[b'X';16],&[b'X';32]).is_err());
    for i in [0,72,80,112,144,176,192,224,240] {
        let mut b=*q.bytes();if i==72 {b[i]=255;} else if i==0 {b[0]=0;} else {
            let n=if [176,224,240].contains(&i){16}else{32};b[i..i+n].fill(0);
        }
        assert!(Request::parse(&b).is_err(),"offset {i}");
    }
    assert!(Request::parse(&q.bytes()[..255]).is_err());
}
#[test]fn receipt_cannot_authorize_another_ticket() {
    let q=fixture();let k=SigningKey::from_bytes(&[7;32]);
    let payload=[4u64.to_be_bytes().as_slice(),q.bytes()].concat();
    let receipt=signed(&k,RECEIPT,&payload);
    assert_eq!(q.admission(&k.verifying_key(),&receipt).unwrap(),4);
    let mut changed=*q.bytes();changed[240]^=1;
    assert!(Request::parse(&changed).unwrap().admission(&k.verifying_key(),&receipt).is_err());
    let mut corrupt=receipt.clone();corrupt[0]^=1;
    assert!(q.admission(&k.verifying_key(),&corrupt).is_err());
    assert!(q.admission(&SigningKey::from_bytes(&[8;32]).verifying_key(),&receipt).is_err());
}
#[test]fn query_freshness_and_terminal_evidence() {
    let q=fixture();let k=SigningKey::from_bytes(&[9;32]);let fresh=[3;32];
    for (state,child,terminal) in [(0,[0;32],0),(1,[5;32],9),(2,[0;32],9)] {
        let data=[q.bytes().as_slice(),&fresh,&4u64.to_be_bytes(),&[state],&child,&(terminal as u64).to_be_bytes()].concat();
        let reply=signed(&k,QUERY,&data);let out=q.outcome(&k.verifying_key(),&fresh,&reply).unwrap();
        assert_eq!(out.state,state);assert_eq!(out.child,child);
        assert!(q.outcome(&k.verifying_key(),&[4;32],&reply).is_err());
    }
    assert!(q.finish(2,&[1;32]).is_err());assert!(q.finish(1,&[0;32]).is_err());
    assert!(q.finish(0,&[0;32]).is_err());
}
