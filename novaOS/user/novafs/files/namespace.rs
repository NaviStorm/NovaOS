//! Les identifiants persistants restent attachés au montage. Seul le canal
//! privé du client donne autorité sur les handles de sa table.
use super::*;

// Les nouvelles opérations distinguent les erreurs de catalogue. OPEN v2
// conserve les statuts déjà compris par les anciens clients.
pub(super) fn error(e: Error) -> u64 {
    match e {
        Error::NotDirectory => w::NOT_DIR,
        Error::IsDirectory => w::IS_DIR,
        Error::Exists => w::EXISTS,
        Error::NotEmpty => w::NOT_EMPTY,
        e => super::error(e),
    }
}

fn rights(actual: u8, needed: u8) -> rt::Result<()> {
    if actual & needed == needed {
        Ok(())
    } else {
        Err(rt::DENIED)
    }
}

impl Client {
    fn directory(&self, key: u64) -> rt::Result<(u64, u8)> {
        if key == 0 {
            return Ok(self.cwd);
        }
        let h = self.handle(key)?;
        if h.kind != Kind::Directory {
            return Err(w::NOT_DIR);
        }
        Ok((h.id, h.rights))
    }
    fn append_path(&mut self, q: &Frame) -> rt::Result<()> {
        let n = w::length(q);
        let total = usize::try_from(w::word(q, 32)).map_err(|_| rt::LIMIT)?;
        let offset = usize::try_from(w::word(q, 24)).map_err(|_| rt::LIMIT)?;
        let p = self.paths.get_mut(q[2] as usize).ok_or(rt::BAD_ARGUMENT)?;
        shape(w::word(q, 16) == 0 && n > 0 && total > 0)?;
        if total > w::PATH_MAX {
            return Err(rt::LIMIT);
        }
        shape(offset.checked_add(n).is_some_and(|v| v <= total))?;
        if offset == 0 {
            p.bytes.clear();
            p.total = 0;
            p.bytes
                .try_reserve_exact(total)
                .map_err(|_| rt::NO_MEMORY)?;
            p.total = total;
        }
        shape(total == p.total && offset == p.bytes.len())?;
        p.bytes.extend_from_slice(&q[w::PAYLOAD..w::PAYLOAD + n]);
        Ok(())
    }
    pub(super) fn take_path(&mut self, index: usize, inline: &[u8]) -> rt::Result<String> {
        let p = core::mem::take(&mut self.paths[index]);
        let bytes = if inline.is_empty() {
            shape(p.total != 0 && p.bytes.len() == p.total)?;
            p.bytes
        } else {
            shape(p.total == 0)?;
            inline.to_vec()
        };
        let path = String::from_utf8(bytes).map_err(|_| rt::BAD_ARGUMENT)?;
        shape(!path.bytes().any(|b| b < 32 || b == 127))?;
        if path.split('/').any(|name| name == "..") {
            return Err(rt::DENIED);
        }
        Ok(path)
    }
}

pub(super) fn stat_reply(
    a: &mut Frame,
    stat: novafs::Stat,
    fs: &Engine<&IpcDevice>,
) -> rt::Result<()> {
    let p = &mut a[w::PAYLOAD..];
    p[..8].copy_from_slice(&stat.id.to_le_bytes());
    p[8..16].copy_from_slice(&stat.revision.to_le_bytes());
    p[16..24].copy_from_slice(&stat.size.to_le_bytes());
    p[24] = if stat.kind == Kind::Directory { 2 } else { 1 };
    if fs.atime_policy().is_some() {
        p[25..73].copy_from_slice(&fs.times_id(stat.id).map_err(error)?.encode());
        w::size(a, 73);
    } else {
        w::size(a, 25);
    }
    Ok(())
}

fn fragment(a: &mut Frame, bytes: &[u8], offset: usize) -> rt::Result<()> {
    let tail = bytes.get(offset..).ok_or(rt::BAD_ARGUMENT)?;
    let len = tail.len().min(w::FRAGMENT);
    a[w::PAYLOAD..w::PAYLOAD + len].copy_from_slice(&tail[..len]);
    w::set(a, 32, bytes.len() as u64);
    w::size(a, len);
    Ok(())
}

impl Session<'_, '_> {
    pub(super) fn directory_request(
        &mut self,
        c: &mut Client,
        peer: Option<&Client>,
        q: &Frame,
        a: &mut Frame,
    ) -> rt::Result<()> {
        let (op, flags, key, offset, total, n) = (
            q[1],
            q[2],
            w::word(q, 16),
            w::word(q, 24),
            w::word(q, 32),
            w::length(q),
        );
        let inline = &q[w::PAYLOAD..w::PAYLOAD + n];
        let fs = self.fs.as_mut().ok_or(rt::CLOSED)?;
        if op == w::PATH_FRAGMENT {
            return c.append_path(q);
        }
        if op == w::STAT {
            shape(flags == 0 && n == 0 && offset == 0 && total == 0)?;
            let id = if key == 0 { c.cwd.0 } else { c.handle(key)?.id };
            stat_reply(a, fs.stat_id(id).map_err(error)?, fs)?;
            return Ok(());
        }
        let (mut base, mut permitted) = c.directory(key)?;
        if op == w::DIR_CLOSE {
            shape(key != 0 && flags == 0 && n == 0 && offset == 0 && total == 0)?;
            c.handles.swap_remove(
                c.handles
                    .iter()
                    .position(|h| h.key == key)
                    .ok_or(rt::REVOKED)?,
            );
            return Ok(());
        }
        if op == w::CHDIR {
            shape(flags == 0 && n == 0 && offset == 0 && total == 0)?;
            rights(permitted, w::TRAVERSE)?;
            fs.stat_id(base).map_err(error)?;
            c.cwd = (base, permitted);
            return Ok(());
        }
        if op == w::GETCWD {
            shape(key == 0 && flags == 0 && n == 0)?;
            let generation = fs.generation();
            if offset != 0 && total != generation {
                return Err(rt::REVOKED);
            }
            let mut node = base;
            // Seul GETCWD demande les noms du chemin affiché. Le P8 borne
            // à 32 les composants temporaires ; aucun chemin absolu n'est
            // matérialisé et la résolution ordinaire ne parcourt pas les parents.
            let mut names = Vec::new();
            while node != fs.root_id() {
                if names.len() == Engine::<&IpcDevice>::MAX_DIRECTORY_DEPTH {
                    return Err(w::CORRUPT);
                }
                names.try_reserve(1).map_err(|_| rt::NO_MEMORY)?;
                let (parent, name) = fs.parent_name(node).map_err(error)?;
                names.push(name);
                node = parent;
            }
            let mut pos = 0usize;
            let start = usize::try_from(offset).map_err(|_| rt::BAD_ARGUMENT)?;
            let mut wrote = 0;
            let mut part = |bytes: &[u8]| {
                let begin = start.saturating_sub(pos).min(bytes.len());
                let count = (bytes.len() - begin).min(w::FRAGMENT - wrote);
                a[w::PAYLOAD + wrote..w::PAYLOAD + wrote + count]
                    .copy_from_slice(&bytes[begin..begin + count]);
                wrote += count;
                pos += bytes.len();
            };
            if names.is_empty() {
                part(b"/");
            }
            for name in names.iter().rev() {
                part(b"/");
                part(name.as_bytes());
            }
            shape(start <= pos)?;
            w::size(a, wrote);
            w::set(a, 24, generation);
            w::set(a, 32, pos as u64);
            return Ok(());
        }
        if op == w::READDIR {
            shape(flags == 0)?;
            rights(permitted, w::ENUMERATE)?;
            let generation = fs.generation();
            if (total != 0 || offset != 0) && total != generation {
                return Err(rt::REVOKED);
            }
            let after = if n == 0 && c.paths[0].total == 0 {
                None
            } else {
                Some(c.take_path(0, inline)?)
            };
            if self.pending.is_some()
                && fs
                    .atime_policy()
                    .is_some_and(|v| v != novafs::time::Atime::No)
            {
                return Err(w::BUSY);
            }
            let names = if offset == 0 {
                fs.list_access_id(base, after.as_deref(), 1)
            } else {
                fs.inspect_dir(base, after.as_deref(), 1)
            }
            .map_err(error)?;
            let bytes = names.first().map_or(&[][..], |s| s.as_bytes());
            w::set(a, 24, fs.generation());
            return fragment(
                a,
                bytes,
                usize::try_from(offset).map_err(|_| rt::BAD_ARGUMENT)?,
            );
        }
        shape(offset == 0 && (op == w::RENAME_AT || total == 0))?;
        let name = c.take_path(0, inline)?;
        let path = if let Some(rest) = name.strip_prefix('/') {
            // Un nom absolu exige l'autorité de la session de montage. Une
            // capacité de sous-répertoire explicite ne permet pas ce raccourci.
            if key != 0 {
                return Err(rt::DENIED);
            }
            base = fs.root_id();
            permitted = c.rights;
            if rest.is_empty() { "." } else { rest }
        } else {
            &name
        };
        rights(permitted, w::TRAVERSE)?;
        let mutating = matches!(op, w::MKDIR_AT | w::RMDIR | w::UNLINK_AT | w::RENAME_AT)
            || op == w::OPEN_AT && flags & w::CREATE != 0;
        if mutating && self.pending.is_some() {
            return Err(w::BUSY);
        }
        match op {
            w::LOOKUP_AT => {
                shape(flags == 0)?;
                stat_reply(a, fs.lookup_at(base, path).map_err(error)?, fs)?;
            }
            w::DIR_OPEN | w::OPEN_AT => {
                if c.handles.len() >= 32 {
                    return Err(rt::LIMIT);
                }
                let next = self
                    .next
                    .checked_add(1)
                    .filter(|v| *v < 1u64 << 63)
                    .ok_or(rt::LIMIT)?;
                c.handles.try_reserve(1).map_err(|_| rt::NO_MEMORY)?;
                let (kind, grant) = if op == w::DIR_OPEN {
                    shape(flags != 0 && flags & !w::DIR_ALL == 0)?;
                    rights(permitted, flags)?;
                    (Kind::Directory, flags)
                } else {
                    shape(flags & 3 != 0 && flags & !7 == 0)?;
                    if flags & w::READABLE != 0 {
                        rights(permitted, w::FILE_READ)?;
                    }
                    if flags & w::WRITABLE != 0 {
                        rights(permitted, w::FILE_WRITE)?;
                    }
                    if flags & w::CREATE != 0 {
                        shape(flags & w::WRITABLE != 0)?;
                        rights(permitted, w::ADD)?;
                    }
                    (Kind::File, flags & 3)
                };
                let stat = match fs.lookup_at(base, path) {
                    Err(Error::NotFound) if op == w::OPEN_AT && flags & w::CREATE != 0 => {
                        fs.create_at(base, path, Kind::File).map_err(error)?
                    }
                    r => r.map_err(error)?,
                };
                if kind != stat.kind {
                    return Err(if kind == Kind::File {
                        w::IS_DIR
                    } else {
                        w::NOT_DIR
                    });
                }
                self.next = next;
                let next = if c.scoped { next | 1u64 << 63 } else { next };
                c.handles.push(File {
                    key: next,
                    id: stat.id,
                    kind,
                    rights: grant,
                });
                w::set(a, 16, next);
                stat_reply(a, stat, fs)?;
            }
            w::MKDIR_AT => {
                shape(flags == 0)?;
                rights(permitted, w::ADD)?;
                fs.create_at(base, path, Kind::Directory).map_err(error)?;
            }
            w::UNLINK_AT | w::RMDIR => {
                shape(flags == 0)?;
                rights(permitted, w::REMOVE)?;
                let kind = if op == w::RMDIR {
                    Kind::Directory
                } else {
                    Kind::File
                };
                let id = fs.lookup_at(base, path).map_err(error)?.id;
                let busy = |v: &Client| {
                    v.mounted
                        && !v.closed
                        && (v.cwd.0 == id || v.handles.iter().any(|h| h.id == id))
                };
                if busy(c) || peer.is_some_and(busy) {
                    return Err(w::BUSY);
                }
                fs.unlink_at(base, path, kind).map_err(error)?;
            }
            w::RENAME_AT => {
                shape(flags == 0 && n == 0)?;
                let (dest, rights_dest) = c.directory(total)?;
                rights(permitted, w::MOVE | w::REMOVE)?;
                rights(rights_dest, w::MOVE | w::ADD | w::TRAVERSE)?;
                let to = c.take_path(1, &[])?;
                self.dev.start_probe();
                fs.rename_at(base, path, dest, &to).map_err(error)?;
            }
            _ => return Err(w::UNSUPPORTED),
        }
        Ok(())
    }
}
