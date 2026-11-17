//! Le superviseur de qualification démarre les deux services avant d'accepter les demandes.
use super::*;
pub(super) fn launch() -> rt::Result<()> {
    for id in [17, 18] {
        let mut frame = [0; 16];
        frame[0] = id;
        frame[4] = 1;
        syscall(proc::START, &mut frame)?;
    }
    Ok(())
}
