//! R2 privé : intentions bornées, confirmation dans la même session, publication
//! existante du triplet. Aucune façade destructive ni acquittement fictif.
use super::{Context, admin_command::{self as wire, Command, Mutation}, selected};
use crate::{common, incarnation, recovery_mutation::Pending, recovery_store::Storage,
            runtime as rt, tete};
use novaos_identity::{Grant, administration::{AdministrativeAction as Action, Operation},
                     recuperation_view::View};
use sha2::{Digest, Sha256};

const UNCERTAIN: u64 = 22;
#[derive(Clone, Copy)]
struct Prepared {
    context: Context,
    binding: [[u8; 32]; 2], // session et instance nsh, attestées par le vérificateur
    request: [u8; 128],
    operation: [u8; 16],
    confirmation: [u8; 32],
}
const _: () = assert!(core::mem::size_of::<Prepared>() <= 512);
pub struct Transactions {
    prepared: [Option<Prepared>; 2],
    pending: Option<Pending>,
    pending_owner: Option<[u8; 16]>,
    completed: [Option<([u8; 16], [u8; 16], u64)>; 2],
    next_completed: usize,
}
impl Transactions {
    pub fn new() -> Self { Self { prepared: [None; 2], pending: None, pending_owner: None,
                                completed: [None; 2], next_completed: 0 } }
    fn remember(&mut self, operation: [u8; 16], owner: [u8; 16], generation: u64) {
        self.completed[self.next_completed] = Some((operation, owner, generation));
        self.next_completed = (self.next_completed + 1) % self.completed.len();
    }

    /// Une nouvelle demande peut retrouver une réponse perdue. Cette méthode
    /// consulte seulement le reçu du même identifiant ; jamais submit à nouveau.
    pub fn reconcile(&mut self, state: &mut View, storage: &mut Storage) -> rt::Result<()> {
        if let Some(p) = self.pending.as_mut() {
            p.reconcile().map_err(|_| UNCERTAIN)?;
            *state = p.activate(storage).map_err(|_| UNCERTAIN)?;
            let operation = p.plan.head().request;
            let owner = self.pending_owner.ok_or(UNCERTAIN)?;
            self.pending = None;
            self.pending_owner = None;
            self.remember(operation, owner, state.head().generation);
        }
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
                let plan = Pending::from_administration(state, grant, operation, current,
                                                        c.reference, operation_kind, None)?;
                drop(plan);
                // La clôture et les facteurs nouveaux ne sont pas annoncés
                // opérationnels avant raccord de leurs effecteurs/provisionneur.
                if !matches!(change, Mutation::CreateGroup(_) | Mutation::Rename {..}) {
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
                    request: *q, operation, confirmation });
                answer[8..24].copy_from_slice(&operation);
                answer[24..56].copy_from_slice(&confirmation);
                Ok(())
            }
            Command::Confirm { operation, digest } => {
                if self.pending.is_some() { return Err(UNCERTAIN); }
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
                let plan = Pending::from_administration(state, grant, operation, current,
                    c.reference, operation_for(state, grant, change)?, None)?;
                self.prepared[slot] = None; // consommation avant toute écriture
                self.pending = Some(plan);
                self.pending_owner = Some(grant.subject());
                let p = self.pending.as_mut().unwrap();
                p.submit(storage).map_err(|_| UNCERTAIN)?;
                *state = p.activate(storage).map_err(|_| UNCERTAIN)?;
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
                if original[160..176] != grant.subject() || original[208] != 4 || terminal.is_some() {
                    return Err(rt::DENIED);
                }
                let generation = u64::from_be_bytes(original[72..80].try_into().unwrap())
                    .checked_add(1).ok_or(rt::LIMIT)?;
                if generation > state.head().generation { return Err(UNCERTAIN); }
                // Le reçu UPDATE seul n'atteste pas une future propagation de
                // retrait de groupe. Ne pas inventer COMPLETE après redémarrage
                // ou pour une opération dont le traitement local n'est plus
                // connu. Le raccord durable des autres issues reste à qualifier.
                if !self.completed.contains(&Some((operation, grant.subject(), generation))) {
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
        Mutation::CreateGroup(name) => Operation::CreateGroup { name },
        Mutation::Rename {user: s, name} => Operation::Rename {subject: user(s)?, name},
        Mutation::Disable(s) => Operation::Disable {subject: user(s)?},
        // Aucun facteur n'est approuvé sur la seule base d'un argument ; aucune
        // réactivation sans reçu de clôture, ni retrait de groupe sans effecteurs.
        _ => return Err(rt::REFUSED),
    })
}
fn completed(answer: &mut [u8; 128], operation: [u8; 16], generation: u64) {
    answer[8..24].copy_from_slice(&operation);
    answer[24..32].copy_from_slice(&generation.to_le_bytes());
    answer[32] = 1;
}
