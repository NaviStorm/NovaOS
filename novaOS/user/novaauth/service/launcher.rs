//! Le canal d'enrôlement de l'installation neuve appartient au lanceur seul.
use super::*;
#[allow(dead_code)]
#[path = "../../../abi/files.rs"]
mod files;
pub(super) fn launch() -> rt::Result<()> {
    fn start(id: u64) -> rt::Result<Frame> {
        let mut w = [0; 16];
        w[0] = id;
        w[4] = 1;
        w[0] = syscall(proc::START, &mut w)?;
        Ok(w)
    }
    let verifier = start(17)?;
    let provisioner = start(20)?;
    let client = start(18)?;
    let second = start(19)?;
    // Seule la référence publique est remise par l'ELF de confiance.
    let msg = rt::receive_for(client[1], 5000)?;
    if msg.kind != 1 || msg.tag != 1 || msg.len != 16 {
        return Err(rt::BAD_ARGUMENT);
    }
    let mut frame = [0; 128];
    frame[..8].copy_from_slice(b"NA3ENROL");
    frame[8..24].copy_from_slice(&msg.payload[..16]);
    rt::send_for(rt::config(5), 1, &frame, 5000)?;
    let result = rt::receive_for(rt::config(6), 5000)?;
    if result.kind != 1 || result.tag != 1 || result.len != 128 {
        return Err(rt::REFUSED);
    }
    let status = u64::from_le_bytes(result.payload[16..24].try_into().unwrap());
    if status != rt::OK {
        let text: &[u8] = match status {
            rt::DENIED => b"NA3 domaine initialise enrolement refuse",
            files::CORRUPT => b"NA3 registre divergent activation refusee",
            files::IO => b"NA3 autorite indisponible activation refusee",
            _ => b"NA3 admission indeterminee",
        };
        rt::value(rt::call(
            rt::DEBUG_WRITE,
            text.as_ptr() as u64,
            text.len() as u64,
            0,
        ))?;
        for child in [client, second, verifier, provisioner] {
            let mut cmd = [0; 16];
            cmd[0] = child[0];
            syscall(proc::KILL, &mut cmd)?;
        }
        for child in [client, second, verifier, provisioner] {
            collect(child[0], true)?;
        }
        let collected = b"NA3 refus collecte";
        rt::value(rt::call(
            rt::DEBUG_WRITE,
            collected.as_ptr() as u64,
            collected.len() as u64,
            0,
        ))?;
        return Ok(());
    }
    let other = rt::receive_for(second[1], 5000)?;
    if other.kind != 1 || other.tag != 1 || other.len != 16 {
        return Err(rt::BAD_ARGUMENT);
    }
    rt::send_for(client[2], 1, &other.payload[..16], 5000)?;
    // Trois étapes bornées sur le canal privé du client. Le temps des contrôles
    // cryptographiques ne consomme pas le délai de publication sur P9.
    for tag in [10, 11] {
        let step = rt::receive_for(client[1], 5000)?;
        if step.kind != 1 || step.tag != tag || step.len != 0 {
            return Err(rt::REFUSED);
        }
    }
    let published = rt::receive_for(client[1], 5000)?;
    if published.kind != 1 || published.tag != 2 || published.len != 16 {
        return Err(rt::REFUSED);
    }
    rt::send_for(second[2], 1, &published.payload[..16], 5000)?;
    let live = rt::receive_for(second[1], 5000)?;
    if live.kind != 1 || live.tag != 2 || live.len != 32 {
        return Err(rt::REFUSED);
    }
    let mut provision = [0; 128];
    provision[..8].copy_from_slice(b"NA3DIR01");
    provision[8..24].copy_from_slice(&published.payload[..16]);
    provision[24..56].copy_from_slice(&live.payload[..32]);
    rt::send_for(provisioner[2], 1, &provision, 5000)?;
    let approved = rt::receive_for(provisioner[1], 5000)?;
    if approved.kind != 1
        || approved.len != 128
        || approved.tag != 1
        || approved.payload != provision
    {
        return Err(rt::DENIED);
    }
    rt::send_for(rt::config(5), 2, &approved.payload, 5000)?;
    let receipt = rt::receive_for(rt::config(6), 5000)?;
    if receipt.kind != 1
        || receipt.len != 128
        || receipt.tag != 2
        || u64::from_le_bytes(receipt.payload[16..24].try_into().unwrap()) != rt::OK
    {
        return Err(rt::REFUSED);
    }
    // Son attribution confirmée, le provisionneur peut être collecté.
    rt::send_for(provisioner[2], 2, &[0], 5000)?;
    collect(provisioner[0], false)?;
    rt::send_for(client[2], 2, &live.payload[..32], 5000)?;
    let root = &receipt.payload[24..32];
    rt::send_for(client[2], 8, root, 5000)?;
    let refused = rt::receive_for(client[1], 15000)?;
    if refused.kind != 1 || refused.tag != 8 || refused.len != 1 || refused.payload[0] != 1 {
        return Err(rt::REFUSED);
    }
    rt::send_for(second[2], 7, root, 5000)?;
    let exercised = rt::receive_for(second[1], 15000)?;
    if exercised.tag != 7 || exercised.kind != 1 || exercised.len != 1 || exercised.payload[0] != 1
    {
        return Err(rt::REFUSED);
    }
    rt::send_for(client[2], 9, &[1], 5000)?;
    let withdrawn = rt::receive_for(client[1], 5000)?;
    if withdrawn.kind != 1 || withdrawn.tag != 3 || withdrawn.len != 1 || withdrawn.payload[0] != 1
    {
        return Err(rt::REFUSED);
    }
    rt::send_for(second[2], 2, &[1], 5000)?;
    let fresh = rt::receive_for(second[1], 5000)?;
    if fresh.kind != 1 || fresh.tag != 3 || fresh.len != 16 {
        return Err(rt::REFUSED);
    }
    rt::send_for(client[2], 3, &fresh.payload[..16], 5000)?;
    let result = rt::receive_for(client[1], 5000)?;
    if result.kind != 1 || result.tag != 4 || result.len != 16 {
        return Err(rt::REFUSED);
    }
    rt::send_for(second[2], 3, &result.payload[..16], 5000)?;
    fn collect(handle: u64, forced: bool) -> rt::Result<()> {
        let end = now()?.checked_add(15_000).ok_or(rt::LIMIT)?;
        loop {
            let mut w = [0; 16];
            w[0] = handle;
            match syscall(proc::POLL, &mut w) {
                Ok(0) if !forced && w[1] == 0 => return Ok(()),
                Ok(2) if forced && w[1] == 125 => return Ok(()),
                Err(rt::WOULD_BLOCK) if now()? < end => {
                    rt::call(rt::THREAD_YIELD, 0, 0, 0);
                }
                _ => return Err(rt::REFUSED),
            }
        }
    }
    collect(client[0], false)?;
    collect(second[0], false)?;
    rt::send_for(verifier[2], 1, &[0], 5000)?;
    collect(verifier[0], false)?;
    let msg = b"NA3 quatre processus collectes";
    rt::value(rt::call(
        rt::DEBUG_WRITE,
        msg.as_ptr() as u64,
        msg.len() as u64,
        0,
    ))?;
    Ok(())
}
