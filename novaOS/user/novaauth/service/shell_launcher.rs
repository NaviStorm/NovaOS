//! Services de qualification. Le gestionnaire conserve le droit de collecte du courtier.
use super::*;
pub(super) fn launch() -> rt::Result<u64> {
    let mut last = 0;
    for id in [17, 18, 21] {
        let mut frame = [0; 16];
        frame[0] = id;
        frame[4] = 1;
        last = syscall(proc::START, &mut frame)?;
    }
    Ok(last)
}
pub(super) fn poll(right: &mut u64) -> rt::Result<()> {
    if *right == 0 {
        return Ok(());
    }
    let mut frame = [0; 16];
    frame[0] = *right;
    match syscall(proc::POLL, &mut frame) {
        Err(rt::WOULD_BLOCK) => Ok(()),
        Ok(_) => {
            *right = 0;
            let message: &[u8] = match frame[1] {
                41 => b"NXRELAY superviseur collecte arret avant remise",
                42 => b"NXRELAY superviseur collecte arret apres remise",
                0 => b"NXRELAY superviseur collecte arret normal",
                _ => b"NXRELAY superviseur collecte sortie inattendue",
            };
            rt::value(rt::call(
                rt::DEBUG_WRITE,
                message.as_ptr() as u64,
                message.len() as u64,
                0,
            ))?;
            Ok(())
        }
        Err(e) => Err(e),
    }
}
