//! Provenance et préparation privées R2. Aucun acquittement d'effecteur implicite.
use crate::{administration::Operation, policy::{Image, Origin, Permission},
    recuperation::Triplet, recuperation_view::View, *};

/// Une origine est calculée dans la vue vérifiée, jamais attestée par le CLI.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Source { subject: Id, group: Id, revision: u64 }
impl Source {
    pub fn subject(self)->Id {self.subject}
    pub fn group(self)->Id {self.group}
    pub fn revision(self)->u64 {self.revision}
}
pub struct Removal {pub source:Source,pub triplet:Triplet,pub principal:Id,pub principal_epoch:u64}
impl View {
    fn source(&self,subject:Id,group:Id)->Result<Source> {
        self.state.user(subject)?;
        let g=self.state.groups.iter().find(|g|g.id==group && g.members.contains(&subject))
            .ok_or(Fault::Denied)?;
        let revision=g.admissions.iter().find(|(u,_)|*u==subject).map_or(0,|(_,r)|*r);
        Ok(Source{subject,group,revision})
    }
    /// Préparer sur la même tête que la capacité administrative authentifiée.
    /// Le triplet ne peut être confirmé via le reçu UPDATE historique : le
    /// raccord doit réserver l'origine puis fournir publication et drainage.
    pub fn membership_removal(&self,admin:Grant,op:Id,subject:Id,group:Id,
        expected_revision:u64)->Result<Removal> {
        let principal_epoch=self.authentication_epoch(admin)?;
        let triplet=self.administration_plan(admin,op,Operation::RemoveMember{subject,group},None)?;
        let source=self.source(subject,group)?;
        if source.revision!=expected_revision {return Err(Fault::Conflict);}
        Ok(Removal{source,triplet,principal:admin.subject(),principal_epoch})
    }
    /// Préparation sur la révision effectivement lue dans CETTE vue immuable.
    pub fn prepare_membership_removal(&self,admin:Grant,op:Id,subject:Id,group:Id)->Result<Removal> {
        let source=self.source(subject,group)?;
        self.membership_removal(admin,op,subject,group,source.revision)
    }
    /// Décision pour une image attestée par le lanceur et une session vérifiée.
    /// Le raccord applique aussi la barrière durable de cette origine, même si
    /// P9 correspond encore à la tête d'avant publication.
    pub fn permission(&self,grant:Grant,image:Image,owner:Id,object:u64,origin:Origin)->Result<Permission> {
        if grant.head!=self.head || object==0 {return Err(Fault::Denied);}
        self.state.permission(grant.owner,image,owner,object,origin)
    }
    pub fn revalidate_permission(&self,grant:Grant,image:Image,owner:Id,object:u64,
        previous:Permission)->Result<Permission> {
        let current=self.permission(grant,image,owner,object,previous.origin)?;
        if current!=previous {return Err(Fault::Denied);}
        Ok(current)
    }
}

impl View {
    /// Résoudre une image mesurée par le noyau sur le port privé du vérificateur.
    /// Aucune sélection par nom et aucune installation appartenant à un tiers.
    pub fn membership_image(&self,grant:Grant,digest:Hash)->Result<Image> {
        if grant.head!=self.head || digest==[0;32]{return Err(Fault::Denied);}
        let user=self.state.user(grant.owner)?;
        let mut images=user.installations.iter().filter(|i|i.image==Some(digest));
        let item=images.next().ok_or(Fault::Denied)?;
        if images.next().is_some(){return Err(Fault::Conflict);}
        Ok(Image{installation:item.id,digest})
    }
}
