#![no_std]
#![no_main]
mod common;
#[allow(dead_code)]
#[path = "../../../../../abi/files.rs"]
mod files;
#[allow(dead_code)]
#[path = "../../../../../abi/instances.rs"]
mod instances;
#[allow(dead_code)]
#[path = "../../../../../abi/service_process.rs"]
mod process;
#[allow(dead_code)]
#[path = "../../../../storage/runtime.rs"]
mod runtime;
use common::*;
use ed25519_dalek::VerifyingKey;
use novaos_auth_session::{Binding, Possession, Route, Verifier};
use runtime as rt;
fn now() -> rt::Result<u64> {
    rt::value(rt::call(instances::SERVICE_PROCESS, process::NOW, 0, 0))
}
fn identity(offset: usize) -> [u8; 32] {
    let mut id = [0; 32];
    for (i, word) in id.chunks_exact_mut(8).enumerate() {
        word.copy_from_slice(&rt::config(offset + i).to_le_bytes());
    }
    id
}
fn binding(tag: u64, reference: [u8; 16]) -> rt::Result<Binding> {
    let mut q = [0; 128];
    q[..8].copy_from_slice(b"NA3CHECK");
    q[8..24].copy_from_slice(&reference);
    let a = rpc(rt::config(5), rt::config(6), tag, &q)?;
    if number(&a) != rt::OK {
        return Err(rt::DENIED);
    }
    Ok(Binding {
        domain: DOMAIN,
        reference,
        subject: a[8..24].try_into().unwrap(),
        head: a[24..56].try_into().unwrap(),
    })
}
fn serve() -> rt::Result<()> {
    let mut verifier = Verifier::new();
    let mut sessions = novaos_auth_session::sessions::Sessions::new();
    let mut last = [0; 2];
    let mut budget = novaos_auth_session::admission::Admission::new(now()?);
    // Même après redémarrage, attendre la pénalité maximale avant le premier essai.
    match rt::receive_for(rt::config(1), 161) {
        Err(rt::TIMED_OUT) => (),
        _ => return Err(rt::CANCELLED),
    }
    let mut next = 0usize;
    let mut dead = [0u64; 2];
    let mut rpc_seq = 0u64;
    marker(b"NA3 verifier pret");
    loop {
        let lane = next;
        next = 1 - next;
        let offset = 11 + lane * 4;
        let rx = rt::config(offset);
        let tx = rt::config(offset + 1);
        let control_wait = if rx == 0 || dead[lane] == rx { 1 } else { 0 };
        match rt::receive_for(rt::config(1), control_wait) {
            Ok(m) if m.kind == 1 && m.tag == 1 && m.len == 1 && m.payload[0] == 0 => return Ok(()),
            Err(rt::TIMED_OUT | rt::WOULD_BLOCK) => (),
            _ => return Err(rt::CANCELLED),
        }
        if rx == 0 || tx == 0 || dead[lane] == rx {
            continue;
        }
        let issuer = identity(7);
        let mut client = issuer;
        client[16..24].copy_from_slice(&rt::config(offset + 2).to_le_bytes());
        client[24..32].copy_from_slice(&rt::config(offset + 3).to_le_bytes());
        let route = Route { client, issuer };
        let m = match rt::receive_for(rx, 2) {
            Err(rt::CLOSED | rt::REVOKED | rt::DENIED) => {
                sessions.disconnect(route);
                verifier.disconnect(route);
                dead[lane] = rx;
                last[lane] = 0;
                continue;
            }
            Err(rt::TIMED_OUT | rt::WOULD_BLOCK) => continue,
            r => r?,
        };
        let proof_start = if m.payload[0] == 2 {
            Some(now()?)
        } else {
            None
        };
        let mut a = [0; 128];
        let mut charged = None;
        let outcome = (|| -> rt::Result<()> {
            if m.kind != 1 || m.len != 128 || m.tag <= last[lane] {
                return Err(rt::BAD_ARGUMENT);
            }
            last[lane] = m.tag;
            let timestamp = now()?;
            let key_bytes = m.payload[8..40].try_into().unwrap();
            let reference = Possession::reference_bytes(DOMAIN, key_bytes);
            if matches!(m.payload[0], 1 | 2) {
                if !budget.allowed(lane, reference, timestamp) {
                    return Err(rt::DENIED);
                }
                charged = Some(reference);
            }
            let key = VerifyingKey::from_bytes(m.payload[8..40].try_into().unwrap())
                .map_err(|_| rt::DENIED)?;
            let factor = Possession(key);
            if rt::config(offset) != rx {
                return Err(rt::REVOKED);
            }
            let op = m.payload[0];
            rpc_seq = rpc_seq.checked_add(1).ok_or(rt::LIMIT)?;
            let current = if op == 4 {
                None
            } else {
                binding(rpc_seq, reference).ok()
            };
            match m.payload[0] {
                1 => {
                    let (token, challenge) = verifier
                        .begin(
                            current.ok_or(rt::DENIED)?,
                            route,
                            m.payload[40..56].try_into().unwrap(),
                            *random()?,
                            now()?,
                            TTL_MS,
                        )
                        .map_err(|_| rt::REFUSED)?;
                    a[8..16].copy_from_slice(&token.to_le_bytes());
                    a[16..48].copy_from_slice(&challenge);
                }
                2 => {
                    let verified = verifier
                        .finish(
                            number(&m.payload[40..]),
                            route,
                            now()?,
                            current,
                            &factor,
                            m.payload[48..112].try_into().unwrap(),
                        )
                        .map_err(|_| rt::DENIED)?;
                    a[8..24].copy_from_slice(&verified.subject());
                    rpc_seq = rpc_seq.checked_add(1).ok_or(rt::LIMIT)?;
                    let live = binding(rpc_seq, reference)?;
                    if rt::config(offset) != rx {
                        return Err(rt::REVOKED);
                    }
                    let id = sessions
                        .open(verified, live, now()?, 60_000)
                        .map_err(|_| rt::REFUSED)?;
                    a[24..56].copy_from_slice(&id);
                }
                3 => {
                    let subject = sessions
                        .inspect(
                            m.payload[40..72].try_into().unwrap(),
                            route,
                            current,
                            now()?,
                        )
                        .map_err(|_| rt::DENIED)?;
                    a[8..24].copy_from_slice(&subject);
                }
                4 => sessions
                    .close(m.payload[40..72].try_into().unwrap(), route)
                    .map_err(|_| rt::DENIED)?,
                11 => {
                    let current = current.ok_or(rt::DENIED)?;
                    let sid = m.payload[40..72].try_into().unwrap();
                    let subject = sessions
                        .inspect(sid, route, Some(current), now()?)
                        .map_err(|_| rt::DENIED)?;
                    let mut q = [0; 128];
                    q[..8].copy_from_slice(b"NA3DATT1");
                    q[8] = lane as u8;
                    q[16..24].copy_from_slice(&m.tag.to_le_bytes());
                    q[24..40].copy_from_slice(&subject);
                    q[40..72].copy_from_slice(&sid);
                    q[72..104].copy_from_slice(&current.head);
                    rpc_seq = rpc_seq.checked_add(1).ok_or(rt::LIMIT)?;
                    rt::send_for(rt::config(5), rpc_seq, &q, 5000)?;
                    let reply = remote(rt::config(6), rpc_seq).map_err(|_| files::UNCERTAIN)?;
                    if number(&reply.payload) != 0 {
                        return Err(number(&reply.payload));
                    }
                }
                5..=10 => {
                    sessions
                        .inspect(
                            m.payload[40..72].try_into().unwrap(),
                            route,
                            current,
                            now()?,
                        )
                        .map_err(|_| rt::DENIED)?;
                    let mut q = [0; 128];
                    q[..8].copy_from_slice(match op {
                        5 => b"NA3CREAT",
                        6 => b"NA3DISAB",
                        7 => b"NA3DELET",
                        8 => b"NA3RENAM",
                        9 => b"NA3UNGRP",
                        _ => b"NA3ISSUE",
                    });
                    q[8..24].copy_from_slice(&reference);
                    q[24..40].copy_from_slice(&m.payload[72..88]);
                    q[88..112].copy_from_slice(&m.payload[104..128]);
                    q[40..56].copy_from_slice(&m.payload[88..104]);
                    q[56..88].copy_from_slice(&current.ok_or(rt::DENIED)?.head);
                    rpc_seq = rpc_seq.checked_add(1).ok_or(rt::LIMIT)?;
                    rt::send_for(rt::config(5), rpc_seq, &q, 5000)?;
                    let reply = remote(rt::config(6), rpc_seq)
                        .map_err(|e| if op == 10 { e } else { files::UNCERTAIN })?
                        .payload;
                    if number(&reply) != 0 {
                        return Err(number(&reply));
                    }
                    a[8..24].copy_from_slice(&reply[8..24]);
                }
                _ => return Err(rt::BAD_ARGUMENT),
            }
            Ok(())
        })();
        let status = outcome.err().unwrap_or(rt::OK);
        if status == rt::OK && m.payload[0] == 2 {
            if let Some(reference) = charged {
                budget.success(lane, reference);
            }
            marker(b"NA3 preuve acceptee");
            if let Some(start) = proof_start {
                // Depuis la remise du message, incluant les deux lectures du registre.
                let mut elapsed = now()?.saturating_sub(start);
                let mut line = *b"NA3 verification ms=00000000000000000000";
                for digit in line[20..].iter_mut().rev() {
                    *digit = b'0' + (elapsed % 10) as u8;
                    elapsed /= 10;
                }
                marker(&line);
            }
        }
        if let Some(reference) = charged.filter(|_| status != rt::OK) {
            budget.fail(lane, reference, now()?);
            marker(b"NA3 demande refusee");
        }
        a[..8].copy_from_slice(&status.to_le_bytes());
        match rt::send_for(tx, m.tag, &a, 5000) {
            Ok(()) => (),
            Err(rt::TIMED_OUT | rt::REVOKED | rt::CLOSED | rt::DENIED) => {
                sessions.disconnect(route);
                verifier.disconnect(route);
                if rt::config(offset) == rx {
                    dead[lane] = rx;
                }
            }
            Err(e) => return Err(e),
        }
    }
}
#[unsafe(no_mangle)]
extern "C" fn run() -> ! {
    rt::exit(serve().err().unwrap_or(0))
}
