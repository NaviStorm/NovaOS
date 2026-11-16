//! Le canal d'enrôlement de l'installation neuve appartient au lanceur seul.
use super::*;
pub(super) fn launch() -> rt::Result<()> {
    fn start(id: u64) -> rt::Result<Frame> {
        let mut w = [0; 16];
        w[0] = id;
        w[4] = 1;
        w[0] = syscall(proc::START, &mut w)?;
        Ok(w)
    }
    let verifier = start(17)?;
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
    if result.payload[16..24] != [0; 8] {
        return Err(rt::REFUSED);
    }
    let other = rt::receive_for(second[1], 5000)?;
    if other.kind != 1 || other.tag != 1 || other.len != 16 {
        return Err(rt::BAD_ARGUMENT);
    }
    rt::send_for(client[2], 1, &other.payload[..16], 5000)?;
    let published = rt::receive_for(client[1], 5000)?;
    if published.kind != 1 || published.tag != 2 || published.len != 16 {
        return Err(rt::REFUSED);
    }
    rt::send_for(second[2], 1, &published.payload[..16], 5000)?;
    let live = rt::receive_for(second[1], 5000)?;
    if live.kind != 1 || live.tag != 2 || live.len != 32 {
        return Err(rt::REFUSED);
    }
    rt::send_for(client[2], 2, &live.payload[..32], 5000)?;
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
    fn collect(handle: u64) -> rt::Result<()> {
        let end = now()?.checked_add(15_000).ok_or(rt::LIMIT)?;
        loop {
            let mut w = [0; 16];
            w[0] = handle;
            match syscall(proc::POLL, &mut w) {
                Ok(0) if w[1] == 0 => return Ok(()),
                Err(rt::WOULD_BLOCK) if now()? < end => {
                    rt::call(rt::THREAD_YIELD, 0, 0, 0);
                }
                _ => return Err(rt::REFUSED),
            }
        }
    }
    collect(client[0])?;
    collect(second[0])?;
    rt::send_for(verifier[2], 1, &[0], 5000)?;
    collect(verifier[0])?;
    let msg = b"NA3 trois ELF collectes";
    rt::value(rt::call(
        rt::DEBUG_WRITE,
        msg.as_ptr() as u64,
        msg.len() as u64,
        0,
    ))?;
    Ok(())
}
