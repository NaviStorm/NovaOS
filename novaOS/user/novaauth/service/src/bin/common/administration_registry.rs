//! R1 : route privée du vérificateur, jamais un port d'administration NovaFS.
//! Les champs de contexte proviennent des métadonnées noyau et d'une session
//! vérifiée. Une référence de facteur reçue directement d'un ELF serait invalide.
use crate::{common, incarnation, runtime as rt, tete};
#[path = "../../../../../../../components/exec-client/src/admin_command.rs"]
mod admin_command;
use admin_command::{Command, Read, Selector as WireSelector};
use novaos_identity::{
    Status,
    administration::{AdministrativeAction as Action, Scope, Selector},
    policy::Image,
    recuperation_view::View,
};
struct Context {
    reference: [u8; 16],
    head: [u8; 32],
    image: Image,
    scope: Scope,
    epoch: u64,
}
pub struct Port {
    context: Option<Context>,
}
impl Port {
    pub fn new() -> Self {
        Self { context: None }
    }
    pub fn clear(&mut self) {
        self.context = None;
    }
    pub fn accepts(q: &[u8; 128]) -> bool {
        matches!(&q[..8], b"NXASCP01" | b"NXACTX01" | b"NXACMD01")
    }
    pub fn request(
        &mut self,
        state: &View,
        q: &[u8; 128],
        answer: &mut [u8; 128],
    ) -> rt::Result<()> {
        let current = tete::query(rt::config(8), &incarnation::identity())?;
        if current[72..80] != state.head().generation.to_be_bytes()
            || current[80..112] != state.head().digest
        {
            self.clear();
            return Err(rt::REVOKED);
        }
        match &q[..8] {
            b"NXASCP01" => {
                self.clear();
                if q[104..] != [0; 24] || q[24..56] != state.head().digest {
                    return Err(rt::DENIED);
                }
                let grant = state
                    .resolve_verified_factor(q[8..24].try_into().unwrap())
                    .map_err(|_| rt::DENIED)?;
                let image = Image {
                    installation: q[56..72].try_into().unwrap(),
                    digest: q[72..104].try_into().unwrap(),
                };
                let scope = state
                    .administrative_scope_optional(grant, image)
                    .map_err(|_| rt::DENIED)?;
                answer[8..16]
                    .copy_from_slice(&u64::from(scope.map_or(0, Scope::bits)).to_le_bytes());
                answer[16..24].copy_from_slice(
                    &state
                        .authentication_epoch(grant)
                        .map_err(|_| rt::DENIED)?
                        .to_le_bytes(),
                );
                Ok(())
            }
            b"NXACTX01" => {
                self.clear();
                let scope = common::number(&q[104..]);
                if q[120..] != [0; 8] || q[24..56] != state.head().digest || scope > 15 {
                    return Err(rt::DENIED);
                }
                let c = Context {
                    reference: q[8..24].try_into().unwrap(),
                    head: q[24..56].try_into().unwrap(),
                    image: Image {
                        installation: q[56..72].try_into().unwrap(),
                        digest: q[72..104].try_into().unwrap(),
                    },
                    scope: Scope::parse(scope as u8).map_err(|_| rt::DENIED)?,
                    epoch: common::number(&q[112..]),
                };
                let grant = state
                    .resolve_verified_factor(c.reference)
                    .map_err(|_| rt::DENIED)?;
                let actual = state
                    .administrative_scope(grant, c.image)
                    .map_err(|_| rt::DENIED)?;
                if c.epoch != state.authentication_epoch(grant).map_err(|_| rt::DENIED)?
                    || c.scope.bits() & !actual.bits() != 0
                {
                    return Err(rt::REVOKED);
                }
                self.context = Some(c);
                Ok(())
            }
            b"NXACMD01" => {
                // Une seule demande possible par contexte ; une réponse perdue
                // ne remet ni contexte ni mutation en circulation.
                let c = self.context.take().ok_or(rt::DENIED)?;
                if c.head != state.head().digest {
                    return Err(rt::REVOKED);
                }
                let grant = state
                    .resolve_verified_factor(c.reference)
                    .map_err(|_| rt::DENIED)?;
                if c.epoch != state.authentication_epoch(grant).map_err(|_| rt::DENIED)? {
                    return Err(rt::REVOKED);
                }
                let cmd = admin_command::decode(q).map_err(|_| rt::BAD_ARGUMENT)?;
                let read = match cmd {
                    Command::Read(r) => r,
                    _ => return Err(rt::REFUSED),
                };
                // Les mutations restent refusées tant que R2 n'est pas raccordé.
                let action = match read {
                    Read::SelfIdentity => Action::SelfRead,
                    Read::User(WireSelector::Id(id)) | Read::UserGroup(WireSelector::Id(id), _)
                        if id == grant.subject() =>
                    {
                        Action::SelfRead
                    }
                    Read::User(s) | Read::UserGroup(s, _) => {
                        let info = state
                            .user_info(grant, Some(selected(s)))
                            .map_err(|_| rt::DENIED)?;
                        if info.id == grant.subject() {
                            Action::SelfRead
                        } else {
                            Action::DirectoryRead
                        }
                    }
                    Read::Group(_) | Read::GroupMember(_, _) => Action::DirectoryRead,
                };
                state
                    .authorize_administrative(grant, c.image, c.scope, action)
                    .map_err(|_| rt::DENIED)?;
                match read {
                    Read::SelfIdentity | Read::User(_) => {
                        let selector = if let Read::User(s) = read {
                            Some(selected(s))
                        } else {
                            None
                        };
                        let u = state.user_info(grant, selector).map_err(|_| rt::DENIED)?;
                        answer[8..24].copy_from_slice(&u.id);
                        answer[24] = u.name.len() as u8;
                        answer[25..25 + u.name.len()].copy_from_slice(u.name.as_bytes());
                        answer[57] = match u.status {
                            Status::Active => 1,
                            Status::Disabled => 2,
                            Status::Deleted => 3,
                        };
                        answer[58] = u.administrator as u8;
                        answer[64..72].copy_from_slice(&u.revision.to_le_bytes());
                        answer[72..80].copy_from_slice(&(u.groups.len() as u64).to_le_bytes());
                    }
                    Read::Group(s) => {
                        let g = state
                            .group_info(grant, selected(s))
                            .map_err(|_| rt::DENIED)?;
                        answer[8..24].copy_from_slice(&g.id);
                        answer[24] = g.name.len() as u8;
                        answer[25..25 + g.name.len()].copy_from_slice(g.name.as_bytes());
                        answer[72..80].copy_from_slice(&(g.members.len() as u64).to_le_bytes());
                    }
                    Read::UserGroup(s, i) => {
                        let u = state
                            .user_info(grant, Some(selected(s)))
                            .map_err(|_| rt::DENIED)?;
                        let id = u
                            .groups
                            .get(usize::try_from(i).map_err(|_| rt::BAD_ARGUMENT)?)
                            .ok_or(rt::BAD_ARGUMENT)?;
                        let g = state
                            .group_info(grant, Selector::Id(*id))
                            .map_err(|_| rt::DENIED)?;
                        answer[8..24].copy_from_slice(id);
                        answer[24] = g.name.len() as u8;
                        answer[25..25 + g.name.len()].copy_from_slice(g.name.as_bytes());
                    }
                    Read::GroupMember(s, i) => {
                        let g = state
                            .group_info(grant, selected(s))
                            .map_err(|_| rt::DENIED)?;
                        let id = g
                            .members
                            .get(usize::try_from(i).map_err(|_| rt::BAD_ARGUMENT)?)
                            .ok_or(rt::BAD_ARGUMENT)?;
                        answer[8..24].copy_from_slice(id);
                    }
                };
                Ok(())
            }
            _ => {
                self.clear();
                Err(rt::DENIED)
            }
        }
    }
}
fn selected(s: WireSelector<'_>) -> Selector<'_> {
    match s {
        WireSelector::Id(i) => Selector::Id(i),
        WireSelector::Name(n) => Selector::Name(n),
    }
}
