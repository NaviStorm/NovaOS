use crate::runtime::*;
#[allow(dead_code)]
#[path = "reprise_wire.rs"]
pub mod wire;
use core::sync::atomic::{AtomicU64, Ordering};
use wire::*;
fn word(i: usize) -> u64 {
    // La page DATA et les bornes du contrat appartiennent à cette image.
    unsafe { (&*(0x401000 as *const AtomicU64).add(i)).load(Ordering::Acquire) }
}
fn depot(key: u64, tag: u64, epoch: u64, n: u64) -> Result<()> {
    let mut b = [0; 16];
    b[..8].copy_from_slice(&epoch.to_le_bytes());
    b[8..].copy_from_slice(&n.to_le_bytes());
    send_frame(key, tag, &b, 0, false)
}
fn attendre(rx: u64, bit: u64) -> Result<()> {
    loop {
        match receive_for(rx, 15000) {
            Ok(m) if m.kind == 2 && m.aux == bit => return Ok(()),
            Err(TIMED_OUT) => continue,
            _ => return Err(REFUSED),
        }
    }
}
pub fn acteur(role: u64, selection: impl FnOnce() -> Result<[u8; 32]>) -> Result<()> {
    let plan = if word(RESTART) == 0 {
        Some(selection()?)
    } else {
        None
    };
    let (tx, rx, epoch) = (word(TX), word(RX), word(LINK));
    if tx == 0 || rx == 0 || epoch == 0 {
        return Err(REFUSED);
    }
    let mut last = 0;
    let mut etat: Option<EtatRepris> = None;
    loop {
        attendre(rx, 1)?;
        let (op, id) = (word(COMMAND), word(NUMBER));
        if id != last + 1 || !matches!(op, READ | PREPARE | LAUNCH | RECEIPT | END) {
            return Err(REFUSED);
        }
        if last == 0 && word(OLD_LINK) != 0 {
            if !matches!(
                depot(word(OLD_KEY), READ, word(OLD_LINK), id),
                Err(DENIED | REVOKED | CLOSED)
            ) {
                return Err(REFUSED);
            }
            depot(tx, op, word(OLD_LINK), id)?;
            attendre(rx, 4)?;
        }
        let intention = etat.as_ref().map_or(0, EtatRepris::intention);
        if op == LAUNCH && (intention == 0 || intention != word(OPERATION) || role != LAUNCHER) {
            return Err(REFUSED);
        }
        depot(tx, op, epoch, if op == LAUNCH { intention } else { id })?;
        attendre(rx, 2)?;
        if word(STATUS) != 0 {
            return Err(REVOKED);
        }
        let s: [u64; WORDS] = core::array::from_fn(|i| word(SNAP + i));
        let nouveau = EtatRepris::lire(s, etat.as_ref()).ok_or(REFUSED)?;
        if plan.is_some_and(|p| p != nouveau.plan()) {
            return Err(REFUSED);
        }
        etat = Some(nouveau);
        for (i, w) in digest(role, epoch, id, &s).into_iter().enumerate() {
            depot(tx, ACK + i as u64, epoch, w)?
        }
        if op == END {
            return attendre(rx, 8);
        }
        last = id;
    }
}
