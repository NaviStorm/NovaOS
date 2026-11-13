//! Le parent prépare des vues par opérande avant la publication de l'enfant.
//! Un emplacement reconnaît exactement un nom dans un parent déjà résolu.
use super::*;

pub(super) struct Target {
    scope: u8,
    id: u64,
    name: String,
    kind: Kind,
    basename: String,
}

pub(super) fn stage(s: &Session, c: &mut Client, q: &Frame) -> rt::Result<()> {
    if let Some(e) = s.failed {
        return Err(e);
    }
    let n = w::length(q);
    shape(
        c.mounted
            && !c.closed
            && q[0] == w::VERSION
            && q[3] == 0
            && q[6..8] == [0; 2]
            && w::word(q, 8) == c.id
            && q[16..40] == [0; 24]
            && n <= w::FRAGMENT
            && q[w::PAYLOAD + n..].iter().all(|&b| b == 0),
    )?;
    let scope = q[2];
    if scope == 0 {
        shape(n == 0)?;
        c.targets.clear();
        c.paths = Default::default();
        return Ok(());
    }
    if c.targets.len() >= 32 {
        return Err(rt::LIMIT);
    }
    let name = c.take_path(0, &q[w::PAYLOAD..w::PAYLOAD + n])?;
    let fs = s.fs.as_ref().ok_or(rt::CLOSED)?;
    let (base, path, rights) = if let Some(p) = name.strip_prefix('/') {
        (fs.root_id(), if p.is_empty() { "." } else { p }, c.rights)
    } else {
        (c.cwd.0, name.as_str(), c.cwd.1)
    };
    let needed = w::TRAVERSE
        | match scope {
            w::SELECT_FILE => w::FILE_READ,
            w::SELECT_LIST | w::SELECT_TREE => w::ENUMERATE,
            w::SELECT_COPY_TREE => w::ENUMERATE | w::FILE_READ,
            w::SELECT_NEW | w::SELECT_TOUCH => w::ADD | w::FILE_WRITE,
            w::SELECT_COPY_DEST => w::ADD | w::FILE_WRITE,
            w::SELECT_FROM => w::MOVE | w::REMOVE,
            w::SELECT_TO => w::MOVE | w::ADD,
            _ => return Err(rt::BAD_ARGUMENT),
        };
    if rights & needed != needed {
        return Err(rt::DENIED);
    }
    let destination = matches!(scope, w::SELECT_NEW | w::SELECT_COPY_DEST | w::SELECT_TO);
    let directory_dest = if destination {
        match fs.lookup_at(base, path) {
            Ok(st) if st.kind == Kind::Directory => Some(st.id),
            Ok(_) | Err(Error::NotFound) => None,
            Err(e) => return Err(namespace::error(e)),
        }
    } else {
        None
    };
    let existing_touch = if scope == w::SELECT_TOUCH {
        match fs.lookup_at(base, path) {
            Ok(st) => Some(st),
            Err(Error::NotFound) => None,
            Err(e) => return Err(namespace::error(e)),
        }
    } else {
        None
    };
    let (id, leaf, kind) = if let Some(st) = existing_touch {
        (st.id, String::new(), st.kind)
    } else if let Some(dir) = directory_dest {
        let source = c.targets.first().ok_or(rt::BAD_ARGUMENT)?;
        shape(!source.basename.is_empty())?;
        if source.name.is_empty() && source.id == dir {
            return Err(rt::BAD_ARGUMENT);
        }
        (dir, source.basename.clone(), Kind::Directory)
    } else if matches!(
        scope,
        w::SELECT_FILE | w::SELECT_LIST | w::SELECT_TREE | w::SELECT_COPY_TREE
    ) {
        let st = fs.lookup_at(base, path).map_err(namespace::error)?;
        if scope == w::SELECT_FILE && st.kind != Kind::File {
            return Err(w::IS_DIR);
        }
        (st.id, String::new(), st.kind)
    } else {
        let (parent, leaf) = path.rsplit_once('/').unwrap_or((".", path));
        shape(!matches!(leaf, "" | "." | "..") && leaf.len() <= 255 && !parent.is_empty())?;
        let st = fs.lookup_at(base, parent).map_err(namespace::error)?;
        if st.kind != Kind::Directory {
            return Err(w::NOT_DIR);
        }
        (st.id, String::from(leaf), Kind::Directory)
    };
    let basename = if !leaf.is_empty() {
        leaf.clone()
    } else if scope == w::SELECT_FILE || scope == w::SELECT_COPY_TREE {
        fs.parent_name(id).map_err(namespace::error)?.1
    } else {
        String::new()
    };
    c.targets.try_reserve(1).map_err(|_| rt::NO_MEMORY)?;
    c.targets.push(Target {
        scope,
        id,
        name: leaf,
        kind,
        basename,
    });
    Ok(())
}

pub(super) fn admit(c: &mut Client, targets: &[Target]) -> rt::Result<()> {
    c.handles
        .try_reserve(targets.len())
        .map_err(|_| rt::NO_MEMORY)?;
    if let [dest, src] = targets
        && dest.scope == w::SELECT_NEW
        && src.scope == w::SELECT_FILE
    {
        c.copy_source = Some(2);
    }
    for (i, t) in targets.iter().enumerate() {
        let rights = match t.scope {
            w::SELECT_FILE => w::READABLE,
            w::SELECT_TREE | w::SELECT_COPY_TREE if t.kind == Kind::Directory => {
                w::TRAVERSE
                    | w::ENUMERATE
                    | if t.scope == w::SELECT_COPY_TREE {
                        w::FILE_READ
                    } else {
                        0
                    }
            }
            w::SELECT_COPY_TREE => w::READABLE,
            w::SELECT_LIST if t.kind == Kind::Directory => w::TRAVERSE | w::ENUMERATE,
            w::SELECT_LIST => 0,
            _ => continue,
        };
        c.handles.push(File {
            key: 1 + i as u64,
            id: t.id,
            kind: t.kind,
            rights,
        });
    }
    Ok(())
}

fn put_handle(
    c: &mut Client,
    s: &mut Session,
    id: u64,
    kind: Kind,
    rights: u8,
    a: &mut Frame,
) -> rt::Result<()> {
    // Les slots des opérandes occupent la moitié basse. Le bit haut identifie
    // les handles dérivés pendant la commande et évite toute collision.
    s.next = s
        .next
        .checked_add(1)
        .filter(|v| *v < 1u64 << 63)
        .ok_or(rt::LIMIT)?;
    let key = s.next | 1u64 << 63;
    c.handles.push(File {
        key,
        id,
        kind,
        rights,
    });
    w::set(a, 16, key);
    Ok(())
}

pub(super) fn execute(
    s: &mut Session,
    c: &mut Client,
    owner: &Client,
    targets: &[Target],
    q: &Frame,
    a: &mut Frame,
) -> rt::Result<()> {
    let op = q[1];
    let key = w::word(q, 16);
    if op == w::COPY_RANGE {
        return copy_range(s, c, q, a);
    }
    if op == w::PATH_FRAGMENT {
        return s.dispatch(c, Some(owner), q, a);
    }
    if key == 0 {
        return Err(rt::DENIED);
    }
    let target = key
        .checked_sub(1)
        .and_then(|i| usize::try_from(i).ok())
        .and_then(|i| targets.get(i));
    if matches!(
        op,
        w::CREATE_NEW | w::TOUCH_SELECTED | w::MOVE_SELECTED | w::PUBLISH_COPY
    ) {
        let t = target.ok_or(rt::DENIED)?;
        shape(q[4..8] == [0; 4] && q[24..] == [0; 104])?;
        if s.pending.is_some() {
            return Err(w::BUSY);
        }
        let fs = s.fs.as_mut().ok_or(rt::CLOSED)?;
        let now = if fs.atime_policy().is_some() {
            Some(s.clock.sample()?)
        } else {
            None
        };
        match op {
            w::CREATE_NEW if matches!(t.scope, w::SELECT_NEW | w::SELECT_COPY_DEST) => {
                if c.handles.len() >= 32 {
                    return Err(rt::LIMIT);
                }
                c.handles.try_reserve(1).map_err(|_| rt::NO_MEMORY)?;
                shape(q[2] == 0 || (q[2] == 1 && t.scope == w::SELECT_COPY_DEST))?;
                let dir = q[2] == 1;
                if dir && let Some(src) = targets.first() {
                    let mut parent = t.id;
                    loop {
                        if parent == src.id {
                            return Err(rt::BAD_ARGUMENT);
                        }
                        if parent == fs.root_id() {
                            break;
                        }
                        parent = fs.parent_name(parent).map_err(namespace::error)?.0;
                    }
                }
                let kind = if dir { Kind::Directory } else { Kind::File };
                if c.copy.is_some() {
                    return Err(w::BUSY);
                }
                match fs.lookup_at(t.id, &t.name) {
                    Err(Error::NotFound) => {}
                    Ok(_) => return Err(w::EXISTS),
                    Err(e) => return Err(namespace::error(e)),
                }
                let temp = alloc::format!(".nova-copie-{:016x}", fs.generation());
                if t.name == temp {
                    return Err(w::EXISTS);
                }
                let st = fs.create_at(t.id, &temp, kind).map_err(namespace::error)?;
                c.copy = Some((t.id, temp, st.id));
                put_handle(
                    c,
                    s,
                    st.id,
                    kind,
                    if dir {
                        w::TRAVERSE | w::ADD | w::FILE_WRITE
                    } else {
                        w::WRITABLE
                    },
                    a,
                )
            }
            w::PUBLISH_COPY if matches!(t.scope, w::SELECT_NEW | w::SELECT_COPY_DEST) => {
                shape(q[2] == 0)?;
                let (parent, name, id) = c.copy.as_ref().ok_or(rt::BAD_ARGUMENT)?;
                if *parent != t.id
                    || fs.lookup_at(*parent, name).map_err(namespace::error)?.id != *id
                {
                    return Err(rt::REVOKED);
                }
                fs.rename_at(*parent, name, t.id, &t.name)
                    .map_err(namespace::error)?;
                c.copy = None;
                Ok(())
            }
            w::TOUCH_SELECTED if t.scope == w::SELECT_TOUCH => {
                shape(q[2] < 8 && q[2] & 3 != 0)?;
                let object = if t.name.is_empty() {
                    fs.stat_id(t.id)
                } else {
                    fs.lookup_at(t.id, &t.name)
                };
                let st = match object {
                    Err(Error::NotFound) if q[2] & 4 != 0 => return Ok(()),
                    Err(Error::NotFound) => {
                        now.ok_or(w::UNSUPPORTED)?;
                        fs.create_at(t.id, &t.name, Kind::File)
                            .map_err(namespace::error)?;
                        return Ok(());
                    }
                    r => r.map_err(namespace::error)?,
                };
                let stamp = now.ok_or(w::UNSUPPORTED)?;
                fs.set_times_id(
                    st.id,
                    (q[2] & 1 != 0).then_some(stamp),
                    (q[2] & 2 != 0).then_some(stamp),
                )
                .map_err(namespace::error)
            }
            w::MOVE_SELECTED
                if t.scope == w::SELECT_FROM
                    && targets.len() == 2
                    && targets[1].scope == w::SELECT_TO =>
            {
                shape(q[2] == 0)?;
                let dest = &targets[1];
                fs.rename_at(t.id, &t.name, dest.id, &dest.name)
                    .map_err(namespace::error)
            }
            _ => Err(rt::DENIED),
        }
    } else if op == w::STAT
        && let Some(t) = target
        && !t.name.is_empty()
    {
        shape(q[2..8] == [0; 6] && q[24..] == [0; 104])?;
        let fs = s.fs.as_ref().ok_or(rt::CLOSED)?;
        let st = fs.lookup_at(t.id, &t.name).map_err(namespace::error)?;
        namespace::stat_reply(a, st, fs)
    } else {
        let h = c.handle(key)?;
        match op {
            w::READ | w::SYNC | w::WRITE | w::CLOSE | w::DIR_CLOSE | w::READDIR | w::STAT => {}
            w::LOOKUP_AT | w::DIR_OPEN | w::OPEN_AT | w::MKDIR_AT => {
                if h.kind != Kind::Directory {
                    return Err(w::NOT_DIR);
                }
                if target.is_some_and(|t| t.scope == w::SELECT_LIST) {
                    if op != w::LOOKUP_AT {
                        return Err(rt::DENIED);
                    }
                    let n = w::length(q);
                    shape(n <= w::FRAGMENT)?;
                    let name = c.take_path(0, &q[w::PAYLOAD..w::PAYLOAD + n])?;
                    shape(!name.is_empty() && name != "." && !name.contains('/'))?;
                    let fs = s.fs.as_ref().ok_or(rt::CLOSED)?;
                    let st = fs
                        .lookup_at(c.handle(key)?.id, &name)
                        .map_err(namespace::error)?;
                    return namespace::stat_reply(a, st, fs);
                }
                // N'autoriser les créations que dans le nouvel arbre destination.
                if op == w::OPEN_AT && q[2] & w::WRITABLE != 0 {
                    if q[2] != w::WRITABLE | w::CREATE {
                        return Err(rt::DENIED);
                    }
                }
            }
            _ => return Err(rt::DENIED),
        }
        s.dispatch(c, Some(owner), q, a)
    }
}

/// Le service traite une seule requête et ne quitte jamais le groupe exclusif.
fn copy_range(s: &mut Session, c: &Client, q: &Frame, a: &mut Frame) -> rt::Result<()> {
    let dest = c.handle(w::word(q, 16))?;
    let src = c.handle(c.copy_source.ok_or(rt::DENIED)?)?;
    if dest.rights & w::WRITABLE == 0
        || src.rights & w::READABLE == 0
        || c.copy.as_ref().is_none_or(|(_, _, id)| *id != dest.id)
        || src.id == dest.id
    {
        return Err(rt::DENIED);
    }
    shape(
        matches!(q[2], 0 | w::TEXT_RANGE) && q[4..8] == [0; 4] && q[56..].iter().all(|x| *x == 0),
    )?;
    if s.pending.is_some() {
        return Err(w::BUSY);
    }
    let source_at = w::word(q, 24);
    let dst_at = w::word(q, 32);
    let expected = w::word(q, 40);
    let len = usize::try_from(w::word(q, 48)).map_err(|_| rt::BAD_ARGUMENT)?;
    shape((1..=w::MAX_WRITE).contains(&len))?;
    let fs = s.fs.as_mut().ok_or(rt::CLOSED)?;
    let st = fs.stat_id(src.id).map_err(error)?;
    if st.revision != expected {
        return Err(rt::REVOKED);
    }
    let end = source_at.checked_add(len as u64).ok_or(rt::BAD_ARGUMENT)?;
    shape(
        end <= st.size
            && dst_at
                .checked_add(len as u64)
                .is_some_and(|n| n <= i64::MAX as u64),
    )?;
    if fs.stat_id(dest.id).map_err(error)?.size != dst_at {
        return Err(rt::REVOKED);
    }
    if fs.atime_policy().is_some() {
        s.clock.sample()?;
    }
    let bytes = fs.read_access_id(src.id, source_at, len).map_err(error)?;
    shape(bytes.len() == len)?;
    let next = fs.stat_id(src.id).map_err(error)?.revision;
    let used = if q[2] == w::TEXT_RANGE {
        text_length(&bytes, end == st.size).ok_or(rt::BAD_ARGUMENT)?
    } else {
        len
    };
    if fs.atime_policy().is_some() {
        s.clock.sample()?;
    }
    s.dev.start_probe();
    fs.prepare_write_id(dest.id, dst_at, &bytes[..used])
        .map_err(error)?
        .commit()
        .map_err(error)?;
    w::set(a, 24, used as u64);
    w::set(a, 32, next);
    Ok(())
}

/// Une fin UTF-8 ou CRLF incomplète est relue au prochain lot.
fn text_length(data: &[u8], last: bool) -> Option<usize> {
    let text = match core::str::from_utf8(data) {
        Ok(s) => s,
        Err(e) if !last && e.error_len().is_none() => {
            core::str::from_utf8(&data[..e.valid_up_to()]).ok()?
        }
        _ => return None,
    };
    let s = if last {
        text
    } else {
        text.strip_suffix('\r').unwrap_or(text)
    };
    if s.is_empty()
        || s.match_indices('\r')
            .any(|(i, _)| !s[i..].starts_with("\r\n"))
        || s.chars()
            .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
    {
        None
    } else {
        Some(s.len())
    }
}

/// Après révocation, retirer exclusivement la candidate identifiée à la création.
/// Le service est le seul écrivain. La pile de parcours est bornée par la
/// profondeur persistante de P9 et chaque retrait reste transactionnel.
pub(super) fn discard(
    fs: &mut Engine<&IpcDevice>,
    parent: u64,
    name: &str,
    id: u64,
) -> novafs::Result<()> {
    let st = fs.lookup_at(parent, name)?;
    if st.id != id {
        return Err(Error::Corrupt("identité de la copie de travail"));
    }
    let mut stack = alloc::vec![(parent, String::from(name), st)];
    while let Some((parent, name, st)) = stack.last() {
        if st.kind == Kind::Directory
            && let Some(next) = fs.inspect_dir(st.id, None, 1)?.pop()
        {
            let child = fs.lookup_at(st.id, &next)?;
            stack.push((st.id, next, child));
        } else {
            fs.unlink_at(*parent, name, st.kind)?;
            stack.pop();
        }
    }
    Ok(())
}
