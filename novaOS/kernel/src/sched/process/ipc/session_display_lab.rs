//! Canaux de restitution du profil de lancement. Seul nsh possède la console.
//! Un envoi acquitté a déjà été lu par le shell, même si l'enfant sort aussitôt.
use super::*;

#[derive(Clone, Copy)]
pub(super) struct Capture {
    child: Pid,
    input: Option<Reference>,
    writer: u64,
    files: [Option<Reference>; 2],
    roots: [Option<Reference>; 2],
    reads: [u64; 2],
}

pub(in crate::sched::process::ipc) fn attach(
    s: &mut Core,
    shell: Pid,
    child: Pid,
    binding: exec::Binding,
    scope: [u64; 3],
    stdout: bool,
    stdin: bool,
) -> Res<()> {
    let mask = crate::terminal::OUTPUT | crate::terminal::ERROR;
    if !owns_session(s, shell, binding.session) || live(s, shell)?.terminal & mask != mask {
        return Err(DENIED);
    }
    let r = s.processes.relay.as_ref().ok_or(REVOKED)?;
    let available = r
        .captures
        .iter()
        .position(|entry| {
            entry.is_none_or(|c| {
                let p = s.processes.get(shell);
                ![p.wait_right, p.pipe_reader_wait]
                    .into_iter()
                    .flatten()
                    .any(|w| w.target == c.child)
            })
        })
        .ok_or(LIMIT)?;
    let output = if stdout {
        service::channel(s, child, Some(shell), SEND)?
    } else {
        (0, 0)
    };
    let error = service::channel(s, child, Some(shell), SEND)?;
    let roots = [
        if stdout {
            Some(s.ipc.lookup(child, output.0, SEND)?.id)
        } else {
            None
        },
        Some(s.ipc.lookup(child, error.0, SEND)?.id),
    ];
    service::config(s, child, 2, &[output.0, error.0]);
    let mut files = [None; 2];
    let storage = r_storage(s)?;
    service::config(s, storage, 96 + available * 16, &[0]);
    if scope[0] != 0 && scope[1] != 0 {
        let tx = service::channel(s, child, Some(storage), SEND)?;
        let rx = service::channel(s, child, Some(storage), RECEIVE)?;
        files = [
            Some(s.ipc.lookup(child, tx.0, SEND)?.id),
            Some(s.ipc.lookup(child, rx.0, RECEIVE)?.id),
        ];
        service::config(s, child, 4, &[tx.0, rx.0]);
        let mut data = [0; 13];
        data[..5].copy_from_slice(&[tx.1, rx.1, scope[0], scope[1], scope[2]]);
        let mut ids = [0u8; 64];
        ids[..16].copy_from_slice(&binding.holder);
        ids[16..32].copy_from_slice(&binding.installation);
        ids[32..].copy_from_slice(&binding.digest);
        for (dst, word) in data[5..].iter_mut().zip(ids.chunks_exact(8)) {
            *dst = u64::from_le_bytes(word.try_into().unwrap());
        }
        service::config(s, storage, 97 + available * 16, &data);
        service::config(s, storage, 96 + available * 16, &[child.number]);
    }
    let (input, writer) = if stdin {
        // Le shell a déjà son contrôleur inter-étages pour le producteur.
        let owner = if stdout { shell } else { child };
        let key = s.ipc.pipe_open(owner, false)?;
        let root = s.ipc.pipe_check(owner, key, None)?;
        let ends = (|| {
            let read = s.ipc.attach_stream(root, child, true, "stdin")?;
            let write = s.ipc.attach_stream(root, shell, false, "stdin-nsh")?;
            service::config(s, child, 1, &[read]);
            // Seules les feuilles directionnelles sont exposées aux processus.
            s.ipc.nodes.get_mut(root.id.slot()).rights = 0;
            Ok(write)
        })();
        match ends {
            Ok(send) => (Some(root.id), send),
            Err(e) => {
                s.ipc.revoke(root);
                s.ipc.close(root);
                return Err(e);
            }
        }
    } else {
        (None, 0)
    };
    s.processes.relay.as_mut().unwrap().captures[available] = Some(Capture {
        child,
        input,
        writer,
        files,
        roots,
        reads: [output.1, error.1],
    });
    Ok(())
}

pub(super) fn retire_input(s: &mut Core, route: &mut Route, child: Pid) {
    if let Some(c) = route
        .captures
        .iter_mut()
        .flatten()
        .find(|c| c.child == child)
        && let Some(n) = c.input.take().and_then(|root| s.ipc.node(root))
    {
        c.writer = 0;
        s.ipc.close(n);
    }
}

pub(super) fn revoke(s: &mut Core, r: &mut Route) {
    for c in r.captures.iter_mut().filter_map(Option::take) {
        for root in c
            .roots
            .into_iter()
            .chain(c.files)
            .chain([c.input])
            .flatten()
        {
            if let Some(n) = s.ipc.node(root) {
                s.ipc.revoke(n);
                s.ipc.close(n);
            }
        }
    }
}

pub(super) fn intercept(s: &Core, r: &Route, p: Pid, sys: u64, args: [u64; 3]) -> Option<Res<u64>> {
    const GET: u64 = 0x4e58_4f55_5400;
    if p == r.shell && sys == IPC_NOTIFY && (GET..=GET + 2).contains(&args[2]) {
        return Some((|| {
            let n = s.ipc.lookup(p, args[0], SEND)?;
            let end = s.ipc.ends.get(n.endpoint.slot());
            if end.owner != p.number
                || end.root != n.id
                || n.rights != SEND
                || !s.ipc.nodes.ids().any(|i| {
                    let v = s.ipc.nodes.get(i);
                    v.parent == n.id && v.holder as usize == r.issuer.slot && v.rights == RECEIVE
                })
                || r.live.is_none_or(|l| l.limit <= time_ms())
            {
                return Err(DENIED);
            }
            let c = r
                .captures
                .iter()
                .flatten()
                .find(|c| c.child.number == args[1])
                .ok_or(DENIED)?;
            let shell = s.processes.get(p);
            if ![shell.wait_right, shell.pipe_reader_wait]
                .into_iter()
                .flatten()
                .any(|w| w.target == c.child)
            {
                return Err(DENIED);
            }
            if args[2] == GET + 2 {
                Ok(c.writer)
            } else {
                Ok(c.reads[(args[2] - GET) as usize])
            }
        })());
    }
    let c = r.captures.iter().flatten().find(|c| c.child == p)?;
    let n = s.ipc.lookup(p, args[0], SEND).ok()?;
    if !c.roots.contains(&Some(n.id)) {
        return None;
    }
    match sys {
        IPC_SEND => {
            let frame = match input::<160>(
                live(s, p).ok()?.space.as_ref()?,
                args[1],
                args[2],
                160,
                false,
            ) {
                Ok(frame) => frame,
                Err(code) => return Some(Err(code)),
            };
            if word(&frame, 16) != 0 || !(1..=128).contains(&dword(&frame, 24)) {
                return Some(Err(BAD_ARGUMENT));
            }
            if dword(&frame, 28) == 0 {
                Some(Err(WOULD_BLOCK))
            } else {
                None
            }
        }
        IPC_CLOSE => None,
        _ => Some(Err(DENIED)),
    }
}

fn r_storage(s: &Core) -> Res<Pid> {
    s.processes.relay.as_ref().map(|r| r.storage).ok_or(REVOKED)
}
