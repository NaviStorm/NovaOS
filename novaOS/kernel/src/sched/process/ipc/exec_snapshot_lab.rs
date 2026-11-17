//! Admission conjointe. NovaAuth autorise le titulaire et la session, le service
//! P9 atteste la vue. Le courtier et le shell ne peuvent alimenter ces deux voies.
use super::*;

pub(super) struct Admission {
    request: [u8; 16],
    holder: [u8; 16],
    head: [u8; 32],
    session: [u8; 32],
    argv: Vec<u8>,
    argv_len: usize,
    binding: Option<exec::Binding>,
    staging: Option<user::ImageStaging>,
    sealed: Option<user::SealedImage>,
}
fn geometry(_: crate::memory::Error) -> u64 {
    REFUSED
}

pub(super) fn operation(s: &mut Core, r: &mut Route, p: Pid, q: &[u8]) -> Res<u64> {
    let lease = r.live.ok_or(REVOKED)?;
    if r.down || lease.limit <= time_ms() {
        return Err(REVOKED);
    }
    let request: [u8; 16] = q[8..24].try_into().unwrap();
    if request == [0; 16] {
        return Err(BAD_ARGUMENT);
    }
    if &q[..8] == b"NXLDOPEN" && p == r.issuer {
        if q[24..56] != lease.id
            || q[56..72] == [0; 16]
            || q[72..104] == [0; 32]
            || q[112..] != [0; 16]
        {
            return Err(DENIED);
        }
        if r.admission.is_some() {
            return Err(WOULD_BLOCK);
        }
        let len = usize::try_from(word(q, 104)).map_err(|_| LIMIT)?;
        if !(1..=crate::terminal::ARG_MAX).contains(&len) {
            return Err(LIMIT);
        }
        let mut argv = Vec::new();
        argv.try_reserve_exact(len).map_err(|_| NO_MEMORY)?;
        r.admission = Some(Admission {
            request,
            holder: q[56..72].try_into().unwrap(),
            head: q[72..104].try_into().unwrap(),
            session: lease.id,
            argv,
            argv_len: len,
            binding: None,
            staging: None,
            sealed: None,
        });
        return Ok(128);
    }
    let a = r
        .admission
        .as_mut()
        .filter(|a| a.request == request && a.session == lease.id)
        .ok_or(DENIED)?;
    match &q[..8] {
        b"NXLDABRT" if p == r.issuer || p == r.storage => {
            r.admission = None;
        }
        b"NXLDARGV" if p == r.issuer => {
            let offset = word(q, 24) as usize;
            let n = word(q, 32) as usize;
            if n == 0
                || n > 88
                || offset != a.argv.len()
                || offset.checked_add(n).is_none_or(|end| end > a.argv_len)
                || q[40 + n..].iter().any(|v| *v != 0)
            {
                return Err(BAD_ARGUMENT);
            }
            a.argv.extend_from_slice(&q[40..40 + n]);
        }
        b"NXLDMETA" if p == r.storage => {
            let app = word(q, 104);
            let object = word(q, 112);
            let revision = word(q, 120);
            if a.binding.is_some()
                || q[24..40] != a.holder
                || q[40..72] != a.head
                || q[72..88] == [0; 16]
                || q[88..104] == [0; 16]
                || app >> 63 != 1
                || object == 0
                || revision == 0
            {
                return Err(DENIED);
            }
            a.binding = Some(exec::Binding {
                session: a.session,
                domain: [7; 16],
                holder: a.holder,
                installation: q[88..104].try_into().unwrap(),
                app,
                epoch: 0,
                object: user::ObjectRevision {
                    volume: q[72..88].try_into().unwrap(),
                    object,
                    revision,
                    length: 0,
                },
                digest: [0; 32],
            });
        }
        b"NXLDSIZE" if p == r.storage => {
            let b = a.binding.as_mut().ok_or(DENIED)?;
            if b.epoch != 0 || word(q, 64) == 0 || q[72..].iter().any(|v| *v != 0) {
                return Err(BAD_ARGUMENT);
            }
            b.epoch = word(q, 64);
            b.object.length = usize::try_from(word(q, 24)).map_err(|_| LIMIT)?;
            b.digest = q[32..64].try_into().unwrap();
            a.staging = Some(user::ImageStaging::new(b.object, b.digest).map_err(geometry)?);
        }
        b"NXLDDATA" if p == r.storage => {
            let offset = word(q, 24) as usize;
            let address = word(q, 32);
            let n = word(q, 40) as usize;
            if n == 0 || n > 4096 || q[48..].iter().any(|v| *v != 0) {
                return Err(BAD_ARGUMENT);
            }
            let b = a.binding.ok_or(DENIED)?;
            let mut fragment = Vec::new();
            fragment.try_reserve_exact(n).map_err(|_| NO_MEMORY)?;
            fragment.resize(n, 0);
            let space = live(s, p)?.space.as_ref().ok_or(REVOKED)?;
            if !heap::with_pages(|m| Ok(space.copy_from_user(m, address, &mut fragment))).unwrap() {
                return Err(BAD_ADDRESS);
            }
            let staging = a.staging.take().ok_or(DENIED)?;
            a.staging = Some(
                staging
                    .append(b.object, offset, &fragment)
                    .map_err(geometry)?,
            );
        }
        b"NXLDSEAL" if p == r.storage => {
            a.sealed = Some(a.staging.take().ok_or(DENIED)?.seal().map_err(geometry)?);
        }
        b"NXLDISUE" if p == r.issuer => {
            if a.head != q[24..56] || q[56..88] != lease.id || a.argv.len() != a.argv_len {
                return Err(REVOKED);
            }
            let mut a = r.admission.take().unwrap();
            let binding = a.binding.ok_or(DENIED)?;
            let sealed = a.sealed.take().ok_or(DENIED)?;
            let mut config = Vec::new();
            config
                .try_reserve_exact(64 + a.argv_len)
                .map_err(|_| NO_MEMORY)?;
            config.resize(64, 0);
            config[..8].copy_from_slice(b"NXEXEC01");
            config[48..56].copy_from_slice(&(a.argv_len as u64).to_le_bytes());
            config.extend_from_slice(&a.argv);
            let ttl = word(q, 88);
            if ttl == 0 || ttl > 5000 || q[96..].iter().any(|b| *b != 0) {
                return Err(BAD_ARGUMENT);
            }
            let until = time_ms().saturating_add(ttl).min(lease.limit);
            let deadline = until.saturating_mul(u64::from(HZ)) / 1000;
            return exec::issue_authenticated(
                s, r.issuer, r.shell, binding, sealed, config, deadline,
            );
        }
        _ => return Err(DENIED),
    }
    Ok(128)
}
