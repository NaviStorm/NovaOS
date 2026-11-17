// Inclus dans le profil de lancement QEMU. Les deux valeurs publiques de WAIT
// restent celles du mécanisme natif. Les extrémités utilisent la même file.
pub(super) fn matches_ticket(s: &Core, parent: Pid, root: Reference) -> bool {
    s.processes
        .get(parent)
        .pending_reader
        .as_ref()
        .is_some_and(|v| v.controller == root)
}

pub(super) fn attach_ticket(
    s: &mut Core,
    parent: Pid,
    child: Pid,
    key: u64,
    expected: Reference,
    reader: bool,
) -> Res<()> {
    let p = s.processes.get(parent);
    let writer = if reader {
        let pair = p
            .pending_reader
            .as_ref()
            .filter(|v| v.controller == expected)
            .ok_or(DENIED)?;
        if p.pipe_reader_wait.is_some() {
            return Err(LIMIT);
        }
        Some(pair.writer)
    } else {
        if p.wait_right.is_some()
            || p.pipe_reader_wait.is_some()
            || p.pending_reader.is_some()
            || p.reader_receipt.is_some()
        {
            return Err(LIMIT);
        }
        None
    };
    let root = s.ipc.pipe_check(parent, key, writer)?;
    if root.id != expected {
        return Err(REVOKED);
    }
    let port = s
        .ipc
        .attach_stream(root, child, reader, if reader { "stdin" } else { "stdout" })?;
    set_stream(s, child, if reader { 1 } else { 2 }, port);
    let p = s.processes.get_mut(parent);
    if reader {
        p.pending_reader = None;
    } else {
        p.pending_reader = Some(PendingReader {
            controller: root.id,
            writer: child,
            request: key,
        });
    }
    Ok(())
}

pub(super) fn fail_ticket(s: &mut Core, parent: Pid, root: Reference) {
    if matches_ticket(s, parent, root) {
        let pending = s.processes.get_mut(parent).pending_reader.take().unwrap();
        let producer = pending.writer;
        let forced = pending.abort(s);
        hal::log(format_args!(
            "NXPIPE admission lecteur refusee producteur={} retrait_force={forced}",
            producer.number
        ));
    }
}
