#![allow(dead_code)]
mod runtime {
    pub type Result<T>=core::result::Result<T,u64>;
    pub const DENIED:u64=5;pub const REFUSED:u64=12;pub const BAD_ARGUMENT:u64=3;
    pub fn config(word:usize)->u64{assert!(word<=19);1}
}
mod common {
    pub fn number(b:&[u8])->u64{u64::from_le_bytes(b[..8].try_into().unwrap())}
    pub fn random()->crate::runtime::Result<&'static [u8;32]>{Ok(&[42;32])}
    pub fn marker(_: &[u8]){}
}
mod incarnation {
    pub const SERVER:[u8;16]=[1;16];pub const DOMAIN:[u8;16]=[2;16];pub fn identity()->[u8;32]{[3;32]}
}
mod tete {pub fn query(_:u64,_:&[u8;32])->crate::runtime::Result<[u8;112]>{Ok(crate::sim::current())}}
mod recovery_store {
    use novaos_recovery::{membership_effectors::{VfsClosure,VfsFacts},hash};
    pub struct Storage{pub calls:usize,pub cancelled:u64,pub error:Option<u64>}
    impl Storage {
        pub fn new()->Self{Self{calls:0,cancelled:0,error:None}}
        pub fn membership_terminal(&mut self,_:&VfsClosure)->crate::runtime::Result<()>{self.calls+=1;Ok(())}
        pub fn membership_close(&mut self,q:&VfsClosure)->crate::runtime::Result<VfsFacts>{
            self.calls+=1;if let Some(e)=self.error{return Err(e)}
            let mut a=[0;128];a[8..16].copy_from_slice(b"NXMGACK1");a[16..48].copy_from_slice(&hash(q.bytes()));
            a[48..80].copy_from_slice(&q.bytes()[72..104]);a[80..88].copy_from_slice(&1u64.to_le_bytes());
            a[88..96].copy_from_slice(&self.cancelled.to_le_bytes());a[104..112].copy_from_slice(&2u64.to_le_bytes());
            q.verify(&a).map_err(|_|12)
        }
    }
}
mod recovery_rpc {
    use novaos_recovery::framing::MAX;
    pub struct Client;impl Client {
        pub fn new(_:u64,_:[u8;32])->Self{Self}
        pub fn exchange(&mut self,op:u16,q:&[u8],a:&mut[u8;MAX])->Result<usize,()>{crate::sim::exchange(op,q,a)}
    }
}
mod sim {
    use novaos_recovery::{membership::{Binding,BODY,PREFIX},framing::{HEADER,MAX}};
    use ed25519_dalek::{Signer,SigningKey};
    #[derive(Default)]struct Fake {reserved:bool,published:bool,ack:u8,calls:[usize;7],lost:Option<(u16,bool)>,substitute:bool}
    std::thread_local!{static SIM:core::cell::RefCell<Fake>=core::cell::RefCell::new(Fake::default());}
    pub fn reset(){SIM.with(|s|*s.borrow_mut()=Fake::default())}
    pub fn publish(){SIM.with(|s|{let mut s=s.borrow_mut();assert!(s.reserved);s.published=true;})}
    pub fn lose(op:u16,committed:bool){SIM.with(|s|s.borrow_mut().lost=Some((op,committed)))}
    pub fn substitute(){SIM.with(|s|s.borrow_mut().substitute=true)}
    pub fn calls(op:u16)->usize{SIM.with(|s|s.borrow().calls[(op-54) as usize])}
    pub fn binding()->Binding{Binding{server:[1;16],domain:[2;16],incarnation:[3;32],operation:[6;16],subject:[8;16],group:[9;16],revision:0,generation:2,previous:[10;32],published:[11;32],expected:7}}
    pub fn head(published:bool)->[u8;112]{let b=binding();let mut h=[0;112];h[..16].copy_from_slice(&b.server);h[16..32].copy_from_slice(&b.domain);h[32..64].copy_from_slice(&b.incarnation);h[64..72].copy_from_slice(&1u64.to_be_bytes());h[72..80].copy_from_slice(&(if published{2u64}else{1}).to_be_bytes());h[80..].copy_from_slice(if published{&b.published}else{&b.previous});h}
    pub fn current()->[u8;112]{SIM.with(|s|head(s.borrow().published))}
    pub fn p9()->novaos_identity::Head{novaos_identity::Head{domain:[2;16],volume:[[12;16],[13;16]],recovery:[14;16],objects:[1,2,3],generation:2,request:[6;16],digest:[11;32]}}
    pub fn exchange(op:u16,q:&[u8],a:&mut[u8;MAX])->Result<usize,()> {
        SIM.with(|cell|{
            let mut s=cell.borrow_mut();s.calls[(op-54) as usize]+=1;a.fill(0);
            let lost=s.lost.filter(|(k,_)|*k==op);if lost.is_some(){s.lost=None;}
            if lost==Some((op,false)){return Err(())}
            let n=match op {
                54=>{assert!(!s.reserved);assert_eq!(q.len(),324);assert_eq!(q[224],3);
                    for (i,kind) in [1,2,4].iter().enumerate(){assert_eq!(q[225+33*i],*kind);assert_eq!(&q[226+33*i..258+33*i],&[if *kind==1{4}else{5};32]);}
                    s.reserved=true;8},
                56=>{assert!(s.reserved&&s.published);let kind=q[128];assert_eq!(&q[112..128],&[6;16]);assert_eq!(&q[129..],&[if kind==1{4}else{5};32]);assert_eq!(s.ack&kind,0);s.ack|=kind;8},
                59=>{
                    if !s.reserved{return Err(())}let b=binding();let mut v=[0u8;312];v[..8].copy_from_slice(b"NAMBRE01");
                    for (offset,data) in [(8,b.server.as_slice()),(24,&b.domain),(40,&b.incarnation),(72,&b.operation),(88,&b.subject),(104,&b.group),(136,&b.previous),(168,&b.published)]{v[offset..offset+data.len()].copy_from_slice(data);}
                    v[128..136].copy_from_slice(&b.generation.to_be_bytes());v[200..208].copy_from_slice(&7u64.to_be_bytes());
                    v[208]=if s.ack==7{2}else if s.published{1}else{0};v[209]=7;v[210]=s.ack;v[216..248].copy_from_slice(&q[128..]);
                    if s.substitute{v[104]^=1;}
                    let mut signed=PREFIX.to_vec();signed.extend_from_slice(&v[..BODY]);v[BODY..].copy_from_slice(&SigningKey::from_bytes(&[9;32]).sign(&signed).to_bytes());
                    a[HEADER+8..HEADER+8+v.len()].copy_from_slice(&v);8+v.len()
                },_=>panic!("unexpected RPC {op}")
            };
            if lost.is_some(){Err(())}else{Ok(n)}
        })
    }
}
