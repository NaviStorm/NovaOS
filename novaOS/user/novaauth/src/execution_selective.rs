//! Décision fraîche de politique EXEC, sans accès au stockage ni admission.
//! Les valeurs attendues viennent du descripteur authentifié immuable. Le
//! raccord vérifie séparément sa signature, ses liaisons noyau et la tête.
use crate::{*,policy::{Image,Origin},recuperation_view::View,distribution::{Machine,ExecutionIdentity}};
use ed25519_dalek::VerifyingKey;
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct RequiredPermission {
 pub owner:Id,pub object:u64,pub origin:Origin,pub rights:u8,
 pub membership_revision:u64,pub rule_revision:u64,pub image_revision:u64,
}
impl View {
 pub fn revalidate_execution_dependencies(&self,grant:Grant,epoch:u64,image:Image,
  expected:ExecutionIdentity,key:&VerifyingKey,machine:Machine,required:&[RequiredPermission])->Result<()> {
  // L'installation est une dépendance obligatoire ; au plus trois droits VFS
  // supplémentaires réellement délégués, jamais une liste choisie par le CLI.
  if required.len()>3{return Err(Fault::Limit);}
  if grant.head!=self.head || self.authentication_epoch(grant)?!=epoch
   || self.execution_identity(grant,image,key,machine)?!=expected {return Err(Fault::Denied);}
  for (i,need) in required.iter().enumerate(){
   if need.rights==0 || need.rights&!3!=0 || need.object==0{return Err(Fault::BadInput);}
   if required[..i].iter().any(|p|p.owner==need.owner&&p.object==need.object&&p.origin==need.origin){return Err(Fault::BadInput);}
   let p=self.permission(grant,image,need.owner,need.object,need.origin)?;
   if p.rights&need.rights!=need.rights || p.membership_revision!=need.membership_revision
    || p.rule_revision!=need.rule_revision || p.image_revision!=need.image_revision
    || p.image_revision!=expected.image_revision {return Err(Fault::Denied);}
  }
  Ok(())
 }
}

impl View {
 /// Décision de politique seulement. Le raccord doit avoir vérifié la session,
 /// l'instance et le descripteur signé ; cette fonction n'admet aucun processus.
 pub fn revalidate_execution_subject(&self,subject:Id,epoch:u64,image:Image,
  expected:ExecutionIdentity,key:&VerifyingKey,machine:Machine,required:&[RequiredPermission])->Result<()> {
  self.revalidate_execution_dependencies(Grant{owner:subject,head:self.head},epoch,image,expected,key,machine,required)
 }
}
