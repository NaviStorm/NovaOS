//! Reçu privé de retrait de groupe ; aucune extension de l'ABI native.
//! La preuve de fraîcheur utilise le défi attendu, jamais une date de transport.
use crate::{Error,Result};
use ed25519_dalek::{Signature,VerifyingKey};
pub const BODY:usize=248;
pub const RECEIPT:usize=312;
pub const PREFIX:&[u8]=b"NovaAuth/QEMU/membership-retired/v1";
#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub enum State {Reserved,Draining,Retired}
#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub struct Binding {
    pub server:[u8;16],pub domain:[u8;16],pub incarnation:[u8;32],pub operation:[u8;16],
    pub subject:[u8;16],pub group:[u8;16],pub revision:u64,pub generation:u64,
    pub previous:[u8;32],pub published:[u8;32],pub expected:u8,
}
pub struct Receipt {binding:Binding,state:State,acknowledged:u8,position:u64}
fn word(b:&[u8],n:usize)->u64 {u64::from_be_bytes(b[n..n+8].try_into().unwrap())}
impl Receipt {
    pub fn verify(key:&VerifyingKey,bytes:&[u8],binding:&Binding,fresh:&[u8;32])->Result<Self> {
        if bytes.len()!=RECEIPT || &bytes[..8]!=b"NAMBRE01" || bytes[211..216]!=[0;5]
            || *fresh==[0;32] || bytes[216..248]!=*fresh {return Err(Error::Format);}
        let actual=Binding {server:bytes[8..24].try_into().unwrap(),domain:bytes[24..40].try_into().unwrap(),
            incarnation:bytes[40..72].try_into().unwrap(),operation:bytes[72..88].try_into().unwrap(),
            subject:bytes[88..104].try_into().unwrap(),group:bytes[104..120].try_into().unwrap(),
            revision:word(bytes,120),generation:word(bytes,128),previous:bytes[136..168].try_into().unwrap(),
            published:bytes[168..200].try_into().unwrap(),expected:bytes[209]};
        if actual!=*binding || actual.server==[0;16] || actual.domain==[0;16]
            || actual.incarnation==[0;32] || actual.operation==[0;16] || actual.subject==[0;16]
            || actual.group==[0;16] || actual.previous==[0;32] || actual.published==[0;32]
            || actual.generation==0 || actual.generation>=1<<63 || actual.revision>=actual.generation
            || actual.expected==0 || actual.expected&!7!=0 {return Err(Error::Scope);}
        let state=match bytes[208] {0=>State::Reserved,1=>State::Draining,2=>State::Retired,_=>return Err(Error::Format)};
        let acknowledged=bytes[210];let position=word(bytes,200);
        if position==0 || position>=1<<63 || acknowledged&!actual.expected!=0
            || (state==State::Reserved && acknowledged!=0)
            || (state==State::Retired && (acknowledged!=actual.expected || acknowledged&1==0)) {return Err(Error::Order);}
        let mut signed=[0;PREFIX.len()+BODY];signed[..PREFIX.len()].copy_from_slice(PREFIX);
        signed[PREFIX.len()..].copy_from_slice(&bytes[..BODY]);
        key.verify_strict(&signed,&Signature::from_slice(&bytes[BODY..]).map_err(|_|Error::Crypto)?)
            .map_err(|_|Error::Crypto)?;
        Ok(Self{binding:actual,state,acknowledged,position})
    }
    pub fn state(&self)->State {self.state}
    pub fn position(&self)->u64 {self.position}
    pub fn acknowledged(&self)->u8 {self.acknowledged}
    /// Le registre doit passer la tête P9 relue ET la tête indépendante fraîche.
    /// Cette comparaison n'invente aucune preuve de publication ou d'effecteur.
    pub fn completed(&self,p9_generation:u64,p9_digest:&[u8;32],authority_generation:u64,
        authority_digest:&[u8;32])->Result<bool> {
        if p9_generation!=authority_generation || p9_digest!=authority_digest
            || p9_generation<self.binding.generation {return Err(Error::Scope);}
        if p9_generation==self.binding.generation && *p9_digest!=self.binding.published {return Err(Error::Scope);}
        Ok(self.state==State::Retired)
    }
}

/// Inventaire compact destiné au registre ; le masque de classe n'en tient pas lieu.
/// Les acquittements ne sont jamais fabriqués par le timeout ou une instance neuve.
#[derive(Clone,Copy)]
#[repr(C)]
pub struct Effector {instance:[u8;32],local:u64,class:u8,acknowledged:bool,padding:[u8;6]}
pub struct Inventory {entries:[Option<Effector>;32]}
impl Inventory {
    pub const fn new()->Self {Self{entries:[None;32]}}
    pub fn expect(&mut self,class:u8,instance:[u8;32],local:u64)->Result<()> {
        if !matches!(class,1|2|4) || instance==[0;32] || local==0 {return Err(Error::Scope);}
        if self.entries.iter().flatten().any(|e|e.class==class && e.instance==instance) {return Err(Error::Duplicate);}
        let slot=self.entries.iter_mut().find(|e|e.is_none()).ok_or(Error::Limit)?;
        *slot=Some(Effector{instance,local,class,acknowledged:false,padding:[0;6]});Ok(())
    }
    /// Uniquement après vérification du canal, de l'opération, de la provenance,
    /// de la fermeture locale et de chaque issue. Aucun accès aux capacités ici.
    pub fn acknowledge_verified(&mut self,class:u8,instance:[u8;32],local:u64)->Result<bool> {
        let e=self.entries.iter_mut().flatten().find(|e|e.class==class && e.instance==instance && e.local==local).ok_or(Error::Scope)?;
        let first=!e.acknowledged;e.acknowledged=true;Ok(first)
    }
    pub fn masks(&self)->(u8,u8) {
        let mut expected=0;let mut missing=0;
        for e in self.entries.iter().flatten() {expected|=e.class;if !e.acknowledged {missing|=e.class;}}
        (expected,expected&!missing)
    }
}
impl Default for Inventory {fn default()->Self{Self::new()}}
const _:()=assert!(core::mem::size_of::<Inventory>()+BODY+256<=4096);

// Opérations du protocole privé signataire/autorité ; ce ne sont pas des
// sélecteurs ABI. Aucune de ces requêtes n'est exposée au CLI sans le raccord.
pub const RESERVE:u16=54;
pub const PUBLISH:u16=55;
pub const EFFECT:u16=56;
pub const ADMIT:u16=57;
pub const FINISH:u16=58;
pub const QUERY:u16=59;
pub const PENDING:u16=60;
pub const HISTORY:u16=61;
pub const MAX_REQUEST:usize=225+32*33;
pub const MAX_SIGNED:usize=MAX_REQUEST+64;
pub struct Message<'a> {operation:u16,bytes:&'a[u8]}
impl<'a> Message<'a> {
    pub fn parse(operation:u16,b:&'a[u8])->Result<Self> {
        let length=match operation {RESERVE=>{
            if b.len()<225 || b[224]==0 || b[224]>32 {return Err(Error::Format);}
            225+usize::from(b[224])*33
        },PUBLISH=>128,EFFECT=>161,ADMIT=>241,FINISH=>194,QUERY|HISTORY=>160,PENDING=>144,_=>return Err(Error::Format)};
        if b.len()!=length || b[..16]==[0;16] || b[16..32]==[0;16] || b[32..64]==[0;32]
            || word(b,64)>=1<<63 || word(b,72)>=1<<63 || b[80..112]==[0;32] {return Err(Error::Format);}
        match operation {
            RESERVE=>{
                if b[112..128]==[0;16] || b[128..160]==[0;32] || b[160..176]==[0;16]
                    || word(b,176)==0 || word(b,176)>=1<<63 || b[184..200]==[0;16] || b[200..216]==[0;16]
                    || word(b,216)>word(b,72) || word(b,72)==(1<<63)-1 {return Err(Error::Format);}
                let mut last:Option<&[u8]>=None;let mut p9=false;
                for e in b[225..].chunks_exact(33) {
                    if !matches!(e[0],1|2|4) || e[1..]==[0;32] || last.is_some_and(|l|l>=e) {return Err(Error::Format);}
                    p9|=e[0]==1;last=Some(e);
                }
                if !p9 {return Err(Error::Scope);}
            },
            ADMIT=>{
                if b[112..128]==[0;16] || b[128..144]==[0;16] || word(b,144)>word(b,72)
                    || !matches!(b[152],1|2|4) || b[153..185]==[0;32] || b[185..201]==[0;16]
                    || word(b,201)==0 || word(b,201)>=1<<63 || b[209..241]==[0;32] {return Err(Error::Format);}
            },
            EFFECT|FINISH=>{
                if b[112..128]==[0;16] || !matches!(b[128],1|2|4) || b[129..161]==[0;32] {return Err(Error::Format);}
                if operation==FINISH && (!matches!(b[161],1|2) || (b[162..194]!=[0;32])!=(b[161]==1)) {return Err(Error::Format);}
            },
            QUERY|HISTORY if b[112..128]==[0;16] || b[128..]==[0;32]=>return Err(Error::Format),
            PENDING if b[112..]==[0;32]=>return Err(Error::Format),
            PUBLISH if b[112..]==[0;16]=>return Err(Error::Format),
            _=>(),
        }
        Ok(Self{operation,bytes:b})
    }
    pub fn scope(&self,server:&[u8;16],domain:&[u8;16],incarnation:&[u8;32])->Result<()> {
        if self.bytes[..16]!=*server || self.bytes[16..32]!=*domain || self.bytes[32..64]!=*incarnation {return Err(Error::Scope);}
        Ok(())
    }
    pub fn signed(&self,out:&mut[u8;MAX_SIGNED])->usize {
        let prefix:&[u8]=match self.operation {
            RESERVE=>b"NovaAuth/QEMU/membership/reserve/v1\0",PUBLISH=>b"NovaAuth/QEMU/membership/publish/v1\0",
            EFFECT=>b"NovaAuth/QEMU/membership/effect/v1\0",ADMIT=>b"NovaAuth/QEMU/membership/admit/v1\0",
            FINISH=>b"NovaAuth/QEMU/membership/finish/v1\0",QUERY=>b"NovaAuth/QEMU/membership/query/v1\0",
            PENDING=>b"NovaAuth/QEMU/membership/pending/v1\0",HISTORY=>b"NovaAuth/QEMU/membership/history/v1\0",_=>unreachable!()};
        out[..prefix.len()].copy_from_slice(prefix);out[prefix.len()..prefix.len()+self.bytes.len()].copy_from_slice(self.bytes);
        prefix.len()+self.bytes.len()
    }
}
const _:()=assert!(MAX_REQUEST+64+64+crate::framing::HEADER<=crate::framing::MAX);

/// Consultation authentifiée des réservations avant réouverture des décisions.
/// La liste vide n'est admise qu'avec la signature et le défi de cette requête.
pub struct Reservations {entry:Option<ReservedSource>}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct ReservedSource {pub operation:[u8;16],pub subject:[u8;16],pub group:[u8;16],pub revision:u64,pub state:State}
impl Reservations {
    pub fn verify(key:&VerifyingKey,request:&[u8],response:&[u8])->Result<Self> {
        Message::parse(PENDING,request)?;
        if !matches!(response.len(),209|266) || response[..144]!=*request
            || response[144]>1 || response.len()!=209+57*usize::from(response[144]) {return Err(Error::Format);}
        let n=response.len()-64;const P:&[u8]=b"NovaAuth/QEMU/membership/pending/v1\0";
        let mut signed=[0;P.len()+202];signed[..P.len()].copy_from_slice(P);signed[P.len()..P.len()+n].copy_from_slice(&response[..n]);
        key.verify_strict(&signed[..P.len()+n],&Signature::from_slice(&response[n..]).map_err(|_|Error::Crypto)?).map_err(|_|Error::Crypto)?;
        let entry=if response[144]==1 {
            if response[145..161]==[0;16] || response[161..177]==[0;16] || response[177..193]==[0;16]
                || word(response,193)>word(request,72) || response[201]>1 {return Err(Error::Format);}
            Some(ReservedSource{operation:response[145..161].try_into().unwrap(),subject:response[161..177].try_into().unwrap(),group:response[177..193].try_into().unwrap(),revision:word(response,193),state:if response[201]==0{State::Reserved}else{State::Draining}})
        }else{None};
        Ok(Self{entry})
    }
    pub fn entry(&self)->Option<ReservedSource>{self.entry}
    /// Appeler seulement avec une origine calculée par la politique fraîche.
    /// Direct n'est pas représenté par un identifiant de groupe arbitraire.
    pub fn blocks_group(&self,subject:&[u8;16],group:&[u8;16],revision:u64)->bool {
        self.entry.is_some_and(|e|e.subject==*subject && e.group==*group && e.revision==revision)
    }
}

#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub enum OperationOutcome {Pending,Completed([u8;32]),Cancelled}
/// La preuve d'une issue individuelle n'est ni une permission ni une mutation.
pub fn operation_outcome(key:&VerifyingKey,query:&[u8],original:&[u8],response:&[u8])->Result<OperationOutcome> {
    Message::parse(QUERY,query)?;Message::parse(ADMIT,original)?;
    if response.len()!=522 || &response[..8]!=b"NAMBOU01" || response[8..168]!=*query
        || response[168..409]!=*original || original[..32]!=query[..32]
        || original[185..201]!=query[112..128] {return Err(Error::Scope);}
    const P:&[u8]=b"NovaAuth/QEMU/membership/query/v1\0";
    let mut signed=[0;P.len()+458];signed[..P.len()].copy_from_slice(P);signed[P.len()..].copy_from_slice(&response[..458]);
    key.verify_strict(&signed,&Signature::from_slice(&response[458..]).map_err(|_|Error::Crypto)?).map_err(|_|Error::Crypto)?;
    let admitted=word(response,409);let terminal=word(response,450);let evidence=response[418..450].try_into().unwrap();
    if admitted==0 || admitted>=1<<63 || terminal>=1<<63 {return Err(Error::Order);}
    match response[417] {
        0 if terminal==0 && evidence==[0;32]=>Ok(OperationOutcome::Pending),
        1 if terminal>admitted && evidence!=[0;32]=>Ok(OperationOutcome::Completed(evidence)),
        2 if terminal>admitted && evidence==[0;32]=>Ok(OperationOutcome::Cancelled),
        _=>Err(Error::Order),
    }
}


/// Historique privé additif, sans modification des reçus QUERY historiques.
/// Le principal et les effecteurs sont ceux de la réservation admise, jamais
/// des valeurs proposées par le CLI pendant la récupération.
pub struct History<'a> { original:&'a[u8], receipt:Receipt }
impl<'a> History<'a> {
    pub fn verify(key:&VerifyingKey,query:&[u8],response:&'a[u8])->Result<Self> {
        Message::parse(HISTORY,query)?;
        if response.len()<170+258+RECEIPT+64 || response.len()>170+MAX_REQUEST+RECEIPT+64
            || &response[..8]!=b"NAMBHI01" || response[8..168]!=*query {return Err(Error::Format);}
        let size=usize::from(u16::from_be_bytes(response[168..170].try_into().unwrap()));
        if response.len()!=170+size+RECEIPT+64{return Err(Error::Format);}
        const P:&[u8]=b"NovaAuth/QEMU/membership/history/v1\0";
        let end=response.len()-64;let mut signed=[0;crate::framing::MAX];
        signed[..P.len()].copy_from_slice(P);signed[P.len()..P.len()+end].copy_from_slice(&response[..end]);
        key.verify_strict(&signed[..P.len()+end],&Signature::from_slice(&response[end..]).map_err(|_|Error::Crypto)?).map_err(|_|Error::Crypto)?;
        let original=&response[170..170+size];Message::parse(RESERVE,original)?;
        if original[..32]!=query[..32] || original[112..128]!=query[112..128]{return Err(Error::Scope);}
        let expected=original[225..].chunks_exact(33).fold(0,|v,e|v|e[0]);
        let binding=Binding{server:original[..16].try_into().unwrap(),domain:original[16..32].try_into().unwrap(),
            incarnation:original[32..64].try_into().unwrap(),operation:original[112..128].try_into().unwrap(),
            subject:original[184..200].try_into().unwrap(),group:original[200..216].try_into().unwrap(),
            revision:word(original,216),generation:word(original,72).checked_add(1).ok_or(Error::Order)?,
            previous:original[80..112].try_into().unwrap(),published:original[128..160].try_into().unwrap(),expected};
        let receipt=Receipt::verify(key,&response[170+size..end],&binding,query[128..].try_into().unwrap())?;
        Ok(Self{original,receipt})
    }
    pub fn original(&self)->&[u8]{self.original}
    pub fn principal(&self)->[u8;16]{self.original[160..176].try_into().unwrap()}
    pub fn binding(&self)->&Binding{&self.receipt.binding}
    pub fn receipt(&self)->&Receipt{&self.receipt}
}
const _:()=assert!(170+MAX_REQUEST+RECEIPT+64+64+crate::framing::HEADER<=crate::framing::MAX);

/// Retrouver les octets originaux depuis une réponse signée, en les liant à
/// l'empreinte conservée AVANT émission. Aucun nouvel ADMIT n'est produit.
pub fn recovered_operation<'a>(key:&VerifyingKey,query:&[u8],expected:&[u8;32],response:&'a[u8])
    ->Result<(OperationOutcome,&'a[u8])> {
    if response.len()!=522 || *expected==[0;32]{return Err(Error::Format);}
    let original=&response[168..409];
    let state=operation_outcome(key,query,original,response)?;
    if crate::hash(original)!=*expected{return Err(Error::Scope);}
    Ok((state,original))
}
