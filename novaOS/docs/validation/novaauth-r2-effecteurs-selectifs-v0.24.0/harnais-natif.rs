#![allow(dead_code)]
use novaos_identity::policy::{Origin,Permission};
mod rt {
    pub type Result<T>=core::result::Result<T,u64>;
    pub const OK:u64=0; pub const DENIED:u64=5; pub const LIMIT:u64=6;
    pub const REVOKED:u64=10;pub const CLOSED:u64=11; pub const REFUSED:u64=12;
    pub const BAD_ARGUMENT:u64=3;pub const WOULD_BLOCK:u64=7;pub const CANCELLED:u64=9;
    std::thread_local! {pub static STATUS:core::cell::Cell<u64>=const{core::cell::Cell::new(0)};
        pub static SENDS:core::cell::Cell<u64>=const{core::cell::Cell::new(0)};}
    pub fn send_frame(_:u64,_:u64,b:&[u8;128],_:u64,_:bool)->Result<()> {
        assert_eq!(&b[120..],b"DIRREPLY");assert_eq!(&b[..8],&CANCELLED.to_le_bytes());
        SENDS.with(|s|s.set(s.get()+1));match STATUS.with(|s|s.get()) {0=>Ok(()),e=>Err(e)}
    }
}
mod directory {
use super::*;
const READ: u8 = 1;
const WRITE: u8 = 2;
const ALL: u8 = READ | WRITE;
type Principal = [u8; 16];
type SessionId = [u8; 32];

#[derive(Clone, Copy, PartialEq, Eq)]
struct Holder {
    subject: Principal,
    session: SessionId,
    port: u64,
}
#[derive(Clone, Copy)]
struct Lease {
    holder: Holder,
    key: u64,
    parent: u64,
    rights: u8,
    permission: Option<Permission>,
}
#[derive(Clone, Copy)]
struct PendingWrite {
    holder: Holder,
    key: u64,
    request: u64,
}
#[derive(Default)]
pub(super) struct Directory {
    seal: Option<[u8; 104]>,
    #[cfg(feature = "politiques-lab")]
    group: Principal,
    #[cfg(feature = "politiques-lab")]
    install: Principal,
    sleeper: Option<(Holder, u64, u64)>,
    membership: Option<GroupClosure>,
    next: u64,
    leases: [Option<Lease>; 12],
    pending: Option<PendingWrite>,
}
// Quatre mots de raccord dans la page privée du seul ELF de qualification.
fn port(lane: usize, reply: bool) -> u64 {
    if lane >= 2 {
        return 0;
    }
    let index = 24 + lane * 2 + usize::from(reply);
    unsafe {
        (&*(0x401000 as *const core::sync::atomic::AtomicU64).add(index))
            .load(core::sync::atomic::Ordering::Acquire)
    }
}
fn number(b: &[u8]) -> u64 {
    u64::from_le_bytes(b[..8].try_into().unwrap())
}
impl Directory {
    fn grant(&mut self, holder: Holder, parent: u64, rights: u8) -> rt::Result<u64> {
        if rights == 0 || rights & !ALL != 0 {
            return Err(rt::DENIED);
        }
        if parent != 0 && self.check(holder, parent, rights).is_err() {
            return Err(rt::DENIED);
        }
        let slot = self
            .leases
            .iter_mut()
            .find(|s| s.is_none())
            .ok_or(rt::LIMIT)?;
        self.next = self.next.checked_add(1).ok_or(rt::LIMIT)?;
        *slot = Some(Lease {
            holder,
            key: self.next,
            parent,
            rights,
            permission: None,
        });
        Ok(self.next)
    }
    fn check(&self, holder: Holder, mut key: u64, rights: u8) -> rt::Result<()> {
        for _ in 0..self.leases.len() {
            let l = self
                .leases
                .iter()
                .flatten()
                .find(|l| l.key == key && l.holder == holder)
                .ok_or(rt::REVOKED)?;
            if rights & !l.rights != 0 {
                return Err(rt::DENIED);
            }
            if l.parent == 0 {
                return Ok(());
            }
            key = l.parent;
        }
        Err(rt::REVOKED)
    }
    fn revoke(&mut self, holder: Holder, key: u64) -> rt::Result<()> {
        self.check(holder, key, 0)?;
        for s in &mut self.leases {
            if s.is_some_and(|l| l.key == key) {
                *s = None;
            }
        }
        // Collecter les descendants avant acquittement du retrait.
        for i in 0..self.leases.len() {
            if let Some(l) = self.leases[i] {
                if self.check(l.holder, l.key, 0).is_err() {
                    self.leases[i] = None;
                }
            }
        }
        if self
            .pending
            .is_some_and(|p| self.check(p.holder, p.key, WRITE).is_err())
        {
            self.pending = None;
        }
        Ok(())
    }
}


// Inclus dans Directory seulement dans l'export QEMU sélectif.
// L'appel vient du port privé R -> NovaFS, jamais du port de données client.
// Cette fermeture locale ne constitue pas un reçu durable de l'autorité.


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
            if prior.request != *q { return Err(rt::WOULD_BLOCK); }
        } else {
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
            self.membership = Some(GroupClosure { request: *q, removed: 0, cancelled, wake: false, notify });
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

// Tests natifs du CODE de Directory exporté, avec simulation du seul retour IPC.
#[cfg(test)]
mod tests {
    use super::*;
    fn holder(n:u8)->Holder {Holder{subject:[n;16],session:[n;32],port:n as u64}}
    fn permission(origin:Origin,rights:u8,revision:u64)->Permission {
        Permission{origin,rights,rule_revision:9,membership_revision:revision,image_revision:3}
    }
    fn request()->[u8;128] {
        *novaos_recovery::membership_effectors::VfsClosure::new([1;16],[2;16],[3;16],5,10,[4;32]).unwrap().bytes()
    }
    fn direct()->Permission {permission(Origin::Direct,READ,0)}
    fn g()->Permission {permission(Origin::Group([3;16]),WRITE,5)}
    fn h()->Permission {permission(Origin::Group([8;16]),ALL,6)}
    fn verified(q:&[u8;128],a:&[u8;128])->novaos_recovery::membership_effectors::VfsFacts {
        let query=novaos_recovery::membership_effectors::VfsClosure::new([1;16],[2;16],[3;16],5,10,[4;32]).unwrap();
        assert_eq!(query.bytes(),q);query.verify(a).unwrap()
    }
    #[test] fn only_g_and_its_descendants_are_removed() {
        let mut d=Directory::default();let who=holder(2);
        let own=d.membership_grant(who,0,READ,direct()).unwrap();
        let gh=d.membership_grant(who,0,WRITE,g()).unwrap();
        let child=d.membership_grant(who,gh,WRITE,g()).unwrap();
        let hh=d.membership_grant(who,0,ALL,h()).unwrap();
        let bob=d.membership_grant(holder(7),0,WRITE,g()).unwrap();
        let q=request();let a=d.membership_close(&q,[4;32]).unwrap();let facts=verified(&q,&a);
        assert_eq!((facts.removed(),facts.retained()),(2,3));
        assert_eq!(d.check(who,gh,WRITE),Err(rt::REVOKED));
        assert_eq!(d.check(who,child,WRITE),Err(rt::REVOKED));
        d.check(who,own,READ).unwrap();d.check(who,hh,WRITE).unwrap();d.check(holder(7),bob,WRITE).unwrap();
        assert_eq!(d.membership_grant(who,0,WRITE,g()),Err(rt::REVOKED));
        assert_eq!(d.membership_close(&q,[4;32]).unwrap(),a);
        assert!(d.membership_grant(who,0,WRITE,permission(Origin::Group([3;16]),WRITE,11)).is_ok());
        assert_eq!(d.check(who,gh,WRITE),Err(rt::REVOKED));
    }
    #[test] fn provenance_cannot_be_replaced_at_delegation() {
        let mut d=Directory::default();let who=holder(2);let key=d.membership_grant(who,0,WRITE,g()).unwrap();
        assert_eq!(d.membership_grant(who,key,READ,direct()),Err(rt::DENIED));
        assert_eq!(d.membership_grant(holder(7),key,WRITE,g()),Err(rt::REVOKED));
        assert_eq!(d.membership_grant(who,0,ALL,g()),Err(rt::DENIED));
    }
    #[test] fn ipc_saturation_keeps_cancelled_operation_and_wakeup_for_retry() {
        let mut d=Directory::default();let who=holder(2);let key=d.membership_grant(who,0,WRITE,g()).unwrap();
        let hh=d.membership_grant(who,0,ALL,h()).unwrap();
        d.pending=Some(PendingWrite{holder:who,key,request:123});d.sleeper=Some((who,88,4));
        rt::STATUS.with(|s|s.set(rt::WOULD_BLOCK));
        let q=request();assert_eq!(d.membership_close(&q,[4;32]),Err(rt::WOULD_BLOCK));
        assert!(d.pending.is_none());assert!(d.sleeper.is_some());assert_eq!(d.check(who,key,WRITE),Err(rt::REVOKED));
        d.check(who,hh,WRITE).unwrap();
        rt::STATUS.with(|s|s.set(rt::OK));let answer=d.membership_close(&q,[4;32]).unwrap();
        let facts=verified(&q,&answer);assert_eq!(facts.cancelled_prepare(),Some(123));assert!(facts.notified());
        assert!(d.sleeper.is_none());let sent=rt::SENDS.with(|s|s.get());
        assert_eq!(d.membership_close(&q,[4;32]).unwrap(),answer);assert_eq!(rt::SENDS.with(|s|s.get()),sent);
    }
    #[test] fn independent_pending_and_sleeper_are_preserved() {
        let mut d=Directory::default();let who=holder(2);let key=d.membership_grant(who,0,ALL,h()).unwrap();
        d.membership_grant(who,0,WRITE,g()).unwrap();
        d.pending=Some(PendingWrite{holder:who,key,request:321});d.sleeper=Some((who,89,4));
        let q=request();let answer=d.membership_close(&q,[4;32]).unwrap();
        assert_eq!(verified(&q,&answer).cancelled_prepare(),None);
        assert_eq!(d.pending.unwrap().request,321);assert!(d.sleeper.is_some());d.check(who,key,WRITE).unwrap();
    }
    #[test] fn ambiguous_lease_and_wrong_instance_cannot_ack() {
        let mut d=Directory::default();let who=holder(2);let key=d.grant(who,0,READ).unwrap();
        let q=request();assert_eq!(d.membership_close(&q,[5;32]),Err(rt::DENIED));
        assert_eq!(d.membership_close(&q,[4;32]),Err(rt::REFUSED));
        assert!(d.membership.is_none());d.check(who,key,READ).unwrap();
    }
    #[test] fn concurrent_different_withdrawal_cannot_replace_the_first() {
        let mut d=Directory::default();let who=holder(2);let key=d.membership_grant(who,0,ALL,h()).unwrap();
        let q=request();d.membership_close(&q,[4;32]).unwrap();
        let mut other=q;other[8]^=1;assert_eq!(d.membership_close(&other,[4;32]),Err(rt::WOULD_BLOCK));
        d.check(who,key,WRITE).unwrap();
        let mut other=q;other[104]=1;assert_eq!(d.membership_close(&other,[4;32]),Err(rt::BAD_ARGUMENT));
    }
    #[test] fn no_matching_group_is_not_a_session_revocation() {
        let mut d=Directory::default();let who=holder(2);let k=d.membership_grant(who,0,READ,direct()).unwrap();
        let q=request();let a=d.membership_close(&q,[4;32]).unwrap();let f=verified(&q,&a);
        assert_eq!((f.removed(),f.retained(),f.cancelled_prepare()),(0,1,None));d.check(who,k,READ).unwrap();
    }
    #[test] fn initial_group_membership_zero_is_revoked_without_touching_direct() {
        let mut d=Directory::default();let who=holder(2);
        let group=permission(Origin::Group([3;16]),WRITE,0);
        let g=d.membership_grant(who,0,WRITE,group).unwrap();
        let own=d.membership_grant(who,0,READ,direct()).unwrap();
        let mut q=request();q[56..64].fill(0);d.membership_close(&q,[4;32]).unwrap();
        assert_eq!(d.check(who,g,WRITE),Err(rt::REVOKED));d.check(who,own,READ).unwrap();
    }
    #[test] fn bounded_layout_and_full_table_refuses_without_losing_rights() {
        let mut d=Directory::default();let who=holder(2);
        for _ in 0..12 {d.membership_grant(who,0,READ,direct()).unwrap();}
        assert_eq!(d.membership_grant(who,0,READ,direct()),Err(rt::LIMIT));
        assert_eq!(d.leases.iter().flatten().count(),12);
        println!("Directory={} Lease={} Closure={} OptionClosure={}",size_of::<Directory>(),size_of::<Lease>(),size_of::<GroupClosure>(),size_of::<Option<GroupClosure>>());
        assert!(size_of::<Directory>()<=4096);
    }
    #[test] fn failed_wake_does_not_ack_or_remove_independent_new_leases() {
        let mut d=Directory::default();let who=holder(2);
        let key=d.membership_grant(who,0,WRITE,g()).unwrap();
        d.pending=Some(PendingWrite{holder:who,key,request:123});d.sleeper=Some((who,88,4));
        rt::STATUS.with(|s|s.set(rt::DENIED));let q=request();
        assert_eq!(d.membership_close(&q,[4;32]),Err(rt::DENIED));
        let own=d.membership_grant(who,0,READ,direct()).unwrap();
        assert_eq!(d.membership_grant(who,0,WRITE,g()),Err(rt::REVOKED));
        rt::STATUS.with(|s|s.set(rt::CLOSED));let a=d.membership_close(&q,[4;32]).unwrap();
        let facts=verified(&q,&a);assert!(facts.notified());
        assert_eq!(facts.cancelled_prepare(),Some(123));d.check(who,own,READ).unwrap();
        let sent=rt::SENDS.with(|s|s.get());
        assert_eq!(d.membership_close(&q,[4;32]).unwrap(),a);
        assert_eq!(rt::SENDS.with(|s|s.get()),sent);
        rt::STATUS.with(|s|s.set(rt::OK));
    }
    #[test] fn changed_sleeper_and_restarted_instance_do_not_synthesize_ack() {
        let mut d=Directory::default();let who=holder(2);
        let key=d.membership_grant(who,0,WRITE,g()).unwrap();
        d.pending=Some(PendingWrite{holder:who,key,request:123});d.sleeper=Some((who,88,4));
        rt::STATUS.with(|s|s.set(rt::WOULD_BLOCK));let q=request();
        assert_eq!(d.membership_close(&q,[4;32]),Err(rt::WOULD_BLOCK));
        d.sleeper=Some((holder(7),89,5));rt::STATUS.with(|s|s.set(rt::OK));
        let sent=rt::SENDS.with(|s|s.get());
        assert_eq!(d.membership_close(&q,[4;32]),Err(rt::REFUSED));
        assert_eq!(rt::SENDS.with(|s|s.get()),sent);
        assert!(d.sleeper.unwrap().0==holder(7));
        assert_eq!(Directory::default().membership_close(&q,[5;32]),Err(rt::DENIED));
    }

}

}

mod baseline { use super::*;
const READ: u8 = 1;
const WRITE: u8 = 2;
const ALL: u8 = READ | WRITE;
type Principal = [u8; 16];
type SessionId = [u8; 32];

#[derive(Clone, Copy, PartialEq, Eq)]
struct Holder {
    subject: Principal,
    session: SessionId,
    port: u64,
}
#[derive(Clone, Copy)]
struct Lease {
    holder: Holder,
    key: u64,
    parent: u64,
    rights: u8,
    #[cfg(feature = "politiques-lab")]
    permission: Option<Permission>,
}
#[derive(Clone, Copy)]
struct PendingWrite {
    holder: Holder,
    key: u64,
    request: u64,
}
#[derive(Default)]
pub(super) struct Directory {
    seal: Option<[u8; 104]>,
    #[cfg(feature = "politiques-lab")]
    group: Principal,
    #[cfg(feature = "politiques-lab")]
    install: Principal,
    #[cfg(feature = "politiques-lab")]
    sleeper: Option<(Holder, u64, u64)>,
    next: u64,
    leases: [Option<Lease>; 12],
    pending: Option<PendingWrite>,
}
// Quatre mots de raccord dans la page privée du seul ELF de qualification.
fn port(lane: usize, reply: bool) -> u64 {
    if lane >= 2 {
        return 0;
    }
    let index = 24 + lane * 2 + usize::from(reply);
    unsafe {
        (&*(0x401000 as *const core::sync::atomic::AtomicU64).add(index))
            .load(core::sync::atomic::Ordering::Acquire)
    }
}
fn number(b: &[u8]) -> u64 {
    u64::from_le_bytes(b[..8].try_into().unwrap())
}
impl Directory {
    fn grant(&mut self, holder: Holder, parent: u64, rights: u8) -> rt::Result<u64> {
        if rights == 0 || rights & !ALL != 0 {
            return Err(rt::DENIED);
        }
        if parent != 0 && self.check(holder, parent, rights).is_err() {
            return Err(rt::DENIED);
        }
        let slot = self
            .leases
            .iter_mut()
            .find(|s| s.is_none())
            .ok_or(rt::LIMIT)?;
        self.next = self.next.checked_add(1).ok_or(rt::LIMIT)?;
        *slot = Some(Lease {
            holder,
            key: self.next,
            parent,
            rights,
            #[cfg(feature = "politiques-lab")]
            permission: None,
        });
        Ok(self.next)
    }
    fn check(&self, holder: Holder, mut key: u64, rights: u8) -> rt::Result<()> {
        for _ in 0..self.leases.len() {
            let l = self
                .leases
                .iter()
                .flatten()
                .find(|l| l.key == key && l.holder == holder)
                .ok_or(rt::REVOKED)?;
            if rights & !l.rights != 0 {
                return Err(rt::DENIED);
            }
            if l.parent == 0 {
                return Ok(());
            }
            key = l.parent;
        }
        Err(rt::REVOKED)
    }
    fn revoke(&mut self, holder: Holder, key: u64) -> rt::Result<()> {
        self.check(holder, key, 0)?;
        for s in &mut self.leases {
            if s.is_some_and(|l| l.key == key) {
                *s = None;
            }
        }
        // Collecter les descendants avant acquittement du retrait.
        for i in 0..self.leases.len() {
            if let Some(l) = self.leases[i] {
                if self.check(l.holder, l.key, 0).is_err() {
                    self.leases[i] = None;
                }
            }
        }
        if self
            .pending
            .is_some_and(|p| self.check(p.holder, p.key, WRITE).is_err())
        {
            self.pending = None;
        }
        Ok(())
    }
}


}

#[test] fn incremental_control_budget_is_measured_without_counting_legacy_state_twice() {
    let old=core::mem::size_of::<baseline::Directory>();
    let new=core::mem::size_of::<directory::Directory>();
    let added=new.checked_sub(old).unwrap();
    let subtotal=added+core::mem::size_of::<novaos_recovery::membership::Inventory>()
        +novaos_recovery::membership::BODY+256;
    println!("Directory before={} after={} delta={} measured_control_subtotal={}",old,new,added,subtotal);
    assert!(subtotal<=4096);
}

mod execution {
    type Pid=u64;type Res<T>=Result<T,u64>;
    const WOULD_BLOCK:u64=7;const LIMIT:u64=6;
    struct Binding{holder:[u8;16],domain:[u8;16],installation:[u8;16]}
    struct Ticket{issuer:Pid,binding:Binding,scope:[u64;3],remote:Option<()>,unchanged_state:u64}
    struct Process{exec:Vec<Ticket>}struct Processes{slots:Vec<Option<Process>>}struct Core{processes:Processes}
// Inclus exclusivement dans exec.rs de l'export QEMU sélectif.
// Les installations actuellement résolues par R sont personnelles. Ce contrôle
// atteste l'absence de dépendance de groupe, sans révoquer ces tickets.
// Un scope inconnu ou en construction interdit l'acquittement ; il n'est jamais
// reclassé implicitement comme personnel.
pub(super) fn membership_independent(s:&Core, issuer:Pid, subject:[u8;16], domain:[u8;16])->Res<u64> {
    let mut retained=0u64;
    for p in s.processes.slots.iter().flatten() {
        for ticket in &p.exec {
            if ticket.issuer!=issuer || ticket.binding.holder!=subject || ticket.binding.domain!=domain {continue;}
            if ticket.scope!=[0;3] || ticket.remote.is_none() || ticket.binding.installation==[0;16] {
                return Err(WOULD_BLOCK);
            }
            // Ne changer ni state, ni revoked, ni l'admission distante. Les
            // barrières d'époque et d'incarnation historiques restent actives.
            retained=retained.checked_add(1).ok_or(LIMIT)?;
        }
    }
    Ok(retained)
}

// Harnais des structures d'inventaire ; le code testé est celui inclus dans exec.rs.
#[cfg(test)] mod tests {
    use super::*;
    fn ticket(scope:[u64;3],remote:bool)->Ticket {
        Ticket{issuer:1,binding:Binding{holder:[2;16],domain:[7;16],installation:[3;16]},scope,
               remote:remote.then_some(()),unchanged_state:123}
    }
    fn core(tickets:Vec<Ticket>)->Core {Core{processes:Processes{slots:vec![Some(Process{exec:tickets})]}}}
    #[test] fn independent_ready_or_admitted_tickets_are_preserved() {
        let s=core(vec![ticket([0;3],true)]);
        assert_eq!(membership_independent(&s,1,[2;16],[7;16]),Ok(1));
        assert_eq!(s.processes.slots[0].as_ref().unwrap().exec[0].unchanged_state,123);
    }
    #[test] fn unresolved_file_scope_or_missing_independent_admission_cannot_ack() {
        for t in [ticket([1,0,0],true),ticket([0,1,0],true),ticket([0,0,1],true),ticket([0;3],false)] {
            assert_eq!(membership_independent(&core(vec![t]),1,[2;16],[7;16]),Err(WOULD_BLOCK));
        }
    }
    #[test] fn different_subject_issuer_and_domain_are_not_touched() {
        let s=core(vec![ticket([99;3],true)]);
        assert_eq!(membership_independent(&s,9,[2;16],[7;16]),Ok(0));
        assert_eq!(membership_independent(&s,1,[9;16],[7;16]),Ok(0));
        assert_eq!(membership_independent(&s,1,[2;16],[9;16]),Ok(0));
    }
}

}
