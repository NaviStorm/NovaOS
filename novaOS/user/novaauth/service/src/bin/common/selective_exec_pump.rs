//! Pompe du registre isolé. Seul le noyau produit l'identité et l'issue locales.
use crate::{common,incarnation,recovery_rpc as rpc,runtime as rt,tete};
use novaos_recovery::{exec_admission::RECEIPT,framing::{MAX,HEADER}};
#[cfg(not(feature="administration-r2-guest"))]
use novaos_recovery::exec_admission::{Request,SIZE};
#[cfg(feature="administration-r2-guest")]
use novaos_recovery::subject_admission::{Request,SIZE};
const WORDS:usize=48;
#[path="selective_exec_guest.rs"]
mod selective;
pub(crate) fn register(state:&novaos_identity::recuperation_view::View,q:&[u8;128])->rt::Result<()> {selective::register(state,q)}
use ed25519_dalek::{VerifyingKey,Signature};
const LIST:u64=0x4e58474154454c53;
const READ:u64=0x4e58474154450000;
const UNCERTAIN:u64=22;
struct Pending {leaf:u64,request:Request,sent:bool,finish_sent:bool,descriptor:[u8;32]}
pub struct Pump {pending:Option<Pending>}
fn notify(port:u64,operation:u64)->rt::Result<u64> {
    let r=rt::call(rt::IPC_NOTIFY,port,operation,0);
    if r.status==0 {Ok(r.value)} else {Err(r.status)}
}
fn metadata(port:u64)->rt::Result<[u64;WORDS]> {
    let mut words=[0;WORDS];for (i,w) in words.iter_mut().enumerate(){*w=notify(port,READ|i as u64)?;}Ok(words)
}
fn bytes<const N:usize>(words:&[u64])->[u8;N] {
    let mut out=[0;N];for (w,b) in words.iter().zip(out.chunks_exact_mut(8)){b.copy_from_slice(&w.to_le_bytes());}out
}
fn command(port:u64,kind:u8,op:[u8;16],value:u64,child:[u8;32])->rt::Result<()> {
    let mut q=[0;128];q[..8].copy_from_slice(b"NAXGATE1");q[8]=kind;q[16..32].copy_from_slice(&op);
    q[32..40].copy_from_slice(&value.to_le_bytes());q[40..72].copy_from_slice(&child);
    rt::send_for(port,0,&q,1000)
}
fn key()->rt::Result<VerifyingKey> {
    VerifyingKey::from_bytes(include_bytes!(env!("NOVA_DECISION_PUB"))).map_err(|_|rt::REFUSED)
}
fn exchange(op:u16,q:&[u8],answer:&mut [u8;MAX])->rt::Result<usize> {
    let mut client=rpc::Client::new(rt::config(8),incarnation::identity());
    let n=client.exchange(op,q,answer).map_err(|_|UNCERTAIN)?;
    if n<8 {return Err(UNCERTAIN);}
    let status=common::number(&answer[HEADER..]);
    if status!=0 {return Err(status);}Ok(n-8)
}
fn receipt(prefix:&[u8],payload:&[u8],signature:&[u8])->rt::Result<()> {
    let mut packet=[0;512];let n=prefix.len()+payload.len();if n>packet.len(){return Err(rt::LIMIT);}
    packet[..prefix.len()].copy_from_slice(prefix);packet[prefix.len()..n].copy_from_slice(payload);
    key()?.verify_strict(&packet[..n],&Signature::from_slice(signature).map_err(|_|rt::REFUSED)?)
        .map_err(|_|rt::REFUSED)
}
fn request(m:&[u64;WORDS],generation:u64,head:&[u8;32],op:[u8;16])->rt::Result<Request> {
            let mut q=[0;SIZE];
            q[..8].copy_from_slice(if cfg!(feature="administration-r2-guest"){b"NAEXAD01"}else{b"NAEXEC01"});
            #[cfg(feature="administration-r2-guest")]
            {
                q[256..272].copy_from_slice(&bytes::<16>(&m[29..31]));
                q[272..].copy_from_slice(&m[31].to_be_bytes());
            }
            q[8..24].copy_from_slice(&incarnation::SERVER);q[24..40].copy_from_slice(&incarnation::DOMAIN);
            q[40..72].copy_from_slice(&incarnation::identity());q[72..80].copy_from_slice(&generation.to_be_bytes());
            q[80..112].copy_from_slice(&head[..]);q[112..144].copy_from_slice(&bytes::<32>(&m[4..8]));
            q[144..176].copy_from_slice(&bytes::<32>(&m[8..12]));q[176..192].copy_from_slice(&bytes::<16>(&m[12..14]));
            q[192..224].copy_from_slice(&bytes::<32>(&m[14..18]));q[224..232].copy_from_slice(&m[1].to_le_bytes());
            q[232..240].copy_from_slice(b"NATICK01");q[240..256].copy_from_slice(&op);

Request::parse(&q).map_err(|_|rt::REFUSED)
}
impl Pump {
    pub fn new()->Self {Self {pending:None}}
    pub fn poll(&mut self,state:&novaos_identity::recuperation_view::View)->rt::Result<()> {
        let leaf=notify(rt::config(14),LIST)?;if leaf==0 {return Ok(());}
        let m=metadata(leaf)?;
        if m[0]==5 && m[26..28]==[0;2] && self.pending.is_none() {
            // Pas de réservation locale, donc aucune demande n'a pu être signée.
            return command(leaf,3,[0;16],2,[0;32]);
        }
        if m[0]==5 && m[26..28]!=[0;2] && m[28]==0 && self.pending.is_none() {
            // Jamais Requested : la génération distante n'a pas été fixée.
            // Le noyau atteste Cancelled ; le reçu historique donne le lien exact.
            let q=request(&m,m[39],&bytes::<32>(&m[18..22]),bytes::<16>(&m[26..28]))?;
            let descriptor=selective::cancelled_digest(&m,&q)?;
            self.pending=Some(Pending{leaf,request:q,sent:false,finish_sent:false,descriptor});
        }
        if self.pending.as_ref().is_some_and(|p|p.leaf!=leaf) {return Err(UNCERTAIN);}
        if m[0]==0 {
            let bound=bytes::<16>(&m[26..28]);
            let mut descriptor=[0;32];
            // Le ticket est encore sans demande signée. Une panne à cette étape
            // permet l'annulation locale ; aucune admission n'a été envoyée.
            let head=match tete::query(rt::config(8),&incarnation::identity()) {
                Ok(h) if bound!=[0;16] && h[72..80]==state.head().generation.to_be_bytes() && h[80..112]==state.head().digest=>{
                    descriptor=match selective::digest(state,&m){
                        Ok(d)=>d,
                        Err(rt::REVOKED)=>{command(leaf,2,bound,0,[0;32])?;return Ok(());},
                        Err(e)=>return Err(e),
                    };h
                },
                Ok(h) if bound==[0;16] && h[80..112]==bytes::<32>(&m[18..22])=>h,
                _ if bound!=[0;16]=>return Err(UNCERTAIN),
                _=>{command(leaf,2,[0;16],0,[0;32])?;return command(leaf,3,[0;16],2,[0;32]);}
            };
            let generation=u64::from_be_bytes(head[72..80].try_into().unwrap());
            let op:[u8;16]=if bound!=[0;16]{bound}else{common::random()?[..16].try_into().unwrap()};
            command(leaf,0,op,generation,[0;32])?;
            self.pending=Some(Pending {leaf,request:request(&m,generation,&head[80..112].try_into().unwrap(),op)?,sent:false,finish_sent:false,descriptor});
        }
        let p=match self.pending.as_mut() {
            Some(p)=>p,
            None=>return Err(UNCERTAIN), // aucune reprise supposant une annulation
        };
        let m=metadata(leaf)?;let op=bytes::<16>(&m[26..28]);
        if op!=p.request.bytes()[240..256] {return Err(rt::DENIED);}
        let mut answer=[0;MAX];
        if m[0]==1 {
            if !p.sent {
                p.sent=true;
                match p.admit(&mut answer) {
                    Ok(n)=>{
                        let position=p.request.admission(&key()?,&answer[HEADER+8..HEADER+8+n]).map_err(|_|rt::REFUSED)?;
                        command(leaf,1,op,position,[0;32])?;
                    }
                    Err(UNCERTAIN)=>return Err(UNCERTAIN),
                    Err(_)=>command(leaf,2,op,0,[0;32])?,
                }
            } else {
                let out=p.query(&mut answer)?;
                if out.state==0 {command(leaf,1,op,out.admission,[0;32])?;}
                else {return Err(rt::DENIED);}
            }
            return Ok(());
        }
        if matches!(m[0],2|3) {return Ok(());}
        if !matches!(m[0],4|5) {return Err(rt::DENIED);}
        let state=if m[0]==4 {1}else{2};let child=bytes::<32>(&m[22..26]);
        if !p.finish_sent {
            p.finish_sent=true;
            let mut q=[0;SIZE+33];q[..SIZE].copy_from_slice(p.request.bytes());q[SIZE]=state;q[SIZE+1..].copy_from_slice(&child);
            let n=if state==2 && p.descriptor!=[0;32] {
                let mut cancellation=[0;SIZE+32];cancellation[..SIZE].copy_from_slice(p.request.bytes());cancellation[SIZE..].copy_from_slice(&p.descriptor);
                match exchange(65,&cancellation,&mut answer) {
                    Ok(n)=>n,
                    Err(rt::DENIED)=>{
                        // La tombe pré-admission peut être refusée si l'admission
                        // existe. Sa consultation signée précède alors FINISH.
                        let out=p.query(&mut answer)?;
                        if out.state!=0{return Err(UNCERTAIN);}
                        exchange(41,&q,&mut answer)?
                    },
                    Err(e)=>return Err(e),
                }
            }else{exchange(41,&q,&mut answer)?};let b=&answer[HEADER+8..HEADER+8+n];
            let expected=p.request.finish(state,&child).map_err(|_|rt::REFUSED)?;
            if n!=137 || b[8..73]!=expected || b[..8]==[0;8] {return Err(rt::REFUSED);}
            receipt(RECEIPT,&b[..73],&b[73..])?;
        } else {
            let out=p.query(&mut answer)?;
            if out.state!=state || out.child!=child {return Err(UNCERTAIN);}
        }
        command(leaf,3,op,state as u64,child)?;
        self.pending=None;Ok(())
    }
}
impl Pending {
    fn admit(&self,answer:&mut[u8;MAX])->rt::Result<usize>{
        if self.descriptor==[0;32]{return exchange(40,self.request.bytes(),answer);}
        let mut q=[0;SIZE+32];q[..SIZE].copy_from_slice(self.request.bytes());q[SIZE..].copy_from_slice(&self.descriptor);
        exchange(64,&q,answer)
    }
    fn query(&self,answer:&mut [u8;MAX])->rt::Result<novaos_recovery::exec_admission::Outcome> {
        let fresh=*common::random()?;let mut q=[0;SIZE+32];q[..SIZE].copy_from_slice(self.request.bytes());q[SIZE..].copy_from_slice(&fresh);
        let n=exchange(42,&q,answer)?;
        self.request.outcome(&key()?,&fresh,&answer[HEADER+8..HEADER+8+n]).map_err(|_|rt::REFUSED)
    }
}

// La pompe remplace sa version historique ; le descripteur reste emprunté sur
// la pile. Seule son empreinte de 32 octets persiste dans Pending.
const _:()={
 struct Legacy {leaf:u64,request:Request,sent:bool,finish_sent:bool}
 let old=core::mem::size_of::<Option<Legacy>>();
 let new=core::mem::size_of::<Pump>();
 assert!(new>=old && 3994+new-old<=4096);
};
