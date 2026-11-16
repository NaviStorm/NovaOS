//! Effecteur réservé au volume de qualification. Les données passent directement
//! entre le client et le VFS. NovaAuth atteste une seule remise, sur un canal distinct.
use super::*;
use novaos_identity::{Authority, Registry, lab::Ledger};

pub(super) const NAME: &str = "\x1fNovaAuth-D";
const MAGIC: &[u8; 8] = b"NADIR001";
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

impl Session<'_, '_> {
    pub(super) fn provision_directory(&mut self, q: &Frame) -> rt::Result<u64> {
        if rt::config(13) != 0x4e4133
            || !self.writable
            || self.failed.is_some()
            || self.directory.seal.is_some()
            || q[56..].iter().any(|&v| v != 0)
            || q[24..56] == [0; 32]
            || port(1, false) == 0
        {
            return Err(rt::DENIED);
        }
        let owner: Principal = q[8..24].try_into().unwrap();
        let mut ledger = Ledger::new(self.dev.identity_authority().map_err(error)?);
        let fs = self.fs.as_mut().ok_or(rt::CLOSED)?;
        let reg = Registry::open(fs, &mut ledger, [7; 16]).map_err(|_| rt::DENIED)?;
        reg.grant_attested(owner).map_err(|_| rt::DENIED)?;
        drop(reg);
        // Pas de rattachement à un dossier préexistant, même de même nom.
        if !matches!(fs.lookup_at(fs.root_id(), NAME), Err(Error::NotFound)) {
            return Err(rt::DENIED);
        }
        let d = fs
            .create_at(fs.root_id(), NAME, Kind::Directory)
            .map_err(error)?;
        let data = fs.create_at(d.id, "registre", Kind::File).map_err(error)?;
        let protected = fs
            .create_at(d.id, "prive", Kind::Directory)
            .map_err(error)?;
        let secret = fs
            .create_at(protected.id, "temoin", Kind::File)
            .map_err(error)?;
        let policy = fs.create_at(d.id, "politique", Kind::File).map_err(error)?;
        let mut seal = [0; 104];
        seal[..8].copy_from_slice(MAGIC);
        seal[8..24].copy_from_slice(&owner);
        seal[24..40].copy_from_slice(&fs.ids().fs);
        seal[40..56].copy_from_slice(&fs.ids().lv);
        for (i, id) in [d.id, data.id, protected.id, secret.id, policy.id]
            .into_iter()
            .enumerate()
        {
            seal[56 + 8 * i..64 + 8 * i].copy_from_slice(&id.to_le_bytes());
        }
        seal[96] = ALL; // ACL directe du propriétaire sur D et son registre.
        seal[97] = 1; // Surprotection récursive de prive et de son témoin.
        fs.prepare_write_id(policy.id, 0, &seal)
            .map_err(error)?
            .commit()
            .map_err(error)?;
        let holder = Holder {
            subject: owner,
            session: q[24..56].try_into().unwrap(),
            port: port(1, false),
        };
        let key = self.directory.grant(holder, 0, ALL)?;
        self.directory.seal = Some(seal);
        Ok(key)
    }
    fn folder_op(&mut self, holder: Holder, q: &Frame) -> rt::Result<u64> {
        if self.failed.is_some() || self.pending.is_some() || self.temporary.any_pending() {
            return Err(rt::REFUSED);
        }
        let seal = self.directory.seal.ok_or(rt::DENIED)?;
        let fs = self.fs.as_mut().ok_or(rt::CLOSED)?;
        let dir = fs.lookup_at(fs.root_id(), NAME).map_err(error)?;
        if dir.id != number(&seal[56..]) || dir.kind != Kind::Directory {
            return Err(rt::DENIED);
        }
        let policy = fs.lookup_at(dir.id, "politique").map_err(error)?;
        if policy.id != number(&seal[88..])
            || policy.size != 104
            || fs.inspect_id(policy.id, 0, 104).map_err(error)? != seal
            || holder.subject != seal[8..24]
        {
            return Err(rt::DENIED);
        }
        let file = fs.lookup_at(dir.id, "registre").map_err(error)?;
        if file.id != number(&seal[64..]) || file.kind != Kind::File {
            return Err(rt::DENIED);
        }
        if q[2..8].iter().chain(&q[24..]).any(|&b| b != 0) {
            return Err(rt::BAD_ARGUMENT);
        }
        let key = number(&q[8..]);
        let request = number(&q[16..]);
        let op = q[0];
        let needed = match op {
            1 | 2 => q[1],
            3 | 4 | 8 => READ,
            5 | 6 | 7 => WRITE,
            9 => 0,
            _ => return Err(rt::BAD_ARGUMENT),
        };
        if needed & !seal[96] != 0 {
            return Err(rt::DENIED);
        }
        if op != 1 {
            self.directory.check(holder, key, needed)?;
        }
        match op {
            1 => Err(rt::DENIED),
            2 => self.directory.grant(holder, key, needed),
            3 => Ok(file.size / 8),
            4 => {
                // La politique du parent protégé s'applique avant tout accès au contenu.
                let p = fs.lookup_at(dir.id, "prive").map_err(error)?;
                if p.id != number(&seal[72..]) || p.kind != Kind::Directory {
                    return Err(rt::DENIED);
                }
                let child = fs.lookup_at(p.id, "temoin").map_err(error)?;
                if child.id != number(&seal[80..]) || child.kind != Kind::File || seal[97] != 0 {
                    return Err(rt::DENIED);
                }
                Ok(child.size)
            }
            5 => {
                if !self.writable || request == 0 || self.directory.pending.is_some() {
                    return Err(rt::REFUSED);
                }
                self.directory.pending = Some(PendingWrite {
                    holder,
                    key,
                    request,
                });
                Ok(0)
            }
            6 | 7 => {
                let pending = self.directory.pending.ok_or(rt::REVOKED)?;
                if pending.holder != holder || pending.key != key || pending.request != request {
                    return Err(rt::DENIED);
                }
                self.directory.pending = None;
                if !self.writable || file.size % 8 != 0 || file.size >= 512 {
                    return Err(rt::REFUSED);
                }
                let history = fs
                    .inspect_id(file.id, 0, file.size as usize)
                    .map_err(error)?;
                if history.chunks_exact(8).any(|b| number(b) == request) {
                    return Err(rt::REFUSED);
                }
                fs.prepare_write_id(file.id, file.size, &request.to_le_bytes())
                    .map_err(error)?
                    .commit()
                    .map_err(error)?;
                Ok(0)
            }
            8 => {
                if file.size % 8 != 0 || file.size > 512 {
                    return Err(w::CORRUPT);
                }
                Ok(fs
                    .inspect_id(file.id, 0, file.size as usize)
                    .map_err(error)?
                    .chunks_exact(8)
                    .filter(|b| number(b) == request)
                    .count() as u64)
            }
            9 => self.directory.revoke(holder, key).map(|_| 0),
            _ => Err(rt::BAD_ARGUMENT),
        }
    }
    pub(super) fn attest_folder(&mut self, a: &Frame) -> rt::Result<()> {
        let lane = usize::from(a[8]);
        if lane > 1 {
            return Err(rt::DENIED);
        }
        let rx = port(lane, false);
        let tx = port(lane, true);
        if rx == 0 || tx == 0 || rt::config(18) == 0 {
            return Err(rt::DENIED);
        }
        let tag = number(&a[16..]);
        // L'autorité atteste le canal authentifié. Une requête VFS distincte,
        // portant le même identifiant, doit provenir de l'instance elle-même.
        for _ in 0..16 {
            let msg = rt::receive_for(rx, 0)?;
            if msg.tag < tag {
                continue;
            }
            if msg.tag != tag || msg.len != 128 || msg.kind != 1 {
                return Err(rt::DENIED);
            }
            let holder = Holder {
                subject: a[24..40].try_into().unwrap(),
                session: a[40..72].try_into().unwrap(),
                port: rx,
            };
            let ledger = Ledger::new(self.dev.identity_authority().map_err(error)?);
            if ledger.current().map_err(|_| rt::REFUSED)?.digest != a[72..104] {
                return Err(rt::DENIED);
            }
            let result = self.folder_op(holder, &msg.payload);
            // Injection explicite de perte de la réponse après écriture durable.
            // L'appelant ne peut pas déduire l'issue de cette absence.
            if msg.payload[0] == 7 && result.is_ok() {
                return Err(w::UNCERTAIN);
            }
            let mut reply = [0; 128];
            let status = result.as_ref().err().copied().unwrap_or(rt::OK);
            reply[..8].copy_from_slice(&status.to_le_bytes());
            reply[8..16].copy_from_slice(&result.unwrap_or(0).to_le_bytes());
            reply[120..].copy_from_slice(b"DIRREPLY");
            rt::send_frame(tx, tag, &reply, 0, false).map_err(|_| w::UNCERTAIN)?;
            return Ok(());
        }
        Err(rt::DENIED)
    }
}
