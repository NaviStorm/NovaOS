//! Domaines signés distincts ; l'enregistrement ne vaut pas admission.
use crate::{Error,Result,exec_descriptor as d,subject_admission::Request,exec_admission};
pub const REGISTER:u16=62;
pub const LOOKUP:u16=63;
pub const ADMIT:u16=64;
pub const CANCEL:u16=65;
pub const MAX_SIGNED:usize=d::MAX_SIZE+80;
pub struct Message<'a>{operation:u16,body:&'a[u8]}
impl<'a> Message<'a>{
 pub fn parse(operation:u16,body:&'a[u8])->Result<Self>{
  match operation {
   REGISTER=>{d::Descriptor::parse(body)?;},
   LOOKUP=>{if body.len()!=112 || [(0,16),(16,32),(32,64),(64,80),(80,112)].iter().any(|&(a,b)|body[a..b].iter().all(|&v|v==0)){return Err(Error::Format);}},
   ADMIT|CANCEL=>{if body.len()!=312 || body[280..]==[0;32]{return Err(Error::Format);}Request::parse(&body[..280])?;},
   _=>return Err(Error::Format),
  }
  Ok(Self{operation,body})
 }
 pub fn scope(&self,server:&[u8;16],domain:&[u8;16],instance:&[u8;32])->Result<()> {
  let at=if self.operation==LOOKUP{0}else{8};
  if self.body[at..at+16]!=*server || self.body[at+16..at+32]!=*domain || self.body[at+32..at+64]!=*instance{return Err(Error::Scope);}
  Ok(())
 }
 pub fn signed(&self,out:&mut[u8;MAX_SIGNED])->Result<usize>{
  let prefix=match self.operation{REGISTER=>d::REGISTER,LOOKUP=>d::LOOKUP,ADMIT=>d::ADMIT,CANCEL=>exec_admission::FINISH,_=>return Err(Error::Format)};
  out[..prefix.len()].copy_from_slice(prefix);
  if self.operation==CANCEL {
   let data=Request::parse(&self.body[..280])?.finish(2,&[0;32])?;
   out[prefix.len()..prefix.len()+65].copy_from_slice(&data);Ok(prefix.len()+65)
  }else {out[prefix.len()..prefix.len()+self.body.len()].copy_from_slice(self.body);Ok(prefix.len()+self.body.len())}
 }
}
