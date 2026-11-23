//! R2 privé : intentions bornées, confirmation dans la même session, publication
//! existante du triplet. Aucune façade destructive ni acquittement fictif.
use super::{Context, admin_command::{self as wire, Command, Mutation}, selected};
use crate::{common, incarnation, recovery_mutation::Pending, recovery_store::Storage,
            runtime as rt, tete};
use novaos_identity::{Grant, administration::{AdministrativeAction as Action, Operation},
                     recuperation_view::View};
use sha2::{Digest, Sha256};

const UNCERTAIN: u64 = 22;
#[path="administration_retirement.rs"]
mod retirement;
#[path="administration_factors_lab.rs"]
mod factors;
#[derive(Clone, Copy)]
struct Prepared {
    context: Context,
    binding: [[u8; 32]; 2], // session et instance nsh, attestées par le vérificateur
    request: [u8; 128],
    operation: [u8; 16],
    confirmation: [u8; 32],
    closed: Option<([u8; 16], u64)>,
    requested: bool,
}
const _: () = assert!(core::mem::size_of::<Prepared>() <= 512);
pub struct Transactions {
    prepared: [Option<Prepared>; 2],
    pending: Option<Pending>,
    pending_owner: Option<[u8; 16]>,
    completed: [Option<([u8; 16], [u8; 16], u64)>; 2],
    next_completed: usize,
    effects_submitted: bool,
    #[cfg(feature="membership-r2-guest")]
    selective_complete:Option<([u8;16],crate::membership_transaction::Selective)>,
}
impl Transactions {
    pub fn new() -> Self { Self { prepared: [None; 2], pending: None, pending_owner: None,
                                completed: [None; 2], next_completed: 0, effects_submitted:false,
        #[cfg(feature="membership-r2-guest")] selective_complete:None } }
    #[cfg(feature="membership-r2-guest")]
    pub fn selective_in_progress(&self)->bool{self.pending.as_ref().is_some_and(|p|p.selective.is_some())}
    #[cfg(feature="membership-r2-guest")]
    pub fn reconcile_request(&mut self,state:&mut View,storage:&mut Storage,q:&[u8;128])->rt::Result<()> {
        if let Some(p)=self.pending.as_ref().filter(|p|p.selective.is_some()) {
            // Après activation P9 vérifiée, la résolution de chaque fragment
            // ne relance pas le drainage. Seul OUTCOME de CETTE opération le
            // consulte. Les demandes indépendantes vérifient toujours leur tête
            // et leurs capacités ; aucun ancien état n'est accepté par ce cache.
            let matching_outcome=matches!(wire::decode(q),Ok(Command::Outcome(op)) if op==p.plan.head().request);
            if p.plan.head()==&state.head() && !matching_outcome{return Ok(());}
        }
        self.reconcile(state,storage)
    }
    fn remember(&mut self, operation: [u8; 16], owner: [u8; 16], generation: u64) {
        self.completed[self.next_completed] = Some((operation, owner, generation));
        self.next_completed = (self.next_completed + 1) % self.completed.len();
    }

    /// Une nouvelle demande peut retrouver une réponse perdue. Cette méthode
    /// consulte seulement le reçu du même identifiant ; jamais submit à nouveau.
    pub fn reconcile(&mut self, state: &mut View, storage: &mut Storage) -> rt::Result<()> {
        if let Some(p) = self.pending.as_mut() {
            // Une réservation n'est pas une mutation publiée. Sa confirmation
            // reste explicite, liée à la préparation et à la session d'origine.
            if !p.started(){return Ok(());}
            p.reconcile().map_err(|_| UNCERTAIN)?;
            *state = p.activate(storage).map_err(|_| UNCERTAIN)?;
            let operation = p.plan.head().request;
            if p.retirement().is_some() {
                if !self.effects_submitted {
                    self.effects_submitted=true;
                    retirement::acknowledge(p)?;
                }
                // Consultation pure après un accusé perdu. L'acquittement des
                // effecteurs n'est jamais envoyé automatiquement une seconde fois.
                retirement::terminal(p)?;
            }
            #[cfg(feature="membership-r2-guest")]
            p.finish_membership(state)?;
            let owner = self.pending_owner.ok_or(UNCERTAIN)?;
            #[cfg(feature="membership-r2-guest")]
            if let Some(s)=p.selective.take(){self.selective_complete=Some((owner,s));}
            self.pending = None;
            self.pending_owner = None;
            self.remember(operation, owner, state.head().generation);
        }
        Ok(())
    }

    /// Route privée vérificateur/registre : aucun ELF ne peut soumettre ces
    /// trames. La cible est résolue par le registre à partir de la préparation.
    pub fn effectors(&mut self,state:&View,c:Context,q:&[u8;128],a:&mut [u8;128])->rt::Result<()> {
        let binding=c.binding.ok_or(rt::DENIED)?;
        let mut confirm=*q;confirm[..8].copy_from_slice(b"NXACMD01");
        let Command::Confirm{operation,digest}=wire::decode(&confirm).map_err(|_|rt::BAD_ARGUMENT)?
            else{return Err(rt::BAD_ARGUMENT)};
        let p=self.prepared.iter_mut().flatten().find(|p|p.operation==operation && p.confirmation==digest)
            .ok_or(rt::DENIED)?;
        if p.binding!=binding || p.context.reference!=c.reference || p.context.epoch!=c.epoch
            || p.context.head!=c.head || c.head!=state.head().digest
            || p.context.image!=c.image {return Err(rt::REVOKED);}
        let grant=state.resolve_verified_factor(c.reference).map_err(|_|rt::DENIED)?;
        let Command::Prepare(change)=wire::decode(&p.request).map_err(|_|rt::BAD_ARGUMENT)?
            else{return Err(rt::BAD_ARGUMENT)};
        authorize(state,grant,c,change)?;
        #[cfg(feature="membership-r2-guest")]
        if let Mutation::RemoveMember{..}=change {
            let Operation::RemoveMember{subject,group}=operation_for(state,grant,change)? else{return Err(rt::REFUSED)};
            if self.pending.is_none() {
                if &q[..8]==b"NXRTACK2" {return Err(rt::DENIED);}
                let current=tete::query(rt::config(8),&incarnation::identity())?;
                self.pending=Some(Pending::from_membership(state,grant,operation,current,c.reference,subject,group)?);
                self.pending_owner=Some(grant.subject());
            }
            let pending=self.pending.as_mut().ok_or(UNCERTAIN)?;
            if pending.plan.head().request!=operation || pending.started(){return Err(UNCERTAIN);}
            pending.reserve_membership()?;
            let selective=pending.selective.as_mut().ok_or(rt::DENIED)?;
            selective.selection(a)?;
            if &q[..8]==b"NXRTACK2" {
                if !p.requested{return Err(rt::DENIED);}
                selective.exec_verified();
            }else{p.requested=true;}
            return Ok(());
        }
        if !matches!(change,Mutation::Disable(_)) {return Ok(());}
        // Le même plan vérifie le dernier administrateur et fournit l'époque.
        let current=crate::tete::query(rt::config(8),&incarnation::identity())?;
        let plan=Pending::from_administration(state,grant,operation,current,c.reference,
            operation_for(state,grant,change)?,None)?;
        let (subject,epoch)=plan.retirement().ok_or(rt::REFUSED)?;
        a[8..24].copy_from_slice(&subject);a[24..32].copy_from_slice(&epoch.to_le_bytes());
        a[32..48].copy_from_slice(&operation);
        if &q[..8]==b"NXRTACK2" {
            if !p.requested {return Err(rt::DENIED);}
            p.closed=Some((subject,epoch));
        }else{p.requested=true;}
        Ok(())
    }

    pub fn request(&mut self, state: &mut View, storage: &mut Storage,
                   c: Context, q: &[u8; 128], answer: &mut [u8; 128]) -> rt::Result<()> {
        let binding = c.binding.ok_or(rt::DENIED)?;
        let current = tete::query(rt::config(8), &incarnation::identity())?;
        if c.head != state.head().digest || current[72..80] != state.head().generation.to_be_bytes()
            || current[80..112] != state.head().digest { return Err(rt::REVOKED); }
        let grant = state.resolve_verified_factor(c.reference).map_err(|_| rt::DENIED)?;
        if c.epoch != state.authentication_epoch(grant).map_err(|_| rt::DENIED)? {
            return Err(rt::REVOKED);
        }
        match wire::decode(q).map_err(|_| rt::BAD_ARGUMENT)? {
            Command::Prepare(change) => {
                if self.pending.is_some(){return Err(UNCERTAIN);}
                authorize(state, grant, c, change)?;
                let mut hash = Sha256::new();
                hash.update(b"NovaAuth/QEMU/admin-intention/v1\0");
                hash.update(incarnation::identity());
                for b in binding { hash.update(b); }
                hash.update(c.reference); hash.update(c.epoch.to_le_bytes());
                hash.update(c.head); hash.update(c.image.installation); hash.update(c.image.digest);
                hash.update([c.scope.bits()]); hash.update(q);
                let confirmation: [u8; 32] = hash.finalize().into();
                let operation = confirmation[..16].try_into().unwrap();
                // Préflight par le même moteur de transaction. Désactiver un
                // administrateur est refusé ici par State/apply, pas par la CLI.
                let operation_kind = operation_for(state, grant, change)?;
                let retired=retired_for(state,grant,&operation_kind,current)?;
                let plan = make_plan(state,grant,operation,current,c.reference,operation_kind,retired.as_ref())?;
                drop(plan);
                // RemoveMember reste absent du profil R2 historique : le profil
                // sélectif exige sa réservation et les faits des effecteurs.
                let selective=cfg!(feature="membership-r2-guest") && matches!(change,Mutation::RemoveMember{..});
                if !selective && !matches!(change, Mutation::Create {..} | Mutation::CreateGroup(_) | Mutation::Rename {..} | Mutation::Disable(_) | Mutation::Reactivate(_) | Mutation::AddMember {..}) {
                    return Err(rt::REFUSED);
                }
                let slot = self.prepared.iter().position(|p| p.is_some_and(|p| p.binding == binding))
                    .or_else(|| self.prepared.iter().position(Option::is_none))
                    .or_else(|| self.prepared.iter().position(|p| p.is_some_and(|p|
                        p.binding[1] == binding[1] && p.context.reference == c.reference)))
                    .ok_or(rt::LIMIT)?;
                // Remplacer une préparation sans effet invalide sa confirmation.
                // Une mutation déjà soumise n'utilise jamais cette table.
                self.prepared[slot] = Some(Prepared { context: c, binding,
                    request: *q, operation, confirmation, closed:None, requested:false });
                answer[8..24].copy_from_slice(&operation);
                answer[24..56].copy_from_slice(&confirmation);
                Ok(())
            }
            Command::Confirm { operation, digest } => {
                if self.pending.as_ref().is_some_and(|p|p.started() || p.plan.head().request!=operation) { return Err(UNCERTAIN); }
                let slot = self.prepared.iter().position(|p| p.is_some_and(|p|
                    p.operation == operation && p.confirmation == digest)).ok_or(rt::DENIED)?;
                let p = self.prepared[slot].as_ref().unwrap();
                if p.binding != binding || p.context.reference != c.reference
                    || p.context.image.installation != c.image.installation
                    || p.context.image.digest != c.image.digest { return Err(rt::DENIED); }
                if p.context.epoch != c.epoch { return Err(rt::REVOKED); }
                if p.context.head != c.head { return Err(24); } // conflit natif NovaFS
                let Command::Prepare(change) = wire::decode(&p.request).map_err(|_| rt::BAD_ARGUMENT)?
                    else { return Err(rt::BAD_ARGUMENT); };
                authorize(state, grant, c, change)?;
                let operation_kind=operation_for(state,grant,change)?;
                let retired=retired_for(state,grant,&operation_kind,current)?;
                if self.pending.is_none() {
                    let plan=make_plan(state,grant,operation,current,c.reference,operation_kind,retired.as_ref())?;
                    if plan.retirement().is_some() && plan.retirement()!=p.closed {return Err(UNCERTAIN);}
                    self.pending=Some(plan);
                }
                self.pending_owner = Some(grant.subject());
                self.effects_submitted=false;
                let p = self.pending.as_mut().unwrap();
                let submitted=p.submit(storage);
                // Si une fermeture privée reste à réconcilier, la confirmation
                // garde son identité. Dès la première écriture, aucun rejeu.
                if p.started(){self.prepared[slot]=None;}
                submitted.map_err(|_| UNCERTAIN)?;
                *state = p.activate(storage).map_err(|_| UNCERTAIN)?;
                if p.retirement().is_some() {
                    self.effects_submitted=true; // avant l'envoi ; jamais de répétition
                    retirement::acknowledge(p)?;
                    retirement::terminal(p)?;
                }
                #[cfg(feature="membership-r2-guest")]
                p.finish_membership(state)?;
                #[cfg(feature="membership-r2-guest")]
                if let Some(s)=p.selective.take(){self.selective_complete=Some((grant.subject(),s));}
                self.pending = None;
                self.pending_owner = None;
                self.remember(operation, grant.subject(), state.head().generation);
                completed(answer, operation, state.head().generation);
                Ok(())
            }
            Command::Outcome(operation) => {
                // L'historique signé ne contient pas la syntaxe de l'action.
                // Exiger les deux portées de mutation pour cette première vue
                // conservatrice, et le même principal authentifié d'origine.
                for action in [Action::Users, Action::Groups] {
                    state.authorize_administrative(grant, c.image, c.scope, action).map_err(|_| rt::DENIED)?;
                }
                #[cfg(feature="membership-r2-guest")]
                {
                    if self.pending.as_ref().is_some_and(|p|p.plan.head().request==operation){return Err(UNCERTAIN);}
                    if let Some((owner,s))=&self.selective_complete {
                        if s.binding.operation==operation {
                            if *owner!=grant.subject(){return Err(rt::DENIED);}
                            let generation=s.terminal_for_state(state)?;
                            completed(answer,operation,generation);return Ok(());
                        }
                    }
                }
                let mut packet = [0; 160]; packet[..112].copy_from_slice(&current);
                packet[112..128].copy_from_slice(&operation);
                packet[128..].copy_from_slice(&common::random()?[..]);
                use novaos_recovery::{administration::{Message, HISTORY}, framing::{HEADER, MAX}};
                let message = Message::parse(HISTORY, &packet).map_err(|_| rt::BAD_ARGUMENT)?;
                let mut reply = [0; MAX];
                let n = crate::recovery_rpc::Client::new(rt::config(8), incarnation::identity())
                    .exchange(HISTORY, &packet, &mut reply).map_err(|_| UNCERTAIN)?;
                if n < 8 || common::number(&reply[HEADER..]) != 0 { return Err(UNCERTAIN); }
                let key = ed25519_dalek::VerifyingKey::from_bytes(include_bytes!(env!("NOVA_DECISION_PUB")))
                    .map_err(|_| rt::REFUSED)?;
                let (original, _, terminal) = message.history(&key, &reply[HEADER+8..HEADER+n])
                    .map_err(|_| rt::DENIED)?;
                if original[160..176] != grant.subject()
                    || !matches!((original[208],terminal.is_some()),(1,false)|(4,false)|(2,true)|(2,false)|(3,false)|(5,false)|(6,false)|(7,false)) {
                    return Err(rt::DENIED);
                }
                let generation = u64::from_be_bytes(original[72..80].try_into().unwrap())
                    .checked_add(1).ok_or(rt::LIMIT)?;
                if generation > state.head().generation { return Err(UNCERTAIN); }
                // Un type sans retrait et son historique authentifié permettent
                // la reprise sans cache. UPDATE historique demeure conservateur.
                // Le reçu RETIREE, vérifié séparément, reste indispensable au retrait.
                let durable = message.durable_non_retiring_history(&key, &reply[HEADER+8..HEADER+n], &grant.subject())
                    .map_err(|_|rt::DENIED)?;
                let retired = message.durable_retired_history(&key, &reply[HEADER+8..HEADER+n], &grant.subject())
                    .map_err(|_|rt::DENIED)?;
                let legacy_local=original[208]==4 && self.completed.contains(&Some((operation, grant.subject(), generation)));
                if durable != Some(generation) && retired != Some(generation) && !legacy_local {
                    return Err(UNCERTAIN);
                }
                completed(answer, operation, generation);
                Ok(())
            }
            _ => Err(rt::BAD_ARGUMENT),
        }
    }
}
fn authorize(state: &View, grant: Grant, c: Context, change: Mutation<'_>) -> rt::Result<()> {
    let action = if matches!(change, Mutation::CreateGroup(_) | Mutation::AddMember {..} | Mutation::RemoveMember {..}) {
        Action::Groups
    } else { Action::Users };
    state.authorize_administrative(grant, c.image, c.scope, action).map_err(|_| rt::DENIED)
}
fn operation_for<'a>(state: &View, grant: Grant, change: Mutation<'a>) -> rt::Result<Operation<'a>> {
    let user = |s| state.user_info(grant, Some(selected(s))).map(|u| u.id).map_err(|_| rt::DENIED);
    Ok(match change {
        Mutation::Create {name,group,factor} => {
            factors::authorize(state.head().domain,factor)?;
            Operation::Create {name,factor,group:state.group_info(grant,selected(group)).map_err(|_|rt::DENIED)?.id}
        },
        Mutation::CreateGroup(name) => Operation::CreateGroup { name },
        Mutation::AddMember {user: s, group: g} => Operation::AddMember {
            subject: user(s)?,
            group: state.group_info(grant,selected(g)).map_err(|_|rt::DENIED)?.id,
        },
        #[cfg(feature="membership-r2-guest")]
        Mutation::RemoveMember {user:s,group:g}=>Operation::RemoveMember{
            subject:user(s)?,group:state.group_info(grant,selected(g)).map_err(|_|rt::DENIED)?.id},
        Mutation::Rename {user: s, name} => Operation::Rename {subject: user(s)?, name},
        Mutation::Disable(s) => Operation::Disable {subject: user(s)?},
        Mutation::Reactivate(s) => Operation::Reactivate {subject: user(s)?},
        // Aucun facteur n'est approuvé sur la seule base d'un argument ; aucune
        // réactivation sans reçu de clôture, ni retrait de groupe sans effecteurs.
        _ => return Err(rt::REFUSED),
    })
}
fn make_plan(state:&View,grant:Grant,operation:[u8;16],current:[u8;112],reference:[u8;16],change:Operation<'_>,retired:Option<&novaos_identity::administration::Retired>)->rt::Result<Pending>{
    #[cfg(feature="membership-r2-guest")]
    if let Operation::RemoveMember{subject,group}=change {
        return Pending::from_membership(state,grant,operation,current,reference,subject,group);
    }
    Pending::from_administration(state,grant,operation,current,reference,change,retired)
}
fn retired_for(state:&View,grant:Grant,change:&Operation<'_>,current:[u8;112])
    ->rt::Result<Option<novaos_identity::administration::Retired>> {
    if let Operation::Reactivate{subject}=change {
        retirement::recovered(state,grant,*subject,current).map(Some)
    } else {Ok(None)}
}
fn completed(answer: &mut [u8; 128], operation: [u8; 16], generation: u64) {
    answer[8..24].copy_from_slice(&operation);
    answer[24..32].copy_from_slice(&generation.to_le_bytes());
    answer[32] = 1;
}
