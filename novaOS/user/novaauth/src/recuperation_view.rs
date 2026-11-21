//! Vue de politique en mémoire du registre isolé. Aucun accès bloc ni écrivain.
//! La tête doit provenir de l'autorité authentifiée ; la lecture du triplet est
//! bornée et son empreinte est vérifiée avant de rendre la moindre décision.
use super::*;
use crate::snapshot_digest::SnapshotDigest;
pub struct View {pub(super) head:Head,pub(super) state:State}
impl View {
    pub fn read(head:Head,lengths:[u32;3],
                mut read:impl FnMut(usize,u32,&mut [u8])->Result<usize>)->Result<Self> {
        let bytes=crate::recuperation::Triplet::head_bytes(&head);
        let mut digest=SnapshotDigest::new(&bytes,lengths).map_err(|_|Fault::Limit)?;
        let mut state=State::default();
        // Une table sérialisée au plus en mémoire, pas trois copies du triplet.
        for table in 0..3 {
            let n=lengths[table] as usize;
            if n==0 || n>SLOT-53 {return Err(Fault::Limit);}
            let mut bytes=Vec::new();bytes.try_reserve_exact(n).map_err(|_|Fault::Limit)?;
            bytes.resize(n,0);
            let mut offset=0;
            while offset<n {
                let max=(n-offset).min(96);
                let got=read(table,offset as u32,&mut bytes[offset..offset+max])?;
                if got==0 || got>max {return Err(Fault::Corrupt);}
                digest.push(&bytes[offset..offset+got]).map_err(|_|Fault::StateMismatch)?;
                offset+=got;
            }
            match table {
                0=>state.users=serde_json::from_slice(&bytes).map_err(|_|Fault::Corrupt)?,
                1=>state.groups=serde_json::from_slice(&bytes).map_err(|_|Fault::Corrupt)?,
                2=>state.credentials=serde_json::from_slice(&bytes).map_err(|_|Fault::Corrupt)?,
                _=>return Err(Fault::BadInput)
            }
        }
        digest.finish().map_err(|_|Fault::StateMismatch)?;
        if state.users.len()>MAX_SUBJECTS || state.groups.len()>MAX_SUBJECTS ||
           state.credentials.len()>MAX_SUBJECTS {return Err(Fault::Limit);}
        Ok(Self {head,state})
    }
    pub fn head(&self)->Head {self.head}
    pub fn resolve_verified_factor(&self,reference:Id)->Result<Grant> {
        let owner=self.state.credentials.iter().find(|c|c.reference==reference).ok_or(Fault::Denied)?.owner;
        if self.state.user(owner)?.status!=Status::Active {return Err(Fault::Denied);}
        Ok(Grant {head:self.head,owner})
    }
    /// Décision de politique seulement. L'effecteur doit encore vérifier l'ACL,
    /// la surprotection, l'objet sélectionné et l'empreinte des octets du sceau.
    /// Un identifiant de dossier connu n'accorde aucun droit à son détenteur.
    pub fn folder(&self, grant:Grant, object:u64)->Result<Hash> {
        if grant.head!=self.head || object==0 {return Err(Fault::Denied);}
        let owner=self.state.user(grant.owner)?;
        if owner.status!=Status::Active {return Err(Fault::Denied);}
        owner.folders.iter().find(|(id,_)|*id==object)
            .map(|(_,seal)|*seal).ok_or(Fault::Denied)
    }
    /// Préparer un retrait demandé par le titulaire, sans l'activer en mémoire.
    /// Le retour n'est ni un reçu d'autorité ni un droit de publication.
    pub fn revoke_plan(&self,grant:Grant,object:u64,request:Id)->Result<crate::recuperation::Triplet> {
        self.folder(grant,object)?;
        if request==[0;16] || request==self.head.request {return Err(Fault::Conflict);}
        let mut state=State {users:self.state.users.clone(),groups:self.state.groups.clone(),
            credentials:self.state.credentials.clone()};
        let owner=state.users.iter_mut().find(|u|u.id==grant.owner).ok_or(Fault::Denied)?;
        owner.folders.retain(|(id,_)|*id!=object);
        let head=Head {generation:self.head.generation.checked_add(1).ok_or(Fault::Limit)?,request,..self.head};
        crate::recuperation::Triplet::from_state(head,state)
    }
    pub fn groups(&self,grant:Grant)->Result<usize> {
        if grant.head!=self.head {return Err(Fault::Denied);}
        Ok(self.state.groups.iter().filter(|g|g.members.contains(&grant.owner)).count())
    }
}

/// Empreinte de liaison ; ce calcul ne constitue pas une décision de droits.
pub fn seal_digest(bytes:&[u8])->Hash {Sha256::digest(bytes).into()}
