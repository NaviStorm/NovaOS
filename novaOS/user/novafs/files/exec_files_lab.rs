//! Effecteur direct de lecture pour les objets provisionnés sur le volume de recette.
use super::*;
use novaos_identity::{
    Registry,
    lab::Ledger,
    policy::{Image, Origin, READ},
};

#[derive(Default, Clone, Copy)]
pub(super) struct OpenFile {
    pid: u64,
    next: u64,
    active: Option<(u64, u64)>,
}

fn config(index: usize) -> u64 {
    unsafe {
        (&*(0x401000 as *const core::sync::atomic::AtomicU64).add(index)).load(Ordering::Acquire)
    }
}

impl Session<'_, '_> {
    pub(super) fn exec_seal(&self) -> rt::Result<Option<[u8; 40]>> {
        let fs = self.fs.as_ref().ok_or(rt::CLOSED)?;
        let stat = match fs.lookup_at(fs.root_id(), "\u{1f}NXScope") {
            Ok(v) => v,
            Err(Error::NotFound) => return Ok(None),
            Err(e) => return Err(error(e)),
        };
        if stat.kind != Kind::File || stat.size != 40 {
            return Err(w::CORRUPT);
        }
        let seal: [u8; 40] = fs
            .inspect_id(stat.id, 0, 40)
            .map_err(error)?
            .try_into()
            .map_err(|_| w::CORRUPT)?;
        if &seal[..8] != b"NXScope1" {
            return Err(w::CORRUPT);
        }
        Ok(Some(seal))
    }

    pub(super) fn serve_exec_files(&mut self) -> rt::Result<bool> {
        let mut busy = false;
        for lane in 0..2 {
            let base = 96 + 16 * lane;
            let pid = config(base);
            if pid == 0 {
                continue;
            }
            let words: [u64; 16] = core::array::from_fn(|i| config(base + i));
            if config(base) != pid || words[0] != pid {
                continue;
            }
            let m = match rt::receive_for(words[1], 0) {
                Ok(v) => v,
                Err(rt::WOULD_BLOCK | rt::DENIED | rt::REVOKED | rt::CLOSED) => continue,
                Err(e) => return Err(e),
            };
            busy = true;
            if self.exec_open[lane].pid != pid {
                self.exec_open[lane] = OpenFile {
                    pid,
                    next: 0,
                    active: None,
                };
            }
            let mut a = w::frame(m.payload[1], 1, 0);
            let result = if m.kind == 1 && m.len == 128 && config(base) == pid {
                self.exec_request(lane, &words, &m.payload, &mut a)
            } else {
                Err(rt::BAD_ARGUMENT)
            };
            a[3] = result.err().unwrap_or(0) as u8;
            match rt::send_frame(words[2], m.tag, &a, 0, false) {
                Ok(()) | Err(rt::REVOKED | rt::DENIED | rt::CLOSED) => (),
                Err(e) => return Err(e),
            }
        }
        Ok(busy)
    }

    fn exec_request(
        &mut self,
        lane: usize,
        ctx: &[u64; 16],
        q: &Frame,
        a: &mut Frame,
    ) -> rt::Result<()> {
        if q[0] != w::VERSION || w::word(q, 8) != 1 || q[3] != 0 || q[6..8] != [0; 2] {
            return Err(rt::BAD_ARGUMENT);
        }
        let (dir, file, cwd) = (ctx[3], ctx[4], ctx[5]);
        let mut id = [0; 64];
        for (dst, word) in id.chunks_exact_mut(8).zip(&ctx[6..14]) {
            dst.copy_from_slice(&word.to_le_bytes());
        }
        let owner: [u8; 16] = id[..16].try_into().unwrap();
        let image = Image {
            installation: id[16..32].try_into().unwrap(),
            digest: id[32..].try_into().unwrap(),
        };
        let seal = self.exec_seal()?.ok_or(rt::REVOKED)?;
        if seal[8..24] != owner
            || seal[24..32] != dir.to_le_bytes()
            || seal[32..40] != file.to_le_bytes()
        {
            return Err(rt::REVOKED);
        }
        let mut ledger = Ledger::new(self.dev.identity_authority().map_err(error)?);
        let mut_fs = self.fs.as_mut().ok_or(rt::CLOSED)?;
        let reg = Registry::open(mut_fs, &mut ledger, [7; 16]).map_err(|_| rt::REVOKED)?;
        reg.verify_folder(owner, dir, &seal)
            .map_err(|_| rt::REVOKED)?;
        if !reg
            .permission(owner, image, owner, dir, Origin::Direct)
            .is_ok_and(|p| p.rights & READ != 0)
        {
            return Err(rt::REVOKED);
        }
        let fs = self.fs.as_ref().ok_or(rt::CLOSED)?;
        let stat = fs.stat_id(file).map_err(error)?;
        if stat.kind != Kind::File || fs.parent_name(file).map_err(error)?.0 != dir {
            return Err(rt::REVOKED);
        }
        let handle = w::word(q, 16);
        let entry = &mut self.exec_open[lane];
        if q[1] == w::OPEN_AT {
            let n = w::length(q);
            if handle != 0
                || q[2] != w::READABLE
                || !(1..=w::FRAGMENT).contains(&n)
                || q[24..40].iter().any(|b| *b != 0)
                || q[w::PAYLOAD + n..].iter().any(|b| *b != 0)
            {
                return Err(rt::DENIED);
            }
            let path = core::str::from_utf8(&q[w::PAYLOAD..w::PAYLOAD + n])
                .map_err(|_| rt::BAD_ARGUMENT)?;
            // Le seul objet autorisé possède une liaison physique attestée. Vérifier
            // chaque composante sans suivre de lien, ni autoriser des remontées.
            let mut at = if path.starts_with('/') {
                fs.root_id()
            } else {
                cwd
            };
            let tail = path.strip_prefix('/').unwrap_or(path);
            for part in tail.split('/') {
                if part.is_empty() || part == ".." || part.chars().any(char::is_control) {
                    return Err(rt::DENIED);
                }
                if part != "." {
                    at = fs.lookup_at(at, part).map_err(|_| rt::DENIED)?.id;
                }
            }
            if at != file {
                return Err(rt::DENIED);
            }
            if entry.active.is_some() {
                return Err(w::BUSY);
            }
            entry.next = entry.next.checked_add(1).ok_or(rt::LIMIT)?;
            entry.active = Some((entry.next, stat.revision));
            if option_env!("NOVA_ACTOR_TRACE") == Some("1") {
                let marker=alloc::format!("NXVFS ELF pid={} objet={} ouverture={}",ctx[0],file,entry.next);
                rt::call(rt::DEBUG_WRITE,marker.as_ptr() as u64,marker.len() as u64,0);
            }
            w::set(a, 16, entry.next);
            return Ok(());
        }
        let (_, revision) = entry
            .active
            .filter(|(k, _)| *k == handle)
            .ok_or(rt::REVOKED)?;
        if q[2] != 0 || stat.revision != revision {
            return Err(rt::REVOKED);
        }
        match q[1] {
            w::STAT if q[24..] == [0; 104] => namespace::stat_reply(a, stat, fs),
            w::CLOSE if q[24..] == [0; 104] => {
                entry.active = None;
                Ok(())
            }
            w::READ if w::length(q) <= w::FRAGMENT && q[32..].iter().all(|b| *b == 0) => {
                let offset = w::word(q, 24);
                let n = w::length(q);
                if offset
                    .checked_add(n as u64)
                    .is_none_or(|end| end > stat.size)
                {
                    return Err(rt::BAD_ARGUMENT);
                }
                let fs = self.fs.as_mut().ok_or(rt::CLOSED)?;
                let bytes = fs.read_access_id(file, offset, n).map_err(error)?;
                entry.active = Some((handle, fs.stat_id(file).map_err(error)?.revision));
                a[w::PAYLOAD..w::PAYLOAD + bytes.len()].copy_from_slice(&bytes);
                w::size(a, bytes.len());
                Ok(())
            }
            _ => Err(rt::DENIED),
        }
    }
}

// La route de montage ordinaire de nsh possède déjà les droits de parcours.
// La marque privée impose en plus le bail NovaAuth courant et le dossier attesté.
pub(super) const REDIRECT: u8 = 0x80;

#[derive(Clone, Copy)]
pub(super) struct Redirect {
    pub(super) key: u64,
    lease: u64,
    directory: u64,
}
impl Session<'_, '_> {
    pub(super) fn reap_redirects(&mut self, c: &mut Client) {
        let epoch = config(72);
        for slot in &mut c.redirections {
            if slot.is_some_and(|r| r.lease != epoch || c.closed) {
                let key = slot.take().unwrap().key;
                if self.pending.as_ref().is_some_and(|p| p.file == key) {
                    self.pending = None;
                }
                if c.read_window.as_ref().is_some_and(|w| w.handle == key) {
                    c.read_window = None;
                }
                c.handles.retain(|f| f.key != key);
            }
        }
    }
    fn redirect_authority(&mut self, epoch: u64) -> rt::Result<u64> {
        if epoch == 0 || config(72) != epoch {
            return Err(rt::REVOKED);
        }
        let mut owner = [0; 16];
        owner[..8].copy_from_slice(&config(73).to_le_bytes());
        owner[8..].copy_from_slice(&config(74).to_le_bytes());
        let seal = self.exec_seal()?.ok_or(rt::DENIED)?;
        if seal[8..24] != owner {
            return Err(rt::DENIED);
        }
        let directory = u64::from_le_bytes(seal[24..32].try_into().unwrap());
        let mut ledger = Ledger::new(self.dev.identity_authority().map_err(error)?);
        Registry::open(self.fs.as_mut().ok_or(rt::CLOSED)?, &mut ledger, [7; 16])
            .map_err(|_| rt::REVOKED)?
            .verify_folder(owner, directory, &seal)
            .map_err(|_| rt::REVOKED)?;
        rt::value(rt::call(
            rt::IPC_NOTIFY,
            rt::config(19),
            epoch,
            0x4e585246534944,
        ))?;
        Ok(directory)
    }
    pub(super) fn open_redirect(
        &mut self,
        c: &mut Client,
        q: &Frame,
        a: &mut Frame,
    ) -> rt::Result<()> {
        if c.id >> 63 != 0
            || c.scoped
            || c.private_root.is_some()
            || w::word(q, 16) != 0
            || q[24..40] != [0; 16]
        {
            return Err(rt::DENIED);
        }
        let mode = q[2] & !REDIRECT;
        let output = mode == w::CREATE | w::WRITABLE;
        let rights = if output {
            w::FILE_WRITE | w::ADD
        } else if mode == w::READABLE {
            w::FILE_READ
        } else {
            return Err(rt::BAD_ARGUMENT);
        };
        let path = c.take_path(0, &q[w::PAYLOAD..w::PAYLOAD + w::length(q)])?;
        let epoch = config(72);
        let directory = self.redirect_authority(epoch)?;
        let fs = self.fs.as_mut().ok_or(rt::CLOSED)?;
        let (base, access, tail) = if let Some(rest) = path.strip_prefix('/') {
            (fs.root_id(), c.rights, rest)
        } else {
            (c.cwd.0, c.cwd.1, path.as_str())
        };
        let needed = w::TRAVERSE | rights;
        if access & needed != needed {
            return Err(rt::DENIED);
        }
        let mut current = base;
        let mut parts = tail.split('/').peekable();
        let leaf = loop {
            let part = parts.next().ok_or(rt::BAD_ARGUMENT)?;
            if part.is_empty() || part == ".." || part.chars().any(char::is_control) {
                return Err(rt::DENIED);
            }
            if parts.peek().is_none() {
                if part == "." {
                    return Err(rt::BAD_ARGUMENT);
                }
                break part;
            }
            if part != "." {
                let node = fs.lookup_at(current, part).map_err(namespace::error)?;
                if node.kind != Kind::Directory {
                    return Err(w::NOT_DIR);
                }
                current = node.id;
            }
        };
        if current != directory {
            return Err(rt::DENIED);
        }
        let slot = c
            .redirections
            .iter()
            .position(Option::is_none)
            .ok_or(rt::LIMIT)?;
        let key = self
            .next
            .checked_add(1)
            .filter(|v| *v < 1 << 63)
            .ok_or(rt::LIMIT)?;
        if c.handles.len() >= 32 {
            return Err(rt::LIMIT);
        }
        c.handles.try_reserve(1).map_err(|_| rt::NO_MEMORY)?;
        let stat = if output {
            if self.pending.is_some() {
                return Err(w::BUSY);
            }
            // L'unicité du nom est décidée dans la transaction P9 elle-même.
            fs.create_at(current, leaf, Kind::File)
                .map_err(namespace::error)?
        } else {
            fs.lookup_at(current, leaf).map_err(namespace::error)?
        };
        if stat.kind != Kind::File {
            return Err(w::IS_DIR);
        }
        self.next = key;
        c.handles.push(File {
            key,
            id: stat.id,
            kind: stat.kind,
            rights: mode & 3,
        });
        c.redirections[slot] = Some(Redirect {
            key,
            lease: epoch,
            directory,
        });
        w::set(a, 16, key);
        Ok(())
    }

    pub(super) fn check_redirect(&mut self, c: &mut Client, op: u8, key: u64) -> rt::Result<()> {
        let Some(i) = c
            .redirections
            .iter()
            .position(|v| v.is_some_and(|r| r.key == key))
        else {
            return Ok(());
        };
        let entry = c.redirections[i].unwrap();
        if !matches!(op, w::READ | w::WRITE | w::SYNC | w::CLOSE) {
            return Err(rt::DENIED);
        }
        let permitted = if op == w::CLOSE {
            Ok(())
        } else {
            self.redirect_authority(entry.lease).and_then(|dir| {
                let object = c
                    .handles
                    .iter()
                    .find(|f| f.key == key)
                    .ok_or(rt::REVOKED)?
                    .id;
                if entry.directory != dir
                    || self
                        .fs
                        .as_ref()
                        .ok_or(rt::CLOSED)?
                        .parent_name(object)
                        .map_err(namespace::error)?
                        .0
                        != dir
                {
                    return Err(rt::REVOKED);
                }
                Ok(())
            })
        };
        if permitted.is_err() {
            if self.pending.as_ref().is_some_and(|p| p.file == key) {
                self.pending = None;
            }
            c.handles.retain(|f| f.key != key);
            c.redirections[i] = None;
        }
        permitted
    }
}
