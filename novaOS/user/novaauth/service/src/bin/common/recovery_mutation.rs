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
    #[cfg(feature="administration-r2-guest")]
    administration:Option<[u8;49]>,
    #[cfg(feature="membership-r2-guest")]
    pub selective:Option<crate::membership_transaction::Selective>,
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
        Ok(Self {plan,previous:current,submitted:false,started:false,confirmed:false,reference,captured:false,
            #[cfg(feature="administration-r2-guest")] administration:None,
            #[cfg(feature="membership-r2-guest")] selective:None})
    }
    #[cfg(feature="administration-r2-guest")]
    pub fn from_administration(state:&View,grant:Grant,operation:[u8;16],current:[u8;112],reference:[u8;16],
        change:novaos_identity::administration::Operation<'_>,retired:Option<&novaos_identity::administration::Retired>)->rt::Result<Self> {
        if current[..16]!=crate::incarnation::SERVER || current[16..32]!=state.head().domain ||
            current[32..64]!=incarnation::identity() || current[72..80]!=state.head().generation.to_be_bytes() || current[80..]!=state.head().digest {
            return Err(rt::DENIED);
        }
        let p=state.administration_publication(grant,operation,change,retired).map_err(|e|match e {
            novaos_identity::Fault::Conflict=>24,
            novaos_identity::Fault::Limit=>rt::LIMIT,
            novaos_identity::Fault::BadInput=>rt::BAD_ARGUMENT,
            novaos_identity::Fault::Uncertain=>UNCERTAIN,
            novaos_identity::Fault::Unavailable=>rt::TIMED_OUT,
            novaos_identity::Fault::Corrupt|novaos_identity::Fault::StateMismatch=>rt::REFUSED,
            _=>rt::DENIED,
        })?;
        Ok(Self{plan:p.triplet,previous:current,submitted:false,started:false,confirmed:false,reference,captured:false,administration:Some(p.metadata),
            #[cfg(feature="membership-r2-guest")] selective:None})
    }
    #[cfg(feature="membership-r2-guest")]
    pub fn from_membership(state:&View,grant:Grant,operation:[u8;16],current:[u8;112],reference:[u8;16],subject:[u8;16],group:[u8;16])->rt::Result<Self> {
        if current[..16]!=incarnation::SERVER || current[16..32]!=state.head().domain
            || current[32..64]!=incarnation::identity() || current[72..80]!=state.head().generation.to_be_bytes()
            || current[80..]!=state.head().digest {return Err(rt::REVOKED);}
        let removal=state.prepare_membership_removal(grant,operation,subject,group).map_err(|e|match e {
            novaos_identity::Fault::Conflict=>24,novaos_identity::Fault::Limit=>rt::LIMIT,
            novaos_identity::Fault::Uncertain=>UNCERTAIN,_=>rt::DENIED})?;
        let selective=crate::membership_transaction::Selective::new(&removal,&current)?;
        Ok(Self{plan:removal.triplet,previous:current,submitted:false,started:false,confirmed:false,
            reference,captured:false,administration:None,selective:Some(selective)})
    }
    pub fn started(&self)->bool {self.started}
    #[cfg(feature="membership-r2-guest")]
    pub fn reserve_membership(&mut self)->rt::Result<()> {
        self.selective.as_mut().ok_or(rt::DENIED)?.reserve(&self.previous)
    }
    #[cfg(feature="membership-r2-guest")]
    pub fn finish_membership(&mut self,state:&View,storage:&mut Storage)->rt::Result<()> {
        if let Some(s)=self.selective.as_mut(){s.finish(state)?;s.release_terminal(storage)?;}Ok(())
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
    #[cfg(feature="administration-r2-guest")]
    pub fn retirement(&self)->Option<([u8;16],u64)> {
        let m=self.administration?;
        (m[48]==novaos_recovery::administration::DISABLE).then(||
            (m[24..40].try_into().unwrap(),u64::from_be_bytes(m[40..48].try_into().unwrap())))
    }
    #[cfg(feature="administration-r2-guest")]
    pub fn verify_retirement(&self,receipt:&[u8])->rt::Result<()> {
        let (subject,epoch)=self.retirement().ok_or(rt::DENIED)?;
        let h=self.plan.head();
        if receipt.len()!=192 || receipt[16..32]!=h.domain || receipt[32..48]!=subject
            || receipt[48..56]!=epoch.to_be_bytes() || receipt[56..72]!=h.request
            || receipt[72..80]!=h.generation.to_be_bytes() || receipt[96..128]!=self.previous[80..112] {
            return Err(rt::DENIED);
        }
        let key=VerifyingKey::from_bytes(include_bytes!(env!("NOVA_DECISION_PUB"))).map_err(|_|rt::REFUSED)?;
        novaos_identity::administration::Retired::verify(incarnation::SERVER,&key,&receipt[..128],&receipt[128..])
            .map_err(|_|rt::DENIED)?;Ok(())
    }
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
        #[cfg(feature="membership-r2-guest")]
        if let Some(s)=self.selective.as_mut(){s.close(storage)?;}
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
        #[cfg(feature="administration-r2-guest")]
        let n=if {
            #[cfg(feature="membership-r2-guest")] {self.selective.is_some()}
            #[cfg(not(feature="membership-r2-guest"))] {false}
        } {
            let mut q=[0;128];q[..112].copy_from_slice(&self.previous);q[112..].copy_from_slice(&h.request);
            client.exchange(novaos_recovery::membership::PUBLISH,&q,&mut answer).map_err(|_|UNCERTAIN)?
        }else if let Some(metadata)=self.administration {
            let mut packet=[0;209];packet[..160].copy_from_slice(&self.proposal());packet[160..].copy_from_slice(&metadata);
            novaos_recovery::administration::Message::parse(novaos_recovery::administration::PUBLISH,&packet).map_err(|_|UNCERTAIN)?;
            client.exchange(novaos_recovery::administration::PUBLISH,&packet,&mut answer).map_err(|_|UNCERTAIN)?
        }else{client.exchange(30,&self.proposal(),&mut answer).map_err(|_|UNCERTAIN)?};
        #[cfg(not(feature="administration-r2-guest"))]
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
