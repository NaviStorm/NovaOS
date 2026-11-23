//! Contrat privé du profil de qualification. Les identités d'installation et règles
//! font partie du même état décidé que les utilisateurs et les appartenances.
use super::*;

pub const READ: u8 = 1;
pub const WRITE: u8 = 2;
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct Installation {
    pub(super) id: Id,
    pub(super) image: Option<Hash>,
    revision: u64,
    #[cfg(feature="administration-lab")]
    #[serde(default,skip_serializing_if="Option::is_none")]
    administration: Option<u8>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Origin {
    Direct,
    Group(Id),
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub(super) struct Rule {
    installation: Id,
    object: u64,
    origin: Origin,
    rights: u8,
    revision: u64,
}
/// Attestation privée fournie par le lanceur sur l'image immuable passée au chargeur.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Image {
    pub installation: Id,
    pub digest: Hash,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Permission {
    pub origin: Origin,
    pub rights: u8,
    pub rule_revision: u64,
    pub membership_revision: u64,
    pub image_revision: u64,
}
pub enum InstallChange {
    New(Hash),
    Update(Id, Hash),
    Remove(Id),
}
pub fn installed(h: &Head) -> Id {
    identity(h, h.generation, h.request, 3)
}

impl State {
    fn installation(&self, id: Id) -> Result<&Installation> {
        self.users
            .iter()
            .filter(|u| u.status == Status::Active)
            .flat_map(|u| &u.installations)
            .find(|i| i.id == id && i.image.is_some())
            .ok_or(Fault::Denied)
    }
    pub(super) fn change_install(
        &mut self,
        h: &Head,
        owner: Id,
        change: InstallChange,
        generation: u64,
        request: Id,
    ) -> Result<()> {
        let new_id = identity(h, generation, request, 3);
        if self
            .users
            .iter()
            .flat_map(|u| &u.installations)
            .any(|i| i.id == new_id)
        {
            return Err(Fault::Conflict);
        }
        let u = self
            .users
            .iter_mut()
            .find(|u| u.id == owner && u.status == Status::Active)
            .ok_or(Fault::Denied)?;
        match change {
            InstallChange::New(image)
                if image != [0; 32] && u.installations.len() < MAX_SUBJECTS =>
            {
                u.installations.push(Installation {
                    id: new_id,
                    image: Some(image),
                    revision: generation,
                    #[cfg(feature="administration-lab")]
                    administration: None,
                });
            }
            InstallChange::Update(id, image) if image != [0; 32] => {
                let i = u
                    .installations
                    .iter_mut()
                    .find(|i| i.id == id && i.image.is_some())
                    .ok_or(Fault::Denied)?;
                // Une image différente ne reçoit pas le canal de l'ancienne.
                #[cfg(feature="administration-lab")]
                { i.administration = None; }
                i.image = Some(image);
                i.revision = generation;
            }
            InstallChange::Remove(id) => {
                let i = u
                    .installations
                    .iter_mut()
                    .find(|i| i.id == id && i.image.is_some())
                    .ok_or(Fault::Denied)?;
                i.image = None;
                i.revision = generation;
            }
            _ => return Err(Fault::BadInput),
        }
        Ok(())
    }
    pub(super) fn set_rule(
        &mut self,
        owner: Id,
        install: Id,
        object: u64,
        origin: Origin,
        rights: u8,
        generation: u64,
    ) -> Result<()> {
        if object == 0 || rights & !(READ | WRITE) != 0 {
            return Err(Fault::BadInput);
        }
        self.installation(install)?;
        if let Origin::Group(id) = origin {
            if !self.groups.iter().any(|g| g.id == id) {
                return Err(Fault::BadInput);
            }
        }
        let u = self
            .users
            .iter_mut()
            .find(|u| u.id == owner && u.status == Status::Active)
            .ok_or(Fault::Denied)?;
        if !u.folders.iter().any(|(oid, _)| *oid == object) {
            return Err(Fault::Denied);
        }
        let pos = u
            .rules
            .iter()
            .position(|r| r.installation == install && r.object == object && r.origin == origin);
        if let Some(pos) = pos {
            u.rules.remove(pos);
        }
        if rights != 0 {
            if u.rules.len() >= MAX_SUBJECTS {
                return Err(Fault::Limit);
            }
            u.rules.push(Rule {
                installation: install,
                object,
                origin,
                rights,
                revision: generation,
            });
        }
        Ok(())
    }
}
impl<D: Device, A: Authority> Registry<'_, D, A> {
    /// Appel exclusivement depuis le port privé du provisionneur après vérification
    /// du propriétaire et des octets proposés. Jamais distribué aux clients.
    pub fn provision_install(
        &mut self,
        expected: Head,
        request: Id,
        owner: Id,
        change: InstallChange,
    ) -> Result<Head> {
        let (h, mut s) = self.view()?;
        if expected != h {
            return Err(Fault::Conflict);
        }
        let generation = h.generation.checked_add(1).ok_or(Fault::Limit)?;
        s.change_install(&h, owner, change, generation, request)?;
        self.publish_state(h, s, request, generation)
    }
    pub fn bind_folder(
        &mut self,
        expected: Head,
        owner: Id,
        object: u64,
        seal: &[u8],
        request: Id,
    ) -> Result<Head> {
        let (h, mut s) = self.view()?;
        if h != expected {
            return Err(Fault::Conflict);
        }
        if object == 0
            || s.users
                .iter()
                .any(|u| u.folders.iter().any(|(o, _)| *o == object))
        {
            return Err(Fault::Conflict);
        }
        let u = s
            .users
            .iter_mut()
            .find(|u| u.id == owner && u.status == Status::Active)
            .ok_or(Fault::Denied)?;
        if u.folders.len() >= MAX_SUBJECTS {
            return Err(Fault::Limit);
        }
        u.folders.push((object, Sha256::digest(seal).into()));
        self.publish_state(
            h,
            s,
            request,
            h.generation.checked_add(1).ok_or(Fault::Limit)?,
        )
    }
    pub fn verify_folder(&self, owner: Id, object: u64, seal: &[u8]) -> Result<()> {
        let (_, s) = self.view()?;
        let expected: Hash = Sha256::digest(seal).into();
        if !s.user(owner)?.folders.contains(&(object, expected)) {
            return Err(Fault::Denied);
        }
        Ok(())
    }
    pub fn verify_image(&self, image: Image) -> Result<()> {
        let (_, s) = self.view()?;
        if s.installation(image.installation)?.image != Some(image.digest) {
            return Err(Fault::Denied);
        }
        Ok(())
    }
    /// L'effecteur résout lui-même l'OID et le propriétaire depuis l'ACL du volume.
    pub fn set_permission(
        &mut self,
        owner: Grant,
        folder_owner: Id,
        object: u64,
        install: Id,
        origin: Origin,
        rights: u8,
        request: Id,
    ) -> Result<Head> {
        let (h, mut s) = self.view()?;
        if owner.head != h {
            return Err(Fault::Conflict);
        }
        if owner.owner != folder_owner {
            return Err(Fault::Denied);
        }
        let generation = h.generation.checked_add(1).ok_or(Fault::Limit)?;
        s.set_rule(owner.owner, install, object, origin, rights, generation)?;
        self.publish_state(h, s, request, generation)
    }
    pub fn stored_permission(
        &self,
        owner: Grant,
        object: u64,
        install: Id,
        origin: Origin,
    ) -> Result<u8> {
        let (h, s) = self.view()?;
        if owner.head != h {
            return Err(Fault::Conflict);
        }
        s.installation(install)?;
        Ok(s.user(owner.owner)?
            .rules
            .iter()
            .find(|r| r.object == object && r.installation == install && r.origin == origin)
            .map_or(0, |r| r.rights))
    }
    pub fn permission(
        &self,
        principal: Id,
        image: Image,
        folder_owner: Id,
        object: u64,
        origin: Origin,
    ) -> Result<Permission> {
        let (_, s) = self.view()?;
        s.permission(principal, image, folder_owner, object, origin)
    }
}

#[cfg(feature="recuperation-lab")]
impl State {
    pub(super) fn authorize_installed_image(&self,owner:Id,id:Id,digest:Hash)->Result<()> {
        let user=self.user(owner)?;
        if user.status!=Status::Active || !user.installations.iter().any(|i|i.id==id && i.image==Some(digest)) {
            return Err(Fault::Denied);
        }
        Ok(())
    }
}

#[cfg(feature="administration-lab")]
impl State {
    pub(super) fn administrative_scope(&self,owner:Id,image:Image)->Result<u8> {
        self.authorize_installed_image(owner,image.installation,image.digest)?;
        self.administrative_scope_optional(owner,image)?.ok_or(Fault::Denied)
    }
    pub(super) fn administrative_scope_optional(&self,owner:Id,image:Image)->Result<Option<u8>> {
        self.authorize_installed_image(owner,image.installation,image.digest)?;
        let value=self.installation(image.installation)?.administration;
        if let Some(value)=value {crate::administration::Scope::parse(value)?;}
        Ok(value)
    }
}
#[cfg(feature="administration-lab")]
impl<D:Device,A:Authority> Registry<'_,D,A> {
    /// Port privé du provisionneur de qualification seulement. Ce choix est
    /// décidé avec le triplet, lié aux octets attestés et invalidé par Update.
    /// Aucun appel de cette méthode n'est exposé dans les commandes R1.
    pub fn provision_administrative_image(&mut self,expected:Head,request:Id,
        owner:Id,image:Image,scope:Option<crate::administration::Scope>)->Result<Head> {
        let (head,mut state)=self.view()?;
        if head!=expected {return Err(Fault::Conflict);}
        state.authorize_installed_image(owner,image.installation,image.digest)?;
        let item=state.users.iter_mut().find(|u|u.id==owner).ok_or(Fault::Denied)?
            .installations.iter_mut().find(|i|i.id==image.installation).ok_or(Fault::Denied)?;
        item.administration=scope.map(|s|s.bits());
        self.publish_state(head,state,request,head.generation.checked_add(1).ok_or(Fault::Limit)?)
    }
}

impl State {
    pub(super) fn permission(&self, principal: Id, image: Image, folder_owner: Id,
        object: u64, origin: Origin) -> Result<Permission> {
        if self.user(principal)?.status != Status::Active {
            return Err(Fault::Denied);
        }
        let installation = self.installation(image.installation)?;
        if installation.image != Some(image.digest) {
            return Err(Fault::Denied);
        }
        let owner = self.user(folder_owner)?;
        if owner.status != Status::Active {
            return Err(Fault::Denied);
        }
        let r = owner
            .rules
            .iter()
            .find(|r| {
                r.object == object && r.installation == image.installation && r.origin == origin
            })
            .ok_or(Fault::Denied)?;
        let membership_revision = match origin {
            Origin::Direct if principal == owner.id => 0,
            Origin::Direct => return Err(Fault::Denied),
            Origin::Group(id) => {
                let group = self
                    .groups
                    .iter()
                    .find(|g| g.id == id && g.members.contains(&principal))
                    .ok_or(Fault::Denied)?;
                group
                    .admissions
                    .iter()
                    .find(|(u, _)| *u == principal)
                    .map_or(0, |(_, rev)| *rev)
            }
        };
        Ok(Permission {
            origin,
            rights: r.rights,
            rule_revision: r.revision,
            membership_revision,
            image_revision: installation.revision,
        })
    }
}

#[cfg(feature="administration-lab")]
impl State {
    pub(super) fn installed_revision(&self,owner:Id,image:Image)->Result<u64>{
        self.authorize_installed_image(owner,image.installation,image.digest)?;
        Ok(self.installation(image.installation)?.revision)
    }
}
