//! Liaison de deux routes privées pour l'identité complète du shell démarré par le noyau.
use super::*;

fn live_identity(s: &Core, pid: Pid) -> Res<[u64; 4]> {
    let p = s.processes.slots[pid.slot]
        .as_ref()
        .filter(|p| p.pid == pid && p.space.is_some() && p.outcome.is_none())
        .ok_or(CLOSED)?;
    let bytes = p
        .instance
        .identity(crate::boot_id::current())
        .ok_or(REFUSED)?
        .encode();
    Ok(core::array::from_fn(|i| {
        u64::from_le_bytes(bytes[i * 8..i * 8 + 8].try_into().unwrap())
    }))
}

pub(super) fn connect(s: &mut Core, m: Session, signer: Pid) -> Res<()> {
    let verifier = s
        .processes
        .slots
        .iter()
        .flatten()
        .find(|p| {
            p.service_owner == Some(m.manager)
                && p.service_id == auth_lab::AUTH
                && p.outcome.is_none()
                && p.space.is_some()
        })
        .ok_or(CLOSED)?
        .pid;
    let shell = live_identity(s, m.client)?;
    // Les quatre racines n'accordent ni GRANT ni MANAGE aux participants.
    // Aucun processus ne reçoit les deux extrémités d'une même direction.
    let (sign_in, challenge_tx) = channel(s, signer, Some(verifier), RECEIVE)?;
    let (sign_out, signature_rx) = channel(s, signer, Some(verifier), SEND)?;
    let (ask, requests) = channel(s, m.client, Some(verifier), SEND)?;
    let (reply, replies) = match channel(s, m.client, Some(verifier), RECEIVE) {
        Ok(v) => v,
        Err(e) => {
            let root = s.ipc.lookup(m.client, ask, 0)?;
            s.ipc.close(root);
            return Err(e);
        }
    };
    config(s, signer, 7, &[sign_in, sign_out]);
    config(s, signer, 9, &shell);
    config(s, verifier, 11, &shell);
    config(s, verifier, 16, &[replies, challenge_tx, signature_rx]);
    config(s, verifier, 15, &[requests]);
    config(s, m.client, 17, &[reply]);
    config(s, m.client, 16, &[ask]);
    hal::log(format_args!(
        "NXLOGIN route shell={} verificateur={} signataire={}",
        m.client.number, verifier.number, signer.number
    ));
    Ok(())
}

pub(super) const RELAY: u64 = 21;

/// Les deux racines de transport sont comptées au courtier, avec des pairs
/// fixes. Aucun pair ne peut les dupliquer, en changer les droits ou la cible.
pub(super) fn connect_relay(s: &mut Core, m: Session, relay: Pid) -> Res<()> {
    let issuer = s
        .processes
        .slots
        .iter()
        .flatten()
        .find(|p| {
            p.service_owner == Some(m.manager)
                && p.service_id == auth_lab::AUTH
                && p.outcome.is_none()
                && p.space.is_some()
        })
        .ok_or(CLOSED)?
        .pid;
    let (rx, send) = channel(s, relay, Some(issuer), RECEIVE)?;
    let (tx, receive) = channel(s, relay, Some(issuer), SEND)?;
    super::super::session_relay::register(s, m.client, issuer, relay, m.fs, send, tx)?;
    config(s, relay, 5, &[rx, tx]);
    config(s, issuer, 20, &[receive]);
    config(s, issuer, 19, &[send]);
    Ok(())
}
