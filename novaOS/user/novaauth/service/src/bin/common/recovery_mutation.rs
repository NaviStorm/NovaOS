//! Mutation décidée par R, préparée par NovaFS puis publiée par l'autorité.
use crate::{common,incarnation,recovery_rpc as rpc,recovery_store::Storage,tete,runtime as rt};
use novaos_identity::{recuperation::Triplet,recuperation_view::View,Grant};
use ed25519_dalek::{Signature,VerifyingKey};
use novaos_recovery::framing::{MAX,HEADER};
const CHANGE:&[u8]=b"NovaAuth/QEMU/politique-compare-publie/1\0";
const RECEIPT:&[u8]=b"NovaAuth/QEMU/politique-recu/1\0";
const QUERY:&[u8]=b"NovaAuth/QEMU/reprise-consultation/1\0";
pub const UNCERTAIN:u64=22; // Statut natif NovaFS, sans nouveau sélecteur.
pub struct Pending {
    pub plan:Triplet,
    previous:[u8;112],
    submitted:bool,
    started:bool,
    confirmed:bool,
    reference:[u8;16],
    captured:bool,
}
fn verify(prefix:&[u8],data:&[u8],sig:&[u8])->rt::Result<()> {
    let mut signed=[0;512];let n=prefix.len()+data.len();
    if n>signed.len() {return Err(rt::LIMIT);}
    signed[..prefix.len()].copy_from_slice(prefix);signed[prefix.len()..n].copy_from_slice(data);
    let key=VerifyingKey::from_bytes(include_bytes!(env!("NOVA_DECISION_PUB"))).map_err(|_|rt::REFUSED)?;
    key.verify_strict(&signed[..n],&Signature::from_slice(sig).map_err(|_|rt::REFUSED)?)
        .map_err(|_|rt::REFUSED)
}
impl Pending {
    pub fn new(state:&View,grant:Grant,object:u64,operation:[u8;16],current:[u8;112],reference:[u8;16])->rt::Result<Self> {
        if current[72..80]!=state.head().generation.to_be_bytes() || current[80..]!=state.head().digest {
            return Err(rt::DENIED);
        }
        let plan=state.revoke_plan(grant,object,operation).map_err(|_|rt::DENIED)?;
        Ok(Self {plan,previous:current,submitted:false,started:false,confirmed:false,reference,captured:false})
    }
    pub fn backup(&mut self,epoch:u64,recipients:[[u8;32];2])->rt::Result<()> {
        if !self.confirmed || self.captured {return Err(rt::DENIED);}
        self.captured=true; // consommation avant tout échange ; aucune relance
        common::marker(b"NAR2 capture avant");
        let result=crate::recovery_capture::capture(&self.plan,epoch,recipients);
        common::marker(b"NAR2 capture apres");
        result
    }
    pub fn reference(&self)->[u8;16] {self.reference}
    fn proposal(&self)->[u8;160] {
        let mut out=[0;160];out[..112].copy_from_slice(&self.previous);
        out[112..128].copy_from_slice(&self.plan.head().request);
        out[128..].copy_from_slice(&self.plan.head().digest);out
    }
    fn packet(&self)->[u8;CHANGE.len()+160] {
        let mut out=[0;CHANGE.len()+160];out[..CHANGE.len()].copy_from_slice(CHANGE);
        out[CHANGE.len()..].copy_from_slice(&self.proposal());out
    }
    pub fn submit(&mut self,storage:&mut Storage)->rt::Result<()> {
        if self.started {return Err(rt::DENIED);}
        self.started=true;
        let h=*self.plan.head();let lengths=self.plan.parts().map(|p|p.len() as u32);
        storage.begin_next(self.previous[80..112].try_into().unwrap(),h.request,
            &Triplet::head_bytes(&h),lengths)?;
        for (i,part) in self.plan.parts().iter().enumerate() {storage.append(i,0,part)?;}
        storage.seal()?;
        // Après ce point aucune répétition automatique, même si l'envoi échoue.
        self.submitted=true;
        let mut client=rpc::Client::new(rt::config(8),incarnation::identity());
        let mut answer=[0;MAX];
        let n=client.exchange(30,&self.proposal(),&mut answer).map_err(|_|UNCERTAIN)?;
        if n!=72 || common::number(&answer[HEADER..])!=0 {return Err(UNCERTAIN);}
        verify(RECEIPT,&self.packet(),&answer[HEADER+8..HEADER+n]).map_err(|_|UNCERTAIN)?;
        self.confirmed=true;Ok(())
    }
    pub fn reconcile(&mut self)->rt::Result<()> {
        if !self.submitted {return Err(rt::DENIED);}
        let nonce=common::random()?;let mut q=[0;48];
        q[..16].copy_from_slice(&self.plan.head().request);q[16..].copy_from_slice(&nonce[..]);
        let mut answer=[0;MAX];let mut client=rpc::Client::new(rt::config(8),incarnation::identity());
        let n=client.exchange(31,&q,&mut answer).map_err(|_|UNCERTAIN)?;
        let packet=self.packet();let end=packet.len();
        if n!=8+end+128 || common::number(&answer[HEADER..])!=0 {return Err(UNCERTAIN);}
        let body=&answer[HEADER+8..HEADER+n];
        if body[..end]!=packet {return Err(rt::DENIED);}
        verify(RECEIPT,&packet,&body[end..end+64])?;
        let mut fresh=[0;32+CHANGE.len()+160+64];fresh[..32].copy_from_slice(&nonce[..]);
        fresh[32..].copy_from_slice(&body[..end+64]);
        verify(QUERY,&fresh,&body[end+64..])?;
        self.confirmed=true;Ok(())
    }
    pub fn activate(&self,storage:&mut Storage)->rt::Result<View> {
        if !self.confirmed {return Err(rt::DENIED);}
        let fresh=tete::query(rt::config(8),&incarnation::identity())?;
        let head=*self.plan.head();
        if fresh[72..80]!=head.generation.to_be_bytes() || fresh[80..]!=head.digest {return Err(rt::DENIED);}
        View::read(head,self.plan.parts().map(|p|p.len() as u32),|t,o,b|
            storage.read(t,o as usize,b).map_err(|_|novaos_identity::Fault::Unavailable)).map_err(|_|rt::REFUSED)
    }
}
