//! Permission fraîche, admission de groupe et faits VFS sur le port privé existant.
use crate::{common,incarnation,membership_transaction as tx,recovery_store::Storage,runtime as rt,tete};
use novaos_identity::{recuperation_view::View,policy::Origin};
use novaos_recovery::{membership as m,framing::{MAX,HEADER}};
use ed25519_dalek::VerifyingKey;
use sha2::{Digest,Sha256};
#[derive(Default)]
pub struct Io {context:Option<[u8;112]>,uncertain:bool}
fn key()->rt::Result<VerifyingKey>{VerifyingKey::from_bytes(include_bytes!(env!("NOVA_DECISION_PUB"))).map_err(|_|rt::REFUSED)}
fn outcome(original:&[u8;241])->rt::Result<(m::OperationOutcome,u64)>{
 let mut q=[0;160];q[..112].copy_from_slice(&tete::query(rt::config(8),&incarnation::identity())?);q[112..128].copy_from_slice(&original[185..201]);q[128..].copy_from_slice(&*common::random()?);
 let mut a=[0;MAX];let n=tx::exchange(m::QUERY,&q,&mut a)?;let b=&a[HEADER+8..HEADER+8+n];
 let v=m::operation_outcome(&key()?,&q,original,b).map_err(|_|rt::REFUSED)?;
 Ok((v,u64::from_be_bytes(b[409..417].try_into().unwrap())))
}
impl Io {
 pub fn accepts(q:&[u8;128])->bool{matches!(&q[..8],b"NXMGCTX1"|b"NXMGOP01")}
 pub fn request(&mut self,state:&View,storage:&mut Storage,q:&[u8;128],answer:&mut[u8;128])->rt::Result<()> {
  if self.uncertain{return Err(22);}
  if &q[..8]==b"NXMGCTX1" {
   self.context=None;if q[8..24]==[0;16] || q[24..56]==[0;32] || q[56..88]==[0;32] || q[88..120]==[0;32] || q[120..]!=[0;8]{return Err(rt::DENIED);}
   self.context=Some(q[8..120].try_into().unwrap());return Ok(());
  }
  let mut c=[0;128];c[..8].copy_from_slice(b"NXMGCTX1");c[8..120].copy_from_slice(&self.context.take().ok_or(rt::DENIED)?);
  if q[8..40]!=c[24..56] || !matches!(q[72],1|2) || !matches!(q[73],1|3) || q[74..88]!=[0;14] || q[96..]!=[0;32] || common::number(&q[88..])==0{return Err(rt::DENIED);}
  let current=tete::query(rt::config(8),&incarnation::identity())?;
  if current[72..80]!=state.head().generation.to_be_bytes() || current[80..]!=state.head().digest{return Err(rt::REVOKED);}
  let grant=state.resolve_verified_factor(c[8..24].try_into().unwrap()).map_err(|_|rt::REVOKED)?;
  let image=state.membership_image(grant,c[88..120].try_into().unwrap()).map_err(|_|rt::DENIED)?;
  let object=common::number(&q[40..]);let origin=if q[48..64]==[0;16]{Origin::Direct}else{Origin::Group(q[48..64].try_into().unwrap())};
  let permission=state.permission(grant,image,grant.subject(),object,origin);
  let seal=state.folder(grant,object).map_err(|_|rt::DENIED)?;
  let mut context=[0;128];context[..8].copy_from_slice(b"NXMGCTX2");context[8..24].copy_from_slice(&grant.subject());context[24..88].copy_from_slice(&c[24..88]);context[88..120].copy_from_slice(&seal);
  let mut go=[0;128];go[..8].copy_from_slice(b"NXMGGO01");go[8..16].copy_from_slice(&q[88..96]);go[16..24].copy_from_slice(&q[40..48]);go[24..32].copy_from_slice(&q[64..72]);go[32..48].copy_from_slice(&q[48..64]);go[72]=q[72];go[74]=q[73];
  let mut admitted=None;
  if let Ok(p)=permission {
   if p.rights&q[73]!=q[73]{return Err(rt::DENIED);}
   go[48..56].copy_from_slice(&p.rule_revision.to_le_bytes());go[56..64].copy_from_slice(&p.membership_revision.to_le_bytes());go[64..72].copy_from_slice(&p.image_revision.to_le_bytes());go[73]=p.rights;
   if let Origin::Group(group)=origin {
    let mut hasher=Sha256::new();hasher.update(b"NovaAuth/QEMU/vfs-operation/v1");hasher.update(c);hasher.update(q);let hash:[u8;32]=hasher.finalize().into();
    let mut a=[0;241];a[..112].copy_from_slice(&current);a[112..128].copy_from_slice(&grant.subject());a[128..144].copy_from_slice(&group);a[144..152].copy_from_slice(&p.membership_revision.to_be_bytes());a[152]=1;a[153..185].copy_from_slice(&storage.membership_instance()?);a[185..201].copy_from_slice(&hash[..16]);a[201..209].copy_from_slice(&state.authentication_epoch(grant).map_err(|_|rt::REVOKED)?.to_be_bytes());a[209..].copy_from_slice(&c[24..56]);
    self.uncertain=true;let mut response=[0;MAX];tx::exchange(m::ADMIT,&a,&mut response)?;
    let (state,position)=outcome(&a)?;if state!=m::OperationOutcome::Pending{return Err(22);}
    go[80..96].copy_from_slice(&a[185..201]);go[96..104].copy_from_slice(&position.to_le_bytes());admitted=Some(a);
   }
  }else{
   go[..8].copy_from_slice(b"NXMGDEN1");go[80..88].copy_from_slice(&rt::REVOKED.to_le_bytes());
  }
  self.uncertain=true;
  storage.membership_io(&context)?;
  let fact=storage.membership_io(&go)?;
  if &fact[8..16]!=b"NXMGIOA1" || fact[16..48]!=novaos_identity::recuperation_view::seal_digest(&go) || fact[48..80]!=storage.membership_instance()? || fact[80..112]==[0;32] || fact[120..]!=[0;8]{return Err(rt::REFUSED);}
  if common::number(&fact[112..])==22{return Err(22);}
  if let Some(a)=admitted {
   let mut f=[0;194];f[..112].copy_from_slice(&tete::query(rt::config(8),&incarnation::identity())?);f[112..128].copy_from_slice(&a[185..201]);f[128]=1;f[129..161].copy_from_slice(&a[153..185]);f[161]=1;f[162..].copy_from_slice(&fact[80..112]);let mut r=[0;MAX];tx::exchange(m::FINISH,&f,&mut r)?;
   if outcome(&a)?.0!=m::OperationOutcome::Completed(fact[80..112].try_into().unwrap()){return Err(22);}
  }
  self.uncertain=false;answer[8..16].copy_from_slice(&fact[112..120]);Ok(())
 }
}
