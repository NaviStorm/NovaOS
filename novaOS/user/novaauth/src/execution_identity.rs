//! Partie de distribution : une image courante ne remplace pas silencieusement
//! la version, l'objet ou les droits administratifs initiaux du ticket.
use super::*;
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct ExecutionIdentity {
 pub package:Id,pub version:u32,pub machine:u16,pub manifest:Hash,
 pub object:u64,pub revision:u64,pub offset:u64,pub size:u64,
 pub image_revision:u64,pub administration:u8,
}
impl crate::recuperation_view::View {
 pub fn execution_identity(&self,grant:Grant,image:crate::policy::Image,key:&VerifyingKey,machine:Machine)->super::super::Result<ExecutionIdentity>{
  if grant.head!=self.head{return Err(Fault::Conflict);}
  self.state.authorize_installed_image(grant.owner,image.installation,image.digest)?;
  let signer:Hash=Sha256::digest(key.as_bytes()).into();
  let mut found=self.state.user(grant.owner)?.packages.iter().filter(|p|p.active&&p.signer==signer&&p.machine==machine as u16)
   .flat_map(|p|p.entries.iter().filter(move |e|e.principal==Some(image.installation)&&e.digest==image.digest).map(move |e|(p,e)));
  let (p,e)=found.next().ok_or(Fault::Denied)?;
  if found.next().is_some(){return Err(Fault::Conflict);}
  Ok(ExecutionIdentity{package:p.package,version:p.version,machine:p.machine,manifest:p.manifest,object:p.object,revision:p.revision,
   offset:e.start,size:e.size as u64,image_revision:self.state.installed_revision(grant.owner,image)?,
   administration:self.state.administrative_scope_optional(grant.owner,image)?.unwrap_or(0)})
 }
}
