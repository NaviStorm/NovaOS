//! Descripteur EXEC privé, lecture sans allocation. Aucun identifiant n'autorise
//! une opération : le registre revalide les permissions et l'autorité l'admet.
use crate::{Error,Result,hash,u64_at};
use ed25519_dalek::{Signature,VerifyingKey};
pub const HEADER:usize=512;
pub const DEPENDENCY:usize=72;
pub const MAX_DEPENDENCIES:usize=4;
pub const MAX_SIZE:usize=HEADER+MAX_DEPENDENCIES*DEPENDENCY;
pub const REGISTER:&[u8]=b"NovaAuth/QEMU/exec-descriptor/register/v1\0";
pub const LOOKUP:&[u8]=b"NovaAuth/QEMU/exec-descriptor/lookup/v1\0";
pub const ADMIT:&[u8]=b"NovaAuth/QEMU/exec-selective/admit/v1\0";
pub const RECEIPT:&[u8]=b"NovaAuth/QEMU/exec-descriptor/receipt/v1\0";

#[derive(Clone,Copy,Debug,Eq,PartialEq)]
pub struct Dependency<'a>(&'a [u8]);
impl<'a> Dependency<'a> {
 pub fn parse(p:&'a [u8])->Result<Self> {
  if p.len()!=DEPENDENCY || !matches!(p[0],1|2) || !matches!(p[1],1|2)
   || p[3..8]!=[0;5] || p[8..24]==[0;16] || u64_at(p,24)==0
   || [24,48,56,64].iter().any(|&i|u64_at(p,i)>=1<<63) {return Err(Error::Format);}
  if (p[1]==1 && (p[32..48]!=[0;16] || u64_at(p,48)!=0))
   || (p[1]==2 && p[32..48]==[0;16])
   || (p[0]==1 && (p[1]!=1 || p[2]!=0 || u64_at(p,56)!=0))
   || (p[0]==2 && !matches!(p[2],1..=3)) {return Err(Error::Scope);}
  Ok(Self(p))
 }
 pub fn bytes(self)->&'a [u8]{self.0}
 pub fn action(self)->u8{self.0[0]}
 pub fn origin(self)->u8{self.0[1]}
 pub fn rights(self)->u8{self.0[2]}
 pub fn owner(self)->[u8;16]{self.0[8..24].try_into().unwrap()}
 pub fn object(self)->u64{u64_at(self.0,24)}
 pub fn group(self)->[u8;16]{self.0[32..48].try_into().unwrap()}
 pub fn membership_revision(self)->u64{u64_at(self.0,48)}
 pub fn rule_revision(self)->u64{u64_at(self.0,56)}
 pub fn image_revision(self)->u64{u64_at(self.0,64)}
 fn key(self)->(u8,[u8;16],u64,u8,[u8;16]){(self.action(),self.owner(),self.object(),self.origin(),self.group())}
}
#[derive(Clone,Copy,Debug,Eq,PartialEq)]
pub struct Descriptor<'a>(&'a [u8]);
impl<'a> Descriptor<'a> {
 pub fn parse(p:&'a [u8])->Result<Self>{
  if p.len()<HEADER || &p[..8]!=b"NXEXDP01" {return Err(Error::Format);}
  let count=p[295] as usize;
  if !(1..=MAX_DEPENDENCIES).contains(&count) || p.len()!=HEADER+count*DEPENDENCY{return Err(Error::Limit);}
  for (at,n) in [(8,16),(24,16),(40,32),(72,32),(104,32),(136,16),(160,32),(192,32),(224,16),(240,16),(256,16),(272,16),(296,32),(328,32),(392,32),(432,32),(472,32)] {
   if p[at..at+n].iter().all(|&b|b==0){return Err(Error::Format);}
  }
  for at in [152,360,368,376,384,424,464] {
   let n=u64_at(p,at);
   if n>=1<<63 || (!matches!(at,368|376|424) && n==0){return Err(Error::Format);}
  }
  if !matches!(u16::from_be_bytes(p[292..294].try_into().unwrap()),62|183)
   || !matches!(p[294],1|2) || p[504]&!15!=0 || p[505..512]!=[0;7] || p[256..272]!=p[136..152]
   || u64_at(p,384)>4*1024*1024 || u64_at(p,376)+u64_at(p,384)>=1<<63 {return Err(Error::Scope);}
  let d=Self(p);let mut previous=None;
  for (i,item) in d.dependencies().enumerate(){
   let dep=Dependency::parse(item.bytes())?;
   if (i==0)!=(dep.action()==1) || (i==0 && (dep.owner()!=p[136..152] || dep.object()!=u64_at(p,360)))
    || [dep.membership_revision(),dep.rule_revision(),dep.image_revision()].iter().any(|&r|r>u64_at(p,424)) {return Err(Error::Scope);}
   if previous.is_some_and(|key|key>=dep.key()){return Err(Error::Order);}
   previous=Some(dep.key());
  }
  Ok(d)
 }
 pub fn bytes(self)->&'a [u8]{self.0}
 pub fn digest(self)->[u8;32]{hash(self.0)}
 pub fn dependencies(self)->impl Iterator<Item=Dependency<'a>>{self.0[HEADER..].chunks_exact(DEPENDENCY).map(Dependency)}
 pub fn operation(self)->[u8;16]{self.0[224..240].try_into().unwrap()}
 /// La tête initiale reste immuable. La requête porte séparément la tête fraîche.
 pub fn matches(self,q:&crate::subject_admission::Request)->Result<()> {
  let p=q.bytes();
  for (a,b,n) in [(8,8,64),(72,112,32),(160,144,32),(240,176,16),(296,192,32),(136,256,16),(152,272,8),(224,240,16)] {
   if self.0[a..a+n]!=p[b..b+n]{return Err(Error::Scope);}
  }
  if u64_at(p,72)<u64_at(self.0,424){return Err(Error::Scope);}
  Ok(())
 }
 pub fn verify_registration(self,key:&VerifyingKey,position:u64,signature:&[u8;64])->Result<()> {
  if position==0 || key.is_weak(){return Err(Error::Scope);}
  let mut message=[0;MAX_SIZE+80];let n=RECEIPT.len();
  message[..n].copy_from_slice(RECEIPT);message[n..n+8].copy_from_slice(&position.to_be_bytes());
  message[n+8..n+8+self.0.len()].copy_from_slice(self.0);
  key.verify_strict(&message[..n+8+self.0.len()],&Signature::from_bytes(signature)).map_err(|_|Error::Crypto)
 }
}

/// Fragments de 128 octets ; l'assemblage emprunte le buffer déjà possédé par
/// l'appelant. Aucune allocation ni état global ; une erreur exige l'abandon.
pub struct Assembly<'a>{buffer:&'a mut [u8],operation:[u8;16],digest:[u8;32],length:u16,received:u16}
impl<'a> Assembly<'a>{
 pub fn new(buffer:&'a mut [u8],operation:[u8;16],digest:[u8;32],length:usize)->Result<Self>{
  if operation==[0;16] || digest==[0;32] || !(HEADER+DEPENDENCY..=MAX_SIZE).contains(&length)
   || (length-HEADER)%DEPENDENCY!=0 || buffer.len()<length{return Err(Error::Limit);}
  buffer[..length].fill(0);
  Ok(Self{buffer,operation,digest,length:length as u16,received:0})
 }
 pub fn push(&mut self,p:&[u8;128])->Result<bool>{
  if self.received&0x8000!=0{return Err(Error::Order);}
  let result=self.push_checked(p);
  if result.is_err(){self.received|=0x8000;}
  result
 }
 fn push_checked(&mut self,p:&[u8;128])->Result<bool>{
  let index=u16::from_be_bytes(p[56..58].try_into().unwrap()) as usize;
  let count=u16::from_be_bytes(p[58..60].try_into().unwrap()) as usize;
  let length=u16::from_be_bytes(p[60..62].try_into().unwrap()) as usize;
  let part=u16::from_be_bytes(p[62..64].try_into().unwrap()) as usize;
  if &p[..8]!=b"NXEXFR01" || p[8..24]!=self.operation || p[24..56]!=self.digest
   || length!=self.length as usize || count!=length.div_ceil(64) || index>=count
   || part!=(length-index*64).min(64) || p[64+part..].iter().any(|&b|b!=0){return Err(Error::Format);}
  let dst=&mut self.buffer[index*64..index*64+part];
  if self.received&(1<<index)!=0 && dst!=&p[64..64+part]{return Err(Error::Duplicate);}
  dst.copy_from_slice(&p[64..64+part]);self.received|=1<<index;
  let complete=self.received==(1<<count)-1;
  if complete {self.descriptor()?;}
  Ok(complete)
 }
 pub fn descriptor(&self)->Result<Descriptor<'_>>{
  if self.received!=(1<<(self.length as usize).div_ceil(64))-1{return Err(Error::Missing);}
  let p=&self.buffer[..self.length as usize];
  if hash(p)!=self.digest{return Err(Error::Crypto);}
  let d=Descriptor::parse(p)?;
  if d.operation()!=self.operation{return Err(Error::Scope);}
  Ok(d)
 }
}
pub fn fragment(d:Descriptor<'_>,index:usize)->Result<[u8;128]>{
 let bytes=d.bytes();let count=bytes.len().div_ceil(64);
 if index>=count{return Err(Error::Limit);}
 let part=(bytes.len()-index*64).min(64);let mut p=[0;128];p[..8].copy_from_slice(b"NXEXFR01");
 p[8..24].copy_from_slice(&d.operation());p[24..56].copy_from_slice(&d.digest());
 for (at,n) in [(56,index),(58,count),(60,bytes.len()),(62,part)]{p[at..at+2].copy_from_slice(&(n as u16).to_be_bytes());}
 p[64..64+part].copy_from_slice(&bytes[index*64..index*64+part]);Ok(p)
}

/// Consultation fraîche d'un descripteur HISTORIQUE. Ce reçu ne constitue
/// jamais une admission et ne remplace ni View ni la barrière de révocation.
/// La nouvelle incarnation peut consulter l'histoire sans reprendre ses droits.
pub fn verify_lookup<'a>(key:&VerifyingKey,query:&[u8;112],reply:&'a [u8])->Result<(u64,Descriptor<'a>)>{
 if key.is_weak() || query[80..]==[0;32] || reply.len()<120+HEADER+DEPENDENCY+128
  || reply.len()>120+MAX_SIZE+128 || reply[..112]!=*query{return Err(Error::Scope);}
 let end=reply.len()-64;let position=u64_at(reply,112);
 if position==0{return Err(Error::Format);}
 let d=Descriptor::parse(&reply[120..end-64])?;
 if d.bytes()[8..40]!=query[..32] || d.operation()!=query[64..80]{return Err(Error::Scope);}
 let mut signed=[0;MAX_SIZE+320];let n=LOOKUP.len();
 signed[..n].copy_from_slice(LOOKUP);signed[n..n+end].copy_from_slice(&reply[..end]);
 key.verify_strict(&signed[..n+end],&Signature::from_slice(&reply[end..]).map_err(|_|Error::Format)?)
  .map_err(|_|Error::Crypto)?;
 Ok((position,d))
}

impl<'a> Descriptor<'a> {
 /// Métadonnées immuables de la porte privée, lues par le registre. La validité
 /// temporelle reste vérifiée sous le verrou noyau. Cette voie couvre le rôle
 /// simple et au plus une portée de lecture sur le dossier scellé du titulaire.
 /// La permission et l'empreinte du sceau restent à vérifier dans la vue P9.
 pub fn matches_launch_metadata(self,m:&[u64;48])->Result<Option<Dependency<'a>>> {
  fn bytes<const N:usize>(words:&[u64])->[u8;N]{let mut out=[0;N];for(w,b)in words.iter().zip(out.chunks_exact_mut(8)){b.copy_from_slice(&w.to_le_bytes());}out}
  let p=self.bytes();
  if m[35]!=1 || p[294]!=1
   || p[72..104]!=bytes::<32>(&m[4..8]) || p[104..136]!=bytes::<32>(&m[43..47])
   || p[136..152]!=bytes::<16>(&m[29..31]) || u64_at(p,152)!=m[31]
   || p[160..192]!=bytes::<32>(&m[8..12]) || p[192..224]!=p[72..104]
   || p[240..256]!=bytes::<16>(&m[12..14]) || p[296..328]!=bytes::<32>(&m[14..18])
   || p[328..344]!=bytes::<16>(&m[40..42])
   || u64_at(p,360)!=m[36] || u64_at(p,368)!=m[37] || u64_at(p,384)!=m[38]
   || u64_at(p,424)!=m[39] || p[432..464]!=bytes::<32>(&m[18..22])
   || u64_at(p,464)!=m[3] || p[472..504]!=p[72..104] || u64::from(p[504])!=m[47]
   || (m[26..28]!=[0;2] && self.operation()!=bytes::<16>(&m[26..28])) {return Err(Error::Scope);}

  let mut files=self.dependencies().skip(1);
  let dependency=files.next();
  let [directory,file,cwd]=m[32..35].try_into().unwrap();
  match dependency {
   None if [directory,file,cwd]==[0;3]=>Ok(None),
   Some(dep) if files.next().is_none() && dep.owner()==p[136..152]
    && dep.object()==directory && dep.rights()==1 && directory!=file
    && (1..1<<63).contains(&file) && cwd==directory && p[504]==0=>Ok(Some(dep)),
   _=>Err(Error::Scope),
  }
 }
}
