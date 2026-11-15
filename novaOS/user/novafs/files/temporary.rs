//! Le noyau remet une identité attestée avec chaque demande. Les clients n’obtiennent
//! ni le canal d’administration, ni les identifiants d’objets en guise de handles.
use super::*;
#[allow(dead_code)]
#[path = "../../../abi/tmp.rs"]
mod wire;

struct Private {
    owner: [u8; 32],
    root: u64,
    client: Client,
    pending: Option<Pending>,
}

pub(super) struct Views {
    pub enabled: bool,
    store: Option<(tmp_store::Store, [u8; 16])>,
    open: Vec<Private>,
    retired: Vec<([u8; 32], u64)>,
}

impl Views {
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            store: None,
            open: Vec::new(),
            retired: Vec::new(),
        }
    }

    fn find(&self, owner: &[u8; 32]) -> Option<usize> {
        self.open.iter().position(|entry| entry.owner == *owner)
    }
}

fn log(bytes: &[u8]) {
    rt::call(
        rt::DEBUG_WRITE,
        bytes.as_ptr() as u64,
        bytes.len() as u64,
        0,
    );
}

impl Session<'_, '_> {
    fn create_tmp(&mut self, owner: [u8; 32]) -> rt::Result<()> {
        let epoch: [u8; 16] = owner[..16].try_into().unwrap();
        if epoch == [0; 16] || !self.writable {
            return Err(rt::REFUSED);
        }
        if self.temporary.find(&owner).is_some()
            || self.temporary.retired.iter().any(|(id, _)| *id == owner)
        {
            return Err(w::EXISTS);
        }
        if self.pending.is_some() {
            return Err(w::BUSY);
        }
        if self.temporary.open.len() + self.temporary.retired.len() >= 32 {
            return Err(rt::LIMIT);
        }
        self.temporary
            .open
            .try_reserve(1)
            .map_err(|_| rt::NO_MEMORY)?;
        self.clock.sample()?;
        let fs = self.fs.as_mut().ok_or(rt::CLOSED)?;
        let store = match self.temporary.store {
            Some((store, current)) if epoch == current => store,
            Some(_) => return Err(rt::REFUSED),
            None => {
                let store = tmp_store::Store::mount(fs).map_err(namespace::error)?;
                let audit = store.recover(fs, epoch).map_err(namespace::error)?;
                if audit.quarantined != 0 {
                    log(b"TMP reprise quarantaine");
                }
                if audit.removed != 0 {
                    log(b"TMP reprise orphelins retires");
                }
                self.temporary.store = Some((store, epoch));
                store
            }
        };
        let root = store.create(fs, owner).map_err(namespace::error)?;
        let mut client = Client::new(1, w::DIR_ALL);
        client.mounted = true;
        client.private_root = Some(root);
        client.cwd = (root, w::DIR_ALL);
        self.temporary.open.push(Private {
            owner,
            root,
            client,
            pending: None,
        });
        Ok(())
    }

    fn retire_tmp(&mut self, owner: [u8; 32]) -> rt::Result<()> {
        let Some(i) = self.temporary.find(&owner) else {
            return Ok(());
        };
        // Les handles et les fragments non publiés deviennent inaccessibles
        // avant la première E/S de nettoyage. Aucun réemploi de cette vue.
        let entry = self.temporary.open.swap_remove(i);
        let root = entry.root;
        drop(entry);
        if self.pending.is_some() {
            // Le WRITE historique peut réserver tous les blocs restants jusqu'à SYNC.
            // Les handles sont déjà révoqués. Différer seulement le retrait physique.
            self.temporary.retired.try_reserve(1).map_err(|_| {
                log(b"TMP retrait en quarantaine");
                rt::NO_MEMORY
            })?;
            self.temporary.retired.push((owner, root));
            return Ok(());
        }
        self.purge_tmp(owner, root)
    }

    fn purge_tmp(&mut self, owner: [u8; 32], root: u64) -> rt::Result<()> {
        let result = (|| {
            if let Some(e) = self.failed {
                return Err(e);
            }
            self.clock.sample()?;
            let (store, _) = self.temporary.store.ok_or(rt::CLOSED)?;
            let fs = self.fs.as_mut().ok_or(rt::CLOSED)?;
            store.purge(fs, owner, root).map_err(namespace::error)
        })();
        log(if result.is_ok() {
            b"TMP retrait confirme"
        } else {
            b"TMP retrait en quarantaine"
        });
        result
    }

    fn private_request(&mut self, owner: [u8; 32], q: &Frame, a: &mut Frame) -> rt::Result<()> {
        // Ni montage du volume, ni vue choisie par un autre client, ni
        // opérations historiques OPEN/INFO à portée globale sur cette porte.
        if !(matches!(
            q[1],
            w::PATH_FRAGMENT
                | w::MKDIR_AT
                | w::RMDIR
                | w::UNLINK_AT
                | w::RENAME_AT
                | w::DIR_OPEN
                | w::OPEN_AT
                | w::READDIR
                | w::LOOKUP_AT
                | w::DIR_CLOSE
                | w::STAT
                | w::GETCWD
                | w::READ
                | w::WRITE
                | w::SYNC
        ) || q[1] == w::CLOSE && w::word(q, 16) != 0)
        {
            return Err(rt::DENIED);
        }
        let i = self.temporary.find(&owner).ok_or(rt::REVOKED)?;
        if self.pending.is_some() {
            return Err(w::BUSY);
        }
        let mut entry = self.temporary.open.swap_remove(i);
        // Un WRITE préparé dans une vue n’est jamais acquitté, abandonné ni
        // synchronisé au nom d'une autre vue ou de la session historique.
        core::mem::swap(&mut self.pending, &mut entry.pending);
        let result = self.dispatch(&mut entry.client, None, q, a);
        core::mem::swap(&mut self.pending, &mut entry.pending);
        self.temporary.open.push(entry);
        result
    }

    /// Un lot borné laisse une chance au canal VFS public même si des instances
    /// soumettent des demandes en continu. true indique de sonder ce canal sans dormir.
    pub(super) fn serve_tmp(&mut self, rx: u64) -> rt::Result<bool> {
        if !self.temporary.enabled {
            return Ok(false);
        }
        while self.pending.is_none() {
            let Some((owner, root)) = self.temporary.retired.pop() else {
                break;
            };
            if let Err(e @ (w::UNCERTAIN | w::IO | w::CORRUPT)) = self.purge_tmp(owner, root) {
                self.failed = Some(e);
            }
        }
        for _ in 0..8 {
            let mut envelope = [0; wire::ENVELOPE];
            let ticket = match rt::value(rt::call(
                wire::NEXT,
                rx,
                envelope.as_mut_ptr() as u64,
                envelope.len() as u64,
            )) {
                Err(rt::WOULD_BLOCK) => return Ok(false),
                result => result?,
            };
            let owner = envelope[..32].try_into().unwrap();
            let kind = u64::from_le_bytes(envelope[32..40].try_into().unwrap());
            let q: &Frame = envelope[40..].try_into().unwrap();
            let mut a = w::frame(q[1], 1, 0);
            let result = if kind == wire::REMOVE {
                self.retire_tmp(owner)
            } else if let Some(e) = self.failed {
                Err(e)
            } else {
                match kind {
                    wire::CREATE => self.create_tmp(owner),
                    wire::IO => self.private_request(owner, q, &mut a),
                    _ => Err(rt::BAD_ARGUMENT),
                }
            };
            let status = result.err().unwrap_or(rt::OK);
            if matches!(status, w::UNCERTAIN | w::IO | w::CORRUPT) {
                self.failed = Some(status);
            }
            let addr = if kind == wire::IO {
                a.as_ptr() as u64
            } else {
                0
            };
            match rt::value(rt::call(wire::COMPLETE, ticket, addr, status)) {
                Ok(_) | Err(rt::REVOKED) => (),
                Err(e) => return Err(e),
            }
        }
        Ok(true)
    }

    pub(super) fn finish_tmp(&mut self, rx: u64) -> rt::Result<()> {
        if self.pending.is_some() {
            return Err(w::BUSY);
        }
        while !self.temporary.open.is_empty() || !self.temporary.retired.is_empty() {
            let more = self.serve_tmp(rx)?;
            if !more && !self.temporary.open.is_empty() {
                match rt::receive_for(rx, 15000) {
                    Ok(_) | Err(rt::TIMED_OUT) => (),
                    Err(e) => return Err(e),
                }
            }
        }
        rt::value(rt::call(rt::IPC_CLOSE, rx, 0, 0))?;
        self.close_volume()
    }
}
