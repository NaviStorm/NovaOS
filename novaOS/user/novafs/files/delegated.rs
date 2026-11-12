//! Un seul enfant de premier plan. Deux endpoints neufs sont dérivés par IPC_GIVE.
//! Les numéros de handles de la session principale ne traversent jamais ce canal.
use super::*;
#[derive(Default)]
pub(super) struct Ports {
    rx: u64,
    tx: u64,
}
fn close(h: u64) {
    if h != 0 {
        rt::call(rt::IPC_CLOSE, h, 0, 0);
    }
}
impl Drop for Ports {
    fn drop(&mut self) {
        close(self.rx);
        close(self.tx);
    }
}
impl Ports {
    pub fn accept(&mut self, m: rt::ReceiveV1) {
        let dst = match (m.tag, m.aux) {
            (2, 2) => {
                *self = Self::default();
                &mut self.rx
            }
            (1, 1) if self.rx != 0 && self.tx == 0 => &mut self.tx,
            _ => {
                *self = Self::default();
                close(m.handle);
                return;
            }
        };
        close(core::mem::replace(dst, m.handle));
    }
}
pub(super) struct Grant {
    ports: Ports,
    client: Client,
    kind: u8,
    leaf: String,
    targets: Vec<selected::Target>,
    selected_id: Option<u64>,
}

pub(super) fn prepare(
    session: &Session,
    owner: &mut Client,
    q: &Frame,
    ports: &mut Ports,
) -> rt::Result<Grant> {
    let ports = core::mem::take(ports);
    if let Some(e) = session.failed {
        return Err(e);
    }
    // Les handles sont locaux. Aucun plan WRITE de la session source ne peut
    // être consulté ou publié depuis une délégation, même en cas de numéro égal.
    if session.pending.is_some() {
        return Err(w::BUSY);
    }
    shape(
        ports.rx != 0
            && ports.tx != 0
            && owner.mounted
            && !owner.closed
            && q[0] == w::VERSION
            && q[3..8] == [0; 5]
            && w::word(q, 8) == owner.id
            && q[24..].iter().all(|&b| b == 0),
    )?;
    let kind = q[2];
    let key = w::word(q, 16);
    let mut delegated = Client::new(1, 0);
    delegated.mounted = true;
    delegated.scoped = true;
    let mut leaf = String::new();
    let mut selected_id = None;
    let targets = core::mem::take(&mut owner.targets);
    match kind {
        w::GROUP => {
            shape(key == 0 && !targets.is_empty())?;
            selected::admit(&mut delegated, &targets)?;
        }
        w::CWD_VIEW => {
            shape(key == 0)?;
            delegated.cwd = owner.cwd;
        }
        w::FILE_VIEW => {
            let h = owner.handle(key)?;
            if h.kind != Kind::File || h.rights & w::READABLE == 0 {
                return Err(rt::DENIED);
            }
            delegated
                .handles
                .try_reserve(1)
                .map_err(|_| rt::NO_MEMORY)?;
            delegated.handles.push(File {
                key: 1,
                id: h.id,
                kind: Kind::File,
                rights: w::READABLE,
            });
        }
        w::DIR_VIEW | w::CREATE_ENTRY | w::REMOVE_ENTRY | w::REMOVE_EMPTY_DIR | w::REMOVE_TREE => {
            let h = owner.handle(key)?;
            let needed = w::TRAVERSE
                | match kind {
                    w::DIR_VIEW => w::ENUMERATE,
                    w::CREATE_ENTRY => w::ADD,
                    w::REMOVE_TREE => w::REMOVE | w::ENUMERATE,
                    _ => w::REMOVE,
                };
            if h.kind != Kind::Directory || h.rights & needed != needed {
                return Err(rt::DENIED);
            }
            delegated.cwd = (h.id, needed);
            if kind != w::DIR_VIEW {
                leaf = owner.take_path(0, &[])?;
                shape(!leaf.is_empty() && leaf.len() <= 255 && leaf != "." && !leaf.contains('/'))?;
                if matches!(kind, w::REMOVE_EMPTY_DIR | w::REMOVE_TREE) {
                    let fs = session.fs.as_ref().ok_or(rt::CLOSED)?;
                    let st = fs
                        .lookup_at(delegated.cwd.0, &leaf)
                        .map_err(namespace::error)?;
                    if kind == w::REMOVE_EMPTY_DIR && st.kind != Kind::Directory {
                        return Err(w::NOT_DIR);
                    }
                    // Ne pas commencer à vider un ancêtre du répertoire de travail.
                    let mut ancestor = owner.cwd.0;
                    for depth in 0..=Engine::<&IpcDevice>::MAX_DIRECTORY_DEPTH {
                        if ancestor == st.id {
                            return Err(w::BUSY);
                        }
                        if ancestor == fs.root_id() {
                            break;
                        }
                        if depth == Engine::<&IpcDevice>::MAX_DIRECTORY_DEPTH {
                            return Err(w::CORRUPT);
                        }
                        ancestor = fs.parent_name(ancestor).map_err(namespace::error)?.0;
                    }
                    selected_id = Some(st.id);
                    if kind == w::REMOVE_TREE {
                        delegated
                            .handles
                            .try_reserve(1)
                            .map_err(|_| rt::NO_MEMORY)?;
                        delegated.handles.push(File {
                            key: 1,
                            id: st.id,
                            kind: st.kind,
                            rights: if st.kind == Kind::Directory {
                                needed
                            } else {
                                0
                            },
                        });
                    }
                }
            }
        }
        _ => return Err(rt::BAD_ARGUMENT),
    }
    Ok(Grant {
        ports,
        kind,
        client: delegated,
        leaf,
        targets,
        selected_id,
    })
}

impl Grant {
    fn execute(
        &mut self,
        s: &mut Session<'_, '_>,
        owner: &Client,
        q: &Frame,
        a: &mut Frame,
    ) -> rt::Result<()> {
        let op = q[1];
        shape(q[0] == w::VERSION && q[3] == 0 && q[6..8] == [0; 2] && w::word(q, 8) == 1)?;
        if self.kind == w::GROUP {
            return selected::execute(s, &mut self.client, owner, &self.targets, q, a);
        }
        if matches!(self.kind, w::REMOVE_EMPTY_DIR | w::REMOVE_TREE) {
            return self.remove_selected(s, owner, q, a);
        }
        match (self.kind, op) {
            (w::CWD_VIEW, w::GETCWD) | (w::FILE_VIEW, w::READ) => {
                s.dispatch(&mut self.client, Some(owner), q, a)
            }
            (w::DIR_VIEW, w::READDIR | w::PATH_FRAGMENT) => {
                s.dispatch(&mut self.client, Some(owner), q, a)
            }
            (w::DIR_VIEW, w::LOOKUP_AT) => {
                let n = w::length(q);
                shape(
                    q[2] == 0
                        && n <= w::FRAGMENT
                        && q[16..40].iter().all(|&b| b == 0)
                        && q[w::PAYLOAD + n..].iter().all(|&b| b == 0),
                )?;
                let name = self.client.take_path(0, &q[w::PAYLOAD..w::PAYLOAD + n])?;
                shape(!name.is_empty() && name != "." && !name.contains('/'))?;
                let stat =
                    s.fs.as_ref()
                        .ok_or(rt::CLOSED)?
                        .lookup_at(self.client.cwd.0, &name)
                        .map_err(namespace::error)?;
                namespace::stat_reply(a, stat, s.fs.as_ref().ok_or(rt::CLOSED)?)
            }
            (w::CREATE_ENTRY, w::MKDIR_AT) | (w::REMOVE_ENTRY, w::UNLINK_AT) => {
                shape(
                    q[2..8] == [0; 6] && q[16..].iter().all(|&b| b == 0) && !self.leaf.is_empty(),
                )?;
                let bytes = core::mem::take(&mut self.leaf).into_bytes();
                self.client.paths[0] = PathInput {
                    total: bytes.len(),
                    bytes,
                };
                s.dispatch(&mut self.client, Some(owner), q, a)
            }
            _ => Err(rt::DENIED),
        }
    }
    fn remove_selected(
        &mut self,
        s: &mut Session<'_, '_>,
        owner: &Client,
        q: &Frame,
        a: &mut Frame,
    ) -> rt::Result<()> {
        let op = q[1];
        let key = w::word(q, 16);
        let id = self.selected_id.ok_or(rt::REVOKED)?;
        if key == 0 && matches!(op, w::RMDIR | w::UNLINK_AT) {
            if self.kind == w::REMOVE_EMPTY_DIR && op != w::RMDIR {
                return Err(rt::DENIED);
            }
            shape(q[2..8] == [0; 6] && q[16..].iter().all(|&b| b == 0))?;
            let fs = s.fs.as_ref().ok_or(rt::CLOSED)?;
            if fs
                .lookup_at(self.client.cwd.0, &self.leaf)
                .map_err(namespace::error)?
                .id
                != id
            {
                return Err(rt::REVOKED);
            }
            let bytes = self.leaf.as_bytes().to_vec();
            self.client.paths[0] = PathInput {
                total: bytes.len(),
                bytes,
            };
            s.dispatch(&mut self.client, Some(owner), q, a)?;
            self.selected_id = None;
            return Ok(());
        }
        if self.kind != w::REMOVE_TREE {
            return Err(rt::DENIED);
        }
        if op == w::PATH_FRAGMENT {
            return s.dispatch(&mut self.client, Some(owner), q, a);
        }
        if key == 0 {
            return Err(rt::DENIED);
        }
        self.client.handle(key)?;
        match op {
            w::STAT | w::READDIR | w::DIR_CLOSE => {}
            w::CLOSE if self.client.handle(key)?.kind == Kind::File => {}
            w::DIR_OPEN | w::LOOKUP_AT | w::UNLINK_AT | w::RMDIR => {
                let n = w::length(q);
                shape(n <= w::FRAGMENT && q[w::PAYLOAD + n..].iter().all(|&b| b == 0))?;
                // Un seul nom sous un handle déjà dérivé. Aucune résolution ambiante.
                let name = self.client.take_path(0, &q[w::PAYLOAD..w::PAYLOAD + n])?;
                if name.is_empty() || name == "." || name.contains('/') {
                    return Err(rt::DENIED);
                }
                if op == w::DIR_OPEN && q[2] & !(w::TRAVERSE | w::ENUMERATE | w::REMOVE) != 0 {
                    return Err(rt::DENIED);
                }
                let mut checked = *q;
                checked[4..6].fill(0);
                checked[w::PAYLOAD..].fill(0);
                self.client.paths[0] = PathInput {
                    total: name.len(),
                    bytes: name.into_bytes(),
                };
                return s.dispatch(&mut self.client, Some(owner), &checked, a);
            }
            _ => return Err(rt::DENIED),
        }
        s.dispatch(&mut self.client, Some(owner), q, a)
    }
    pub fn abandoned_copy(&mut self) -> Option<(u64, String, u64)> {
        self.client.copy.take()
    }
    pub fn serve(&mut self, s: &mut Session<'_, '_>, owner: &Client) -> rt::Result<()> {
        let mut last = 0;
        loop {
            let m = match rt::receive_for(self.ports.rx, 15000) {
                Err(rt::REVOKED | rt::DENIED | rt::CLOSED) => return Ok(()),
                Err(rt::TIMED_OUT) => continue,
                r => r?,
            };
            let mut a = w::frame(m.payload[1], 1, 0);
            let r = if m.kind == 1 && m.len == 128 && m.tag > last {
                last = m.tag;
                if let Some(e) = s.failed {
                    Err(e)
                } else {
                    self.execute(s, owner, &m.payload, &mut a)
                }
            } else {
                Err(rt::BAD_ARGUMENT)
            };
            if let Err(e) = r {
                a = w::frame(m.payload[1], 1, 0);
                a[3] = e as u8;
                if matches!(e, w::UNCERTAIN | w::CORRUPT | w::IO) {
                    s.failed = Some(e);
                }
            }
            match rt::send_for(self.ports.tx, m.tag, &a, 15000) {
                Err(rt::REVOKED | rt::DENIED | rt::CLOSED | rt::TIMED_OUT) => return Ok(()),
                r => r?,
            }
        }
    }
}
