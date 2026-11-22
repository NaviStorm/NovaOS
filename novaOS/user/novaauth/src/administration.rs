//! Port privé de conception R1/R2 : aucune opération destructive exposée.
//! Les plans réutilisent State/apply ; ils ne publient rien et ne constituent
//! jamais un reçu d'autorité ou un acquittement de révocation.
use super::*;
use crate::{recuperation::Triplet, recuperation_view::View as RegistryView};
use ed25519_dalek::{Signature, VerifyingKey};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Retirement {
    pub epoch: u64,
    pub request: Id,
    pub generation: u64,
    pub previous: Hash,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Authentication {
    pub epoch: u64,
    pub retirement: Option<Retirement>,
}
#[derive(Clone, Copy)]
pub enum Selector<'a> {
    Id(Id),
    Name(&'a str),
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserInfo {
    pub id: Id,
    pub name: String,
    pub status: Status,
    pub revision: u64,
    pub administrator: bool,
    pub groups: Vec<Id>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupInfo {
    pub id: Id,
    pub name: String,
    pub members: Vec<Id>,
}

/// Le facteur a été attesté séparément par le provisionneur. Cette référence
/// publique n'est pas une preuve de possession ni une capacité administrative.
pub enum Operation<'a> {
    Create {
        name: &'a str,
        factor: Id,
        group: Id,
    },
    Rename {
        subject: Id,
        name: &'a str,
    },
    Disable {
        subject: Id,
    },
    Reactivate {
        subject: Id,
    },
    CreateGroup {
        name: &'a str,
    },
    AddMember {
        subject: Id,
        group: Id,
    },
    RemoveMember {
        subject: Id,
        group: Id,
    },
}

/// Métadonnées dérivées du même plan que les trois objets, jamais des champs
/// fournis par l'ELF. L'autorité ferme les époques dans la transaction de tête.
pub struct Publication {
    pub triplet: Triplet,
    pub metadata: [u8; 49],
}

pub const RETIRED_PREFIX: &[u8] = b"NovaAuth/QEMU/subject-retired/v1\0";
/// Reçu terminal authentifié, distinct d'un retrait demandé ou en cours.
/// Le service choisit la clé et le serveur attendus depuis sa configuration de
/// confiance. Ils ne doivent jamais être pris dans la requête du client.
pub struct Retired {
    domain: Id,
    subject: Id,
    retirement: Retirement,
}
impl Retired {
    /// Corps canonique 128 octets, entiers BE ; signature Ed25519 externe 64.
    /// 0 serveur,16 domaine,32 sujet,48 époque,56 requête,72 génération,
    /// 80 position durable,88 état terminal (=1),89..96 réservés,96 tête précédente.
    pub fn verify(
        expected_server: Id,
        key: &VerifyingKey,
        body: &[u8],
        signature: &[u8],
    ) -> Result<Self> {
        if body.len() != 128
            || signature.len() != 64
            || expected_server == [0; 16]
            || body[..16] != expected_server
            || body[88] != 1
            || body[89..96] != [0; 7]
        {
            return Err(Fault::Denied);
        }
        let word = |i| u64::from_be_bytes(body[i..i + 8].try_into().unwrap());
        if word(48) == 0
            || word(72) == 0
            || word(80) == 0
            || body[16..32] == [0; 16]
            || body[32..48] == [0; 16]
            || body[56..72] == [0; 16]
            || body[96..128] == [0; 32]
        {
            return Err(Fault::Denied);
        }
        let mut signed = [0; RETIRED_PREFIX.len() + 128];
        signed[..RETIRED_PREFIX.len()].copy_from_slice(RETIRED_PREFIX);
        signed[RETIRED_PREFIX.len()..].copy_from_slice(body);
        key.verify_strict(
            &signed,
            &Signature::from_slice(signature).map_err(|_| Fault::Denied)?,
        )
        .map_err(|_| Fault::Denied)?;
        Ok(Self {
            domain: body[16..32].try_into().unwrap(),
            subject: body[32..48].try_into().unwrap(),
            retirement: Retirement {
                epoch: word(48),
                request: body[56..72].try_into().unwrap(),
                generation: word(72),
                previous: body[96..128].try_into().unwrap(),
            },
        })
    }
}
impl RegistryView {
    fn admin_reader(&self, grant: Grant) -> Result<&User> {
        if grant.head != self.head {
            return Err(Fault::Conflict);
        }
        let user = self.state.user(grant.owner)?;
        if user.status != Status::Active {
            return Err(Fault::Denied);
        }
        Ok(user)
    }
    pub fn authentication_epoch(&self, grant: Grant) -> Result<u64> {
        let user = self.admin_reader(grant)?;
        let a = user.authentication.as_ref().ok_or(Fault::Denied)?;
        if a.epoch == 0 || a.retirement.is_some() {
            return Err(Fault::Denied);
        }
        Ok(a.epoch)
    }
    /// Identifiant public du retrait à consulter auprès de l'autorité. Ce
    /// getter ne constitue ni un reçu ni une autorisation de réactivation.
    pub fn retirement_request(&self, grant: Grant, subject: Id) -> Result<Id> {
        if !self.admin_reader(grant)?.admin { return Err(Fault::Denied); }
        let user = self.state.user(subject)?;
        if user.status != Status::Disabled { return Err(Fault::Denied); }
        user.authentication.as_ref().and_then(|a| a.retirement.as_ref())
            .map(|r| r.request).ok_or(Fault::Denied)
    }
    pub fn user_info(&self, grant: Grant, selector: Option<Selector<'_>>) -> Result<UserInfo> {
        let caller = self.admin_reader(grant)?;
        let found = self
            .state
            .users
            .iter()
            .find(|u| match selector {
                None => u.id == caller.id,
                Some(Selector::Id(id)) => u.id == id,
                Some(Selector::Name(name)) => u.name == name && u.status != Status::Deleted,
            })
            .ok_or(Fault::Denied)?;
        if found.id != caller.id && !caller.admin {
            return Err(Fault::Denied);
        }
        Ok(UserInfo {
            id: found.id,
            name: found.name.clone(),
            status: found.status,
            revision: found.revision,
            administrator: found.admin,
            groups: self
                .state
                .groups
                .iter()
                .filter(|g| g.members.contains(&found.id))
                .map(|g| g.id)
                .collect(),
        })
    }
    pub fn group_info(&self, grant: Grant, selector: Selector<'_>) -> Result<GroupInfo> {
        let caller = self.admin_reader(grant)?;
        let group = self
            .state
            .groups
            .iter()
            .find(|g| match selector {
                Selector::Id(id) => g.id == id,
                Selector::Name(name) => g.name == name,
            })
            .ok_or(Fault::Denied)?;
        if !caller.admin && !group.members.contains(&caller.id) {
            return Err(Fault::Denied);
        }
        Ok(GroupInfo {
            id: group.id,
            name: group.name.clone(),
            members: if caller.admin {
                group.members.clone()
            } else {
                alloc::vec![caller.id]
            },
        })
    }
    pub fn administration_publication(
        &self,
        grant: Grant,
        request: Id,
        operation: Operation<'_>,
        retired: Option<&Retired>,
    ) -> Result<Publication> {
        let principal_epoch = self.authentication_epoch(grant)?;
        let transition = match &operation {
            Operation::Create { .. } => (1, [0; 16], 0),
            Operation::Disable { subject } | Operation::Reactivate { subject } => {
                let auth = self
                    .state
                    .user(*subject)?
                    .authentication
                    .as_ref()
                    .ok_or(Fault::Denied)?;
                (
                    if matches!(&operation, Operation::Disable { .. }) {
                        2
                    } else {
                        3
                    },
                    *subject,
                    auth.epoch,
                )
            }
            _ => (4, [0; 16], 0),
        };
        // Contrôles du rôle, du dernier administrateur, de la tête et du reçu
        // terminal dans le plan existant. Aucun octet n'est publié ici.
        let triplet = self.administration_plan(grant, request, operation, retired)?;
        let subject = if transition.0 == 1 {
            created_user(triplet.head())
        } else {
            transition.1
        };
        let mut metadata = [0; 49];
        metadata[..16].copy_from_slice(&grant.subject());
        metadata[16..24].copy_from_slice(&principal_epoch.to_be_bytes());
        metadata[24..40].copy_from_slice(&subject);
        metadata[40..48].copy_from_slice(&transition.2.to_be_bytes());
        metadata[48] = transition.0;
        Ok(Publication { triplet, metadata })
    }

    /// Préparation privée sur une tête exacte. L'appelant vérifie également les
    /// portées, la session et le facteur attesté ; publication via NovaFS seulement.
    pub fn administration_plan(
        &self,
        grant: Grant,
        request: Id,
        operation: Operation<'_>,
        retired: Option<&Retired>,
    ) -> Result<Triplet> {
        let caller = self.admin_reader(grant)?;
        if !caller.admin || request == [0; 16] {
            return Err(Fault::Denied);
        }
        if request == self.head.request {
            return Err(Fault::Conflict);
        }
        let next = self.head.generation.checked_add(1).ok_or(Fault::Limit)?;
        let mut state = State {
            users: self.state.users.clone(),
            groups: self.state.groups.clone(),
            credentials: self.state.credentials.clone(),
        };
        let change = match operation {
            Operation::Create {
                name,
                factor,
                group,
            } => Some(Change::Create {
                name,
                credential: factor,
                group,
            }),
            Operation::Rename { subject, name } => Some(Change::Rename(subject, name)),
            Operation::Disable { subject } => Some(Change::Disable(subject)),
            Operation::CreateGroup { name } => Some(Change::AddGroup(name)),
            Operation::AddMember { subject, group } => Some(Change::AddMember(subject, group)),
            Operation::RemoveMember { subject, group } => {
                Some(Change::RemoveMember(subject, group))
            }
            Operation::Reactivate { subject } => {
                let user = state
                    .users
                    .iter_mut()
                    .find(|u| u.id == subject)
                    .ok_or(Fault::Denied)?;
                if user.status != Status::Disabled || user.admin {
                    return Err(Fault::Denied);
                }
                let auth = user.authentication.as_mut().ok_or(Fault::Denied)?;
                let expected = auth.retirement.as_ref().ok_or(Fault::Denied)?;
                let proof = retired.ok_or(Fault::Denied)?;
                if proof.domain != self.head.domain
                    || proof.subject != subject
                    || &proof.retirement != expected
                    || next <= expected.epoch
                {
                    return Err(Fault::Denied);
                }
                auth.epoch = next;
                auth.retirement = None;
                user.status = Status::Active;
                user.revision = next;
                None
            }
        };
        if let Some(change) = change {
            state.apply(&self.head, change, next, request)?;
        }
        Triplet::from_state(
            Head {
                generation: next,
                request,
                ..self.head
            },
            state,
        )
    }
}

/// Portée applicative du canal, distincte du rôle administratif du titulaire.
/// Le provisionnement ne modifie aucun droit du compte et ne donne aucun
/// accès fichier. Une opération exige l'intersection des deux autorisations.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Scope(u8);
impl Scope {
    pub const SELF: u8 = 1;
    pub const DIRECTORY: u8 = 2;
    pub const USERS: u8 = 4;
    pub const GROUPS: u8 = 8;
    pub fn parse(bits: u8) -> Result<Self> {
        if bits == 0 || bits & !15 != 0 {
            return Err(Fault::BadInput);
        }
        Ok(Self(bits))
    }
    pub fn bits(self) -> u8 {
        self.0
    }
}
#[derive(Clone, Copy)]
pub enum AdministrativeAction {
    SelfRead,
    DirectoryRead,
    Users,
    Groups,
}
impl RegistryView {
    /// La portée mémorisée par un canal n'est pas un droit pérenne : relire la
    /// vue fraîche pour chaque requête, y compris les consultations d'issue.
    pub fn administrative_scope(&self, grant: Grant, image: crate::policy::Image) -> Result<Scope> {
        self.authentication_epoch(grant)?;
        Scope::parse(self.state.administrative_scope(grant.owner, image)?)
    }
    pub fn administrative_scope_optional(
        &self,
        grant: Grant,
        image: crate::policy::Image,
    ) -> Result<Option<Scope>> {
        self.authentication_epoch(grant)?;
        self.state
            .administrative_scope_optional(grant.owner, image)?
            .map(Scope::parse)
            .transpose()
    }
    pub fn authorize_administrative(
        &self,
        grant: Grant,
        image: crate::policy::Image,
        issued_scope: Scope,
        action: AdministrativeAction,
    ) -> Result<()> {
        let actual = self.administrative_scope(grant, image)?;
        let bit = match action {
            AdministrativeAction::SelfRead => Scope::SELF,
            AdministrativeAction::DirectoryRead => Scope::DIRECTORY,
            AdministrativeAction::Users => Scope::USERS,
            AdministrativeAction::Groups => Scope::GROUPS,
        };
        if actual.bits() & issued_scope.bits() & bit == 0
            || (bit != Scope::SELF && !self.admin_reader(grant)?.admin)
        {
            return Err(Fault::Denied);
        }
        Ok(())
    }
}
