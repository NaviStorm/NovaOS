//! Coordinateur privé : aucun ACK avant le fait local et la publication relue.
use crate::{common,incarnation,recovery_rpc,runtime as rt,tete,recovery_store::Storage};
use novaos_identity::{membership::Removal,recuperation_view::View};
use novaos_recovery::{membership::{self as m,Binding,Receipt,State},membership_effectors::VfsClosure,framing::{MAX,HEADER}};
use ed25519_dalek::VerifyingKey;
const UNCERTAIN:u64=22;
pub struct Selective {
    pub binding:Binding,
    principal:[u8;16],epoch:u64,instances:[[u8;32];2],
    reserve_sent:bool,position:u64,closed:bool,exec:bool,ack_sent:u8,
}
// Extension privée du bloc de démarrage R22, publiée par le noyau QEMU.
// runtime::config conserve sa borne historique 0..=19 pour les anciens clients.
fn identity_from(first:usize,read:impl Fn(usize)->u64)->rt::Result<[u8;32]> {
    if !matches!(first,23|27){return Err(rt::DENIED);}
    let mut out=[0;32];
    for (i,c) in out.chunks_exact_mut(8).enumerate(){c.copy_from_slice(&read(first+i).to_le_bytes());}
    if out==[0;32]{Err(rt::DENIED)}else{Ok(out)}
}
pub(crate) fn identity(first:usize)->rt::Result<[u8;32]> {
    identity_from(first,|word| {
        // Le registre ne peut écrire cette page. Ces deux plages seules
        // contiennent les identités de ses interlocuteurs, sans nouveaux droits.
        unsafe {(&*(0x401000 as *const core::sync::atomic::AtomicU64).add(word))
            .load(core::sync::atomic::Ordering::Acquire)}
    })
}
fn key()->rt::Result<VerifyingKey>{VerifyingKey::from_bytes(include_bytes!(env!("NOVA_DECISION_PUB"))).map_err(|_|rt::REFUSED)}
pub fn exchange(operation:u16,q:&[u8],reply:&mut[u8;MAX])->rt::Result<usize>{
    m::Message::parse(operation,q).map_err(|_|rt::BAD_ARGUMENT)?;
    let n=recovery_rpc::Client::new(rt::config(8),incarnation::identity()).exchange(operation,q,reply).map_err(|_|UNCERTAIN)?;
    if n<8{return Err(UNCERTAIN)}let status=common::number(&reply[HEADER..]);
    if status!=0{return Err(status)}Ok(n-8)
}
impl Selective {
    pub fn new(removal:&Removal,current:&[u8;112])->rt::Result<Self>{
        let h=removal.triplet.head();
        Ok(Self{binding:Binding{server:incarnation::SERVER,domain:incarnation::DOMAIN,incarnation:incarnation::identity(),
            operation:h.request,subject:removal.source.subject(),group:removal.source.group(),revision:removal.source.revision(),
            generation:h.generation,previous:current[80..112].try_into().unwrap(),published:h.digest,expected:7},
            principal:removal.principal,epoch:removal.principal_epoch,
            instances:[identity(23)?,identity(27)?],
            reserve_sent:false,position:0,closed:false,exec:false,ack_sent:0})
    }
    pub fn query(&self)->rt::Result<Receipt>{Self::query_binding(&self.binding)}
    pub fn query_binding(binding:&Binding)->rt::Result<Receipt>{
        let mut q=[0;160];q[..112].copy_from_slice(&tete::query(rt::config(8),&incarnation::identity())?);
        q[112..128].copy_from_slice(&binding.operation);let nonce=*common::random()?;q[128..].copy_from_slice(&nonce);
        let mut reply=[0;MAX];let n=exchange(m::QUERY,&q,&mut reply)?;
        Receipt::verify(&key()?,&reply[HEADER+8..HEADER+8+n],binding,&nonce).map_err(|_|rt::REFUSED)
    }
    /// Consultation après perte du cache. Aucun ACK ni nouvelle mutation.
    pub fn historical_terminal(current:&[u8;112],operation:[u8;16],principal:[u8;16],state:&View)->rt::Result<Option<u64>> {
        let mut q=[0;160];q[..112].copy_from_slice(current);q[112..128].copy_from_slice(&operation);q[128..].copy_from_slice(&*common::random()?);
        let mut reply=[0;MAX];let n=match exchange(m::HISTORY,&q,&mut reply) {
            Ok(n)=>n,Err(rt::DENIED)=>return Ok(None),Err(e)=>return Err(e),
        };
        let history=m::History::verify(&key()?,&q,&reply[HEADER+8..HEADER+8+n]).map_err(|_|rt::REFUSED)?;
        if history.principal()!=principal {return Err(rt::DENIED);}
        let head=state.head();
        if !history.receipt().completed(head.generation,&head.digest,u64::from_be_bytes(current[72..80].try_into().unwrap()),current[80..].try_into().unwrap()).map_err(|_|rt::REFUSED)? {
            return Err(UNCERTAIN);
        }
        Ok(Some(history.binding().generation))
    }
    pub fn reserve(&mut self,previous:&[u8;112])->rt::Result<()> {
        if !self.reserve_sent {
            let mut q=[0;324];q[..112].copy_from_slice(previous);q[112..128].copy_from_slice(&self.binding.operation);
            q[128..160].copy_from_slice(&self.binding.published);q[160..176].copy_from_slice(&self.principal);
            q[176..184].copy_from_slice(&self.epoch.to_be_bytes());q[184..200].copy_from_slice(&self.binding.subject);
            q[200..216].copy_from_slice(&self.binding.group);q[216..224].copy_from_slice(&self.binding.revision.to_be_bytes());q[224]=3;
            for (i,kind) in [1,2,4].iter().enumerate(){q[225+33*i]=*kind;q[226+33*i..258+33*i].copy_from_slice(&self.instances[i.min(1)]);}
            self.reserve_sent=true;let mut reply=[0;MAX];exchange(m::RESERVE,&q,&mut reply)?;
        }
        let r=self.query()?;if r.state()!=State::Reserved{return Err(UNCERTAIN)}self.position=r.position();Ok(())
    }
    pub fn selection(&self,a:&mut[u8;128])->rt::Result<()> {
        if self.position==0{return Err(UNCERTAIN)}
        a[8..24].copy_from_slice(&self.binding.subject);a[24..40].copy_from_slice(&self.binding.group);
        a[40..48].copy_from_slice(&self.binding.revision.to_le_bytes());a[48..56].copy_from_slice(b"NXMGSEL1");
        a[56..72].copy_from_slice(&self.binding.operation);a[72..104].copy_from_slice(&self.instances[1]);Ok(())
    }
    pub fn exec_verified(&mut self){self.exec=true;}
    pub fn close(&mut self,storage:&mut Storage)->rt::Result<()> {
        if self.position==0 || !self.exec{return Err(UNCERTAIN)}
        let q=VfsClosure::new(self.binding.operation,self.binding.subject,self.binding.group,self.binding.revision,
            self.position,self.instances[0]).map_err(|_|rt::REFUSED)?;
        let facts=storage.membership_close(&q)?;
        // L'association de l'écriture à son admission est exigée, jamais inférée
        // d'un compteur. Ce cas reste non terminal tant que son issue manque.
        if facts.cancelled_prepare().is_some(){return Err(UNCERTAIN)}
        common::marker(&closure_marker(facts.removed(),facts.retained())?);
        self.closed=true;Ok(())
    }
    pub fn terminal_for_binding(binding:&Binding,state:&View)->rt::Result<u64>{
        let current=tete::query(rt::config(8),&incarnation::identity())?;let h=state.head();
        let receipt=Self::query_binding(binding)?;
        if !receipt.completed(h.generation,&h.digest,u64::from_be_bytes(current[72..80].try_into().unwrap()),current[80..].try_into().unwrap()).map_err(|_|rt::REFUSED)? {return Err(UNCERTAIN);}
        Ok(binding.generation)
    }
    pub fn release_terminal(&self,storage:&mut Storage)->rt::Result<()> {
        let r=self.query()?;if r.state()!=State::Retired{return Err(UNCERTAIN);}
        let q=VfsClosure::new(self.binding.operation,self.binding.subject,self.binding.group,self.binding.revision,
            self.position,self.instances[0]).map_err(|_|rt::REFUSED)?;
        storage.membership_terminal(&q)
    }
    pub fn finish(&mut self,state:&View)->rt::Result<()> {
        self.finish_head(&state.head())
    }
    fn finish_head(&mut self,head:&novaos_identity::Head)->rt::Result<()> {
        if !self.closed || !self.exec{return Err(UNCERTAIN)}
        let current=tete::query(rt::config(8),&incarnation::identity())?;
        if current[72..80]!=head.generation.to_be_bytes() || current[80..]!=head.digest{return Err(UNCERTAIN)}
        for (i,kind) in [1u8,2,4].iter().enumerate(){
            let receipt=self.query()?;
            if receipt.acknowledged()&kind!=0{continue;}
            if self.ack_sent&kind!=0{return Err(UNCERTAIN)}
            let mut q=[0;161];q[..112].copy_from_slice(&current);q[112..128].copy_from_slice(&self.binding.operation);
            q[128]=*kind;q[129..].copy_from_slice(&self.instances[i.min(1)]);self.ack_sent|=kind;
            let mut reply=[0;MAX];exchange(m::EFFECT,&q,&mut reply)?;
        }
        let r=self.query()?;
        if !r.completed(head.generation,&head.digest,u64::from_be_bytes(current[72..80].try_into().unwrap()),current[80..].try_into().unwrap()).map_err(|_|rt::REFUSED)?{return Err(UNCERTAIN)}
        common::marker(b"NARM RETIREE recu durable tete P9 relue effecteurs acquittes");Ok(())
    }
}
const _:()=assert!(core::mem::size_of::<Selective>()<=512);

fn closure_marker(removed:u64,retained:u64)->rt::Result<[u8;62]> {
    if removed>99 || retained>99{return Err(rt::REFUSED)}
    let mut out=*b"NARM VFS fermeture selective acquittee retires=00 conserves=00";
    for (offset,value) in [(b"NARM VFS fermeture selective acquittee retires=".len(),removed),
        (b"NARM VFS fermeture selective acquittee retires=00 conserves=".len(),retained)] {
        out[offset]=b'0'+(value/10) as u8;out[offset+1]=b'0'+(value%10) as u8;
    }
    Ok(out)
}

// Représentations réellement conservées dans le registre du profil R2.
const _:()=assert!(core::mem::size_of::<Option<Selective>>()==304);
const _:()=assert!(core::mem::size_of::<Option<([u8;16],Binding)>>()==224);
