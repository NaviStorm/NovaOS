// Routage R2 privé, inclus uniquement dans l'export de qualification.
// Les indices restent internes ; aucun appelant ne fournit une route à employer.
fn route_for(s: &Core, owner: Pid) -> Res<usize> {
    let mut found = s.processes.relays.iter().enumerate()
        .filter_map(|(i, r)| r.as_ref().filter(|r| r.shell == owner).map(|_| i));
    let i = found.next().ok_or(REVOKED)?;
    if found.next().is_some() { return Err(DENIED); }
    Ok(i)
}

pub(super) fn intercept(s: &mut Core, p: Pid, sys: u64, mut args: [u64; 3]) -> Option<Res<u64>> {
    const LIST: u64 = 0x4e5841444c495354;
    const FENCE: u64 = 0x4e585246454e43;
    const FSID: u64 = 0x4e585246534944;
    const VALIDATE: u64 = 0x4e585656414c;
    let proc = s.processes.get(p);
    if proc.service_owner.is_some() && matches!(proc.service_id, 21 | 26)
        && matches!(sys, IPC_CREATE | IPC_DUP | IPC_GIVE | IPC_REVOKE | IPC_CANCEL) {
        return Some(Err(DENIED));
    }
    if sys == IPC_NOTIFY && args[2] == FENCE {
        // Un changement de tête invalide tous les tickets préparés. Il ne
        // ferme pas les sessions indépendantes encore autorisées.
        let endpoint = match s.ipc.lookup(p, args[0], SEND) {
            Ok(n) => n.endpoint, Err(e) => return Some(Err(e)),
        };
        if args[1] != 0 || !s.processes.relays.iter().flatten()
            .any(|r| p == r.storage && endpoint == r.attestations) {
            return Some(Err(DENIED));
        }
        for i in 0..2 {
            if let Some(mut r) = s.processes.relays[i].take() {
                exec::registry_fence(s, r.shell);
                r.admission = None;
                pipeline::abandon(s, &mut r);
                s.processes.relays[i] = Some(r);
            }
        }
        return Some(Ok(0));
    }
    if sys == IPC_NOTIFY && args[2] == LIST {
        let i = (args[1] / 2) as usize;
        if i >= 2 { return Some(Err(DENIED)); }
        let r = match s.processes.relays[i].as_ref() {
            Some(r) => r, None => return Some(Err(REVOKED)),
        };
        if p != r.issuer { return Some(Err(DENIED)); }
        args[1] %= 2;
        return intercept_route(s, i, p, sys, args);
    }
    let mut selected = None;
    let mut protected = false;
    // Une enveloppe de chargement passe sur le port partagé du stockage.
    // La session ou le nonce attesté, jamais le nom de l'ELF, désigne sa route.
    let mut loading = None;
    if sys == IPC_SEND && s.processes.relays.iter().flatten().any(|r| {
        s.ipc.lookup(p, args[0], SEND).is_ok_and(|n|
            (p == r.issuer && n.endpoint == r.authorizations)
            || (p == r.storage && n.endpoint == r.attestations))
    }) {
        let f = match input::<160>(proc.space.as_ref()?, args[1], args[2], 160, false) {
            Ok(f) => f, Err(e) => return Some(Err(e)),
        };
        if &f[32..35] == b"NXL" {
            protected = true;
            if dword(&f, 24) != 128 || f[35] != b'D' { return Some(Err(BAD_ARGUMENT)); }
            loading = Some(f);
        }
    }
    for (i, r) in s.processes.relays.iter().enumerate() {
        let Some(r) = r.as_ref() else { continue; };
        let belongs = if let Some(f) = loading.as_ref() {
            let q = &f[32..];
            match &q[..8] {
                b"NXLDOPEN" => r.live.is_some_and(|l| q[24..56] == l.id),
                b"NXLDTAKE" => r.live.is_some_and(|l| q[8..40] == l.id),
                _ => r.admission.as_ref().is_some_and(|a| a.matches_request(&q[8..24])),
            }
        } else if p == r.storage && sys == IPC_NOTIFY && args[2] == FSID {
            protected = true;
            r.live.is_some_and(|l| l.sequence == args[1])
        } else if p == r.storage && sys == IPC_NOTIFY && args[1] == VALIDATE {
            protected = true;
            r.bridge.matches_sequence(args[2])
        } else {
            p == r.shell || p == r.relay
                || (p == r.issuer && args[0] == r.auth_port)
                || r.captures.iter().flatten().any(|c| c.belongs(p))
                || r.administrative.iter().flatten().any(|c| c.belongs(p, r.issuer, args[0]))
        };
        if belongs {
            if selected.replace(i).is_some() { return Some(Err(DENIED)); }
        }
    }
    match selected {
        Some(i) => intercept_route(s, i, p, sys, args),
        None if protected => Some(Err(DENIED)),
        None => None,
    }
}
