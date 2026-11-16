//! Flux borné sur les dépôts IPC v1. L'autorité de contrôle ne transporte
//! pas d'octets. Chaque direction peut être installée une seule fois.
use super::*;

#[derive(Clone, Copy, Default)]
pub(super) struct Pipe {
    writer: Option<Pid>,
    reader: Option<Pid>,
    pub(super) writer_closed: bool,
    pub(super) reader_closed: bool,
    pub(super) revoked: bool,
    pub(super) trace: bool,
    pub(super) peak: usize,
    pub(super) blocked: usize,
    pub(super) delivered: u64,
    pub(super) fingerprint: u64,
}

impl Domain {
    pub(super) fn pipe_open(&mut self, owner: Pid, trace: bool) -> Res<u64> {
        let key = self.create(owner)?;
        let root = self.lookup(owner, key, ALL)?;
        self.nodes.get_mut(root.id.slot()).rights = MANAGE;
        self.ends.get_mut(root.endpoint.slot()).pipe = Some(Pipe {
            trace,
            fingerprint: 0xcbf29ce484222325,
            ..Pipe::default()
        });
        Ok(key)
    }

    pub(super) fn pipe_check(&self, parent: Pid, key: u64, writer: Option<Pid>) -> Res<Node> {
        let root = self.lookup(parent, key, MANAGE)?;
        let endpoint = self.ends.get(root.endpoint.slot());
        let pipe = endpoint.pipe.ok_or(DENIED)?;
        if pipe.revoked {
            return Err(REVOKED);
        }
        if root.id != endpoint.root
            || endpoint.owner != parent.number
            || pipe.writer != writer
            || pipe.reader.is_some()
        {
            return Err(DENIED);
        }
        Ok(root)
    }

    pub(super) fn pipe_attach(&mut self, root: Node, child: Pid, reader: bool) -> Res<u64> {
        let key = self.install_child(root, child, if reader { RECEIVE } else { SEND })?;
        let pipe = self
            .ends
            .get_mut(root.endpoint.slot())
            .pipe
            .as_mut()
            .unwrap();
        if reader {
            pipe.reader = Some(child);
        } else {
            pipe.writer = Some(child);
        }
        if pipe.trace {
            let opposite = if reader { SEND } else { RECEIVE };
            let isolated = self.lookup(child, key, opposite).is_err()
                && self.lookup(child, key, GRANT).is_err()
                && self.lookup(child, key, MANAGE).is_err();
            hal::log(format_args!(
                "PIPE role={} pid={} isole={isolated}",
                if reader { "wc" } else { "cat" },
                child.number
            ));
        }
        Ok(key)
    }

    pub(super) fn pipe_close(&mut self, n: Node) {
        let ep = self.ends.get_mut(n.endpoint.slot());
        let Some(pipe) = &mut ep.pipe else { return };
        match n.rights {
            SEND => pipe.writer_closed = true,
            RECEIVE => pipe.reader_closed = true,
            _ => (),
        }
    }
}
