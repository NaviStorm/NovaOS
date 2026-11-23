//! Inclus dans Directory seulement dans l'export QEMU sélectif.
//! L'appel vient du port privé R -> NovaFS, jamais du port de données client.
//! Cette fermeture locale ne constitue pas un reçu durable de l'autorité.
use novaos_identity::policy::{Origin, Permission};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct GroupSource {
    pub subject: [u8; 16],
    pub group: [u8; 16],
    pub revision: u64,
}
#[derive(Clone, Copy)]
struct GroupClosure {
    pub request: [u8; 128],
    pub removed: u64,
    pub cancelled: u64,
    pub wake: bool,
    terminal: bool,
    notify: Option<(Holder, u64, u64)>,
}
impl GroupSource {
    fn matches(self, holder: Holder, permission: Permission) -> bool {
        holder.subject == self.subject && permission.origin == Origin::Group(self.group)
            && permission.membership_revision == self.revision
    }
    fn parse(q: &[u8; 128]) -> rt::Result<Self> {
        if &q[..8] != b"NXMGVFS1" || q[8..24] == [0; 16]
            || q[24..40] == [0; 16] || q[40..56] == [0; 16]
            || number(&q[56..]) >= 1 << 63 || number(&q[64..]) == 0
            || number(&q[64..]) >= 1 << 63
            || q[72..104] == [0; 32] || q[104..] != [0; 24] {
            return Err(rt::BAD_ARGUMENT);
        }
        Ok(Self { subject: q[24..40].try_into().unwrap(),
            group: q[40..56].try_into().unwrap(), revision: number(&q[56..]) })
    }
}
impl Directory {
    /// Appelé par le registre sur son port privé après vérification de RETIREE.
    /// Ce terminal ne retire aucune barrière et ne confère aucun droit.
    pub(super) fn membership_terminal(&mut self,q:&[u8;128],instance:[u8;32])->rt::Result<[u8;128]> {
        if &q[..8]!=b"NXMGEND1" {return Err(rt::BAD_ARGUMENT);}
        let mut original=*q;original[..8].copy_from_slice(b"NXMGVFS1");GroupSource::parse(&original)?;
        if instance==[0;32] || original[72..104]!=instance {return Err(rt::DENIED);}
        if self.membership.is_none_or(|prior|prior.request!=original){return Err(rt::DENIED);}
        let facts=self.membership_close(&original,instance)?;
        self.membership.as_mut().unwrap().terminal=true;Ok(facts)
    }

    fn membership_permission(&self, holder: Holder, mut key: u64) -> rt::Result<Permission> {
        for _ in &self.leases {
            let l = self.leases.iter().flatten().find(|l| l.holder == holder && l.key == key)
                .ok_or(rt::REVOKED)?;
            if l.parent == 0 { return l.permission.ok_or(rt::REFUSED); }
            key = l.parent;
        }
        Err(rt::REVOKED)
    }
    /// Appeler sous la même sérialisation que grant/folder_op. La provenance
    /// appartient au parent ; une sous-délégation ne peut la remplacer.
    fn membership_admit(&self, holder: Holder, permission: Permission) -> rt::Result<()> {
        if self.membership_previous.is_some_and(|source|source.matches(holder,permission)){return Err(rt::REVOKED);}
        if let Some(closed) = self.membership {
            let source = GroupSource::parse(&closed.request)?;
            if source.matches(holder, permission) { return Err(rt::REVOKED); }
        }
        Ok(())
    }
    /// Permission déjà revalidée par R sur sa tête indépendante fraîche.
    /// Cette méthode n'est jamais appelée à partir d'un handle fourni seul.
    fn membership_grant(&mut self, holder: Holder, parent: u64, rights: u8,
        permission: Permission) -> rt::Result<u64> {
        self.membership_admit(holder, permission)?;
        if rights == 0 || rights & !permission.rights != 0 {return Err(rt::DENIED);}
        if parent != 0 && self.membership_permission(holder,parent)? != permission {
            return Err(rt::DENIED);
        }
        let key=self.grant(holder,parent,rights)?;
        if parent==0 {
            self.leases.iter_mut().flatten().find(|l|l.key==key).unwrap().permission=Some(permission);
        }
        Ok(key)
    }
    /// Fermer uniquement les racines correspondant au tuple, puis leurs enfants.
    /// Un pending représente une écriture PREPARE sans effet disque. Les commits
    /// sont synchrones dans cette boucle ; aucun commit n'est annulé ici.
    pub(super) fn membership_close(&mut self, q: &[u8; 128], instance: [u8; 32]) -> rt::Result<[u8; 128]> {
        let source = GroupSource::parse(q)?;
        if instance == [0; 32] || q[72..104] != instance { return Err(rt::DENIED); }
        if let Some(prior) = self.membership {
            if prior.request != *q {
                if !prior.terminal {return Err(rt::WOULD_BLOCK);}
                let previous=GroupSource::parse(&prior.request)?;
                if previous==source {return Err(rt::REVOKED);}
                if self.membership_previous.is_some(){return Err(rt::LIMIT);}
                // Deux retraits bornés ; aucune barrière précédente effacée.
                self.membership_previous=Some(previous);self.membership=None;
            }
        }
        if self.membership.is_none() {
            // Refuser un inventaire ambigu plutôt que classer un ancien lease
            // sans provenance comme personnel. Aucun effet avant cette revue.
            for l in self.leases.iter().flatten().filter(|l| l.holder.subject == source.subject) {
                self.membership_permission(l.holder, l.key)?;
            }
            let cancelled = self.pending.filter(|p|
                self.membership_permission(p.holder, p.key).is_ok_and(|v| source.matches(p.holder, v)))
                .map_or(0, |p| p.request);
            let notify=if cancelled!=0 {
                if let Some((holder,_,_))=self.sleeper {
                    if self.pending.is_none_or(|p|p.holder!=holder) {return Err(rt::REFUSED);}
                }
                self.sleeper
            }else{None};
            self.membership = Some(GroupClosure { request: *q, removed: 0, cancelled, wake: false, terminal:false, notify });
            let before = self.leases.iter().flatten().count();
            for i in 0..self.leases.len() {
                if let Some(l) = self.leases[i]
                    && self.membership_permission(l.holder, l.key).is_ok_and(|v| source.matches(l.holder, v)) {
                    self.revoke(l.holder, l.key)?;
                }
            }
            self.membership.as_mut().unwrap().removed = (before - self.leases.iter().flatten().count()) as u64;
        }
        // Ne perdre ni le réveil ni le fait d'annulation quand IPC est saturé.
        // Une consultation répétée ne répète pas la mutation de droits.
        let closed=self.membership.unwrap();
        if !closed.wake && let Some((_,tag,tx))=closed.notify {
            if self.sleeper!=closed.notify {return Err(rt::REFUSED);}
            let mut reply=[0;128];reply[..8].copy_from_slice(&rt::CANCELLED.to_le_bytes());
            reply[120..].copy_from_slice(b"DIRREPLY");
            match rt::send_frame(tx,tag,&reply,0,false) {
                Ok(_) | Err(rt::CLOSED|rt::REVOKED) => {
                    self.sleeper=None;
                    self.membership.as_mut().unwrap().wake=true;
                }
                Err(e)=>return Err(e),
            }
        }
        let mut remaining = 0;
        for l in self.leases.iter().flatten().filter(|l| l.holder.subject == source.subject) {
            if source.matches(l.holder, self.membership_permission(l.holder, l.key)?) { remaining += 1; }
        }
        if remaining != 0 { return Err(rt::WOULD_BLOCK); }
        let closed = self.membership.unwrap();
        let mut answer = [0; 128];
        answer[8..16].copy_from_slice(b"NXMGACK1");
        answer[16..48].copy_from_slice(&novaos_identity::recuperation_view::seal_digest(q));
        answer[48..80].copy_from_slice(&instance);
        answer[80..88].copy_from_slice(&closed.removed.to_le_bytes());
        answer[88..96].copy_from_slice(&closed.cancelled.to_le_bytes());
        answer[96..104].copy_from_slice(&(remaining as u64).to_le_bytes());
        answer[104..112].copy_from_slice(&(self.leases.iter().flatten().count() as u64).to_le_bytes());
        answer[112..120].copy_from_slice(&u64::from(closed.wake).to_le_bytes());
        Ok(answer)
    }
}
