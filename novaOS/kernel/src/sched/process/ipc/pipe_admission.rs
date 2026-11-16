//! Admission de deux processus par le lanceur. Seul le noyau garde
//! l'autorité temporaire attachée au producteur jusqu'à publication du lecteur.
use super::*;

/// Jamais exporté dans une table de handles. La génération du nœud et le PID
/// complet survivent à la fermeture d'un handle ou au recyclage de son slot.
pub(in crate::sched::process) struct PendingReader {
    controller: Reference,
    writer: Pid,
}

/// Référence privée détenue par la préparation du second étage. Son code
/// d'erreur doit provenir de l'admission locale, jamais d'un message du shell.
pub(super) struct ReaderAdmission {
    parent: Pid,
    controller: Reference,
}

/// Le statut WAIT du producteur reste collectable par le parent dans les deux cas.
#[derive(Debug, PartialEq)]
pub(super) struct ReaderFailure {
    pub(super) status: u64,
    pub(super) forced: bool,
}

impl PendingReader {
    pub(in crate::sched::process) fn abort(self, s: &mut Core) -> bool {
        if let Some(root) = s.ipc.node(self.controller) {
            s.ipc.revoke(root);
            s.ipc.close(root);
            s.flush_ipc();
            s.ipc.collect();
        }
        // Le producteur peut déjà être sorti et même avoir été collecté.
        let live = s.processes.slots[self.writer.slot]
            .as_ref()
            .is_some_and(|p| p.pid == self.writer && p.outcome.is_none());
        if live {
            stop_process(s, self.writer, Outcome::Exit(125), hal::timer::counter());
        }
        live
    }
}

// Le lanceur appelle le constructeur sous la garde. L'image produite est
// nouvelle et reste non publiée jusqu'à l'installation de son extrémité.
pub(super) fn start_writer(
    parent: Pid,
    control: u64,
    create: impl FnOnce() -> Res<Child>,
) -> Res<(u64, ReaderAdmission)> {
    let _guard = Guard::new();
    let check = |s: &Core| {
        let p = s.processes.slots[parent.slot]
            .as_ref()
            .filter(|p| p.pid == parent)
            .ok_or(REVOKED)?;
        if p.outcome.is_some()
            || p.wait_right.is_some()
            || p.pipe_reader_wait.is_some()
            || p.pending_reader.is_some()
        {
            return Err(REFUSED);
        }
        s.ipc.pipe_check(parent, control, None)
    };
    let controller = with(|s| check(s))?.id;
    let child = create()?;
    let result = with(|s| {
        let root = check(s)?;
        if root.id != controller {
            return Err(REVOKED);
        }
        let producer = s.processes.get(child.0);
        if !producer.principal
            || producer.worker_image.is_some()
            || producer.watch != supervision::Watch::None
            || producer.terminal & crate::terminal::LAUNCH != 0
            || producer.service_owner.is_some()
        {
            return Err(DENIED);
        }
        let key = s.ipc.attach_stream(root, child.0, false, "stdout")?;
        set_stream(s, child.0, 1, key);
        let wait = WaitRight { target: child.0 };
        let p = s.processes.get_mut(parent);
        p.wait_right = Some(wait);
        p.pending_reader = Some(PendingReader {
            controller,
            writer: child.0,
        });
        Ok((wait.key(), ReaderAdmission { parent, controller }))
    });
    if result.is_ok() {
        core::mem::forget(child);
    } else {
        retire_unpublished(child);
    }
    result
}

fn set_stream(s: &Core, pid: Pid, index: usize, key: u64) {
    heap::with_pages(|m| {
        s.processes
            .get(pid)
            .space
            .as_ref()
            .unwrap()
            .write_data(m, index, key);
        Ok(())
    })
    .unwrap();
}

impl ReaderAdmission {
    fn matching<'a>(&self, s: &'a Core) -> Res<&'a PendingReader> {
        s.processes.slots[self.parent.slot]
            .as_ref()
            .filter(|p| p.pid == self.parent && p.outcome.is_none())
            .and_then(|p| p.pending_reader.as_ref())
            .filter(|p| p.controller == self.controller)
            .ok_or(REVOKED)
    }

    fn root(&self, s: &Core) -> Res<Node> {
        let pending = self.matching(s)?;
        let root = s.ipc.node(self.controller).ok_or(REVOKED)?;
        s.ipc
            .pipe_check(self.parent, root.key, Some(pending.writer))
    }

    /// Le résultat est celui de la décision locale de publication. Une perte
    /// de réponse IPC en aval ne repasse pas par ce chemin. Le constructeur
    /// retourne uniquement une erreur du chargeur local. Un statut de transport
    /// (TIMED_OUT, UNCERTAIN…) ne peut donc pas déclencher ce retrait.
    pub(super) fn admit(
        self,
        create: impl FnOnce() -> Result<Child, Error>,
    ) -> Result<u64, ReaderFailure> {
        let _guard = Guard::new();
        let admission = (|| {
            with(|s| self.root(s))?;
            let child = create().map_err(|error| match error {
                Error::Memory(crate::memory::Error::Exhausted) => NO_MEMORY,
                Error::Full => LIMIT,
                _ => REFUSED,
            })?;
            let result = with(|s| {
                let root = self.root(s)?;
                let key = s.ipc.attach_stream(root, child.0, true, "stdin")?;
                set_stream(s, child.0, 2, key);
                let wait = WaitRight { target: child.0 };
                let p = s.processes.get_mut(self.parent);
                p.pipe_reader_wait = Some(wait);
                p.pending_reader = None;
                Ok(wait.key())
            });
            if result.is_ok() {
                core::mem::forget(child);
            } else {
                retire_unpublished(child);
            }
            result
        })();
        admission.map_err(|status| {
            let forced = with(|s| {
                if self.matching(s).is_err() {
                    return false;
                }
                let pending = s
                    .processes
                    .get_mut(self.parent)
                    .pending_reader
                    .take()
                    .unwrap();
                pending.abort(s)
            });
            ReaderFailure { status, forced }
        })
    }
}
