//! Seul l'adaptateur du pilote voit l'en-tête VirtIO et le handle du bail.
use super::{runtime::*, virtio_net};
#[path = "backend.rs"]
mod backend;
#[path = "canal.rs"]
mod canal;
use canal::*;
fn materiel(key: u64, epoch: u64, seq: &mut u64, op: u64, len: usize) -> Result<usize> {
    *seq = seq.checked_add(1).ok_or(LIMIT)?;
    mark(6, len as u64);
    send_frame(key, op, &pair(epoch, *seq), 0, false)?;
    for _ in 0..500000 {
        if cfg(3) == *seq {
            return if cfg(4) == OK {
                Ok(cfg(5) as usize)
            } else {
                Err(cfg(4))
            };
        }
        call(THREAD_YIELD, 0, 0, 0);
    }
    Err(TIMED_OUT)
}
fn preparer(
    coupon: usize,
    buf: &[u8],
    slot: &mut [u8],
    coupons: &mut [Option<usize>; 16],
    saturation: &mut bool,
    hw: &mut impl FnMut(u64, usize) -> Result<usize>,
) -> Result<()> {
    require(coupon < backend::CREDITS && !coupons.contains(&Some(coupon)))?;
    let n = virtio_net::encode_tx(buf, slot).map_err(|_| BAD_ARGUMENT)?;
    let id = hw(2, n)?;
    require(id < coupons.len() && coupons[id].is_none())?;
    coupons[id] = Some(coupon);
    if cfg(16) == 1 && !*saturation && coupons.iter().flatten().count() == backend::CREDITS {
        require(hw(2, n) == Err(WOULD_BLOCK))?;
        *saturation = true;
    }
    Ok(())
}
fn autorisee(frame: &[u8]) -> bool {
    frame.len() >= 60
        && frame[..6] == [0x52, 0x54, 0, 0x12, 0x34, 0x56]
        && frame[6..12] == [2, 0x4e, 0x56, 0, 0, 1]
        && frame[12..14] == [0x88, 0xb5]
}
pub(super) fn executer(handle: u64, epoch: u64) -> Result<()> {
    require(
        handle != 0 && epoch != 0 && call(IPC_DUP, handle, u64::from(SEND), 0).status == DENIED,
    )?;
    let lien = Canal::new(cfg(15), cfg(14), cfg(14))?;
    let slot = unsafe { core::slice::from_raw_parts_mut(0x401100 as *mut u8, 1526) };
    let mut frame = [0; FRAME];
    let mut seq = 0;
    let mut coupons = [None; 16];
    let mut rx = 0;
    let mut saturation_verifiee = false;
    loop {
        let msg = lien.recevoir(DRIVER)?;
        require(msg.len >= 16)?;
        let op = u64::from_le_bytes(msg.payload[..8].try_into().unwrap());
        let arg = u64::from_le_bytes(msg.payload[8..16].try_into().unwrap());
        require(op == backend::PREPARER_LOT || msg.len == 16)?;
        let mut hw = |tag, len| materiel(handle, epoch, &mut seq, tag, len);
        use backend::*;
        let res = match op {
            OUVRIR if arg == 0 => hw(1, 0),
            PREPARER_LOT if lien.boite && (1..=8).contains(&arg) => {
                let count = arg as usize;
                require(msg.len as usize == 16 + 4 * count)?;
                let mut lot = Trames::vide();
                let mut ids = [0; 8];
                let mut mask = 0u16;
                for (i, d) in msg.payload[16..msg.len as usize]
                    .chunks_exact(4)
                    .enumerate()
                {
                    let (id, len) = entree(u32::from_le_bytes(d.try_into().unwrap()));
                    require(id < CREDITS && mask & (1 << id) == 0 && !coupons.contains(&Some(id)))?;
                    mask |= 1 << id;
                    ids[i] = id;
                    lot.ajouter(len)?;
                }
                let n = lot.longueur();
                lien.recevoir_lot(&mut lot.bytes[..n])?;
                for (i, &id) in ids[..count].iter().enumerate() {
                    preparer(
                        id,
                        lot.trame(i),
                        slot,
                        &mut coupons,
                        &mut saturation_verifiee,
                        &mut hw,
                    )?;
                }
                Ok(count)
            }
            COMPLETIONS if lien.boite && (1..=8).contains(&arg) => {
                let mut mask = 0usize;
                for _ in 0..arg {
                    match hw(7, 0) {
                        Err(WOULD_BLOCK) => break,
                        Err(e) => return Err(e),
                        Ok(id) => {
                            let coupon =
                                coupons.get_mut(id).and_then(Option::take).ok_or(REFUSED)?;
                            require(mask & (1 << coupon) == 0)?;
                            mask |= 1 << coupon;
                        }
                    }
                }
                Ok(mask)
            }
            RECEPTION_LOT if lien.boite => {
                let limite = (arg & 255) as usize;
                require(arg <= 0x108 && (1..=8).contains(&limite))?;
                if rx == 16 && arg & 256 != 0 {
                    hw(6, 0)?;
                    rx = 0;
                }
                let mut lot = Trames::vide();
                let mut lengths = [0u32; 8];
                let mut n = 0;
                while n < limite && rx < 16 {
                    let bytes = match hw(8, 0) {
                        Ok(v) => v,
                        Err(WOULD_BLOCK) => break,
                        Err(e) => return Err(e),
                    };
                    let frame = virtio_net::decode_rx(slot.get(..bytes).ok_or(REFUSED)?)
                        .map_err(|_| REFUSED)?;
                    rx += 1;
                    if !autorisee(frame) {
                        break;
                    }
                    lot.ajouter(frame.len())?.copy_from_slice(frame);
                    lengths[n] = frame.len() as u32;
                    n += 1;
                }
                let mut msg = [0; 48];
                let size = liste(OK, &lengths[..n], &mut msg);
                lien.envoyer_lot(&lot.bytes[..lot.longueur()], || {
                    lien.envoyer(DRIVER, &msg[..size])
                })?;
                continue;
            }
            PREPARER => {
                let coupon = (arg >> 32) as usize;
                let len = arg as u32 as usize;
                require(coupon < CREDITS && !coupons.contains(&Some(coupon)))?;
                let buf = frame.get_mut(..len).ok_or(BAD_ARGUMENT)?;
                lien.donnees(false, 1, buf)?;
                preparer(
                    coupon,
                    buf,
                    slot,
                    &mut coupons,
                    &mut saturation_verifiee,
                    &mut hw,
                )?;
                Ok(coupon)
            }
            PUBLIER if (1..=8).contains(&arg) => {
                if lien.boite && rx == 16 {
                    hw(6, 0)?;
                    rx = 0;
                }
                let published = hw(31 + arg, 0);
                if published.is_ok() {
                    match cfg(16) {
                        3 => return Err(CANCELLED),
                        6 => {
                            let ancien = hw(7, 0);
                            require(matches!(ancien, Err(DENIED | REVOKED)))?;
                            lien.envoyer(DRIVER, &pair(REVOKED, 0))?;
                            return Err(REVOKED);
                        }
                        4 => {
                            lien.envoyer(DRIVER, &pair(TIMED_OUT, 0))?;
                            continue;
                        }
                        _ => {}
                    }
                }
                published
            }
            COMPLETION if arg == 0 => {
                hw(7, 0).and_then(|id| coupons.get_mut(id).and_then(Option::take).ok_or(REFUSED))
            }
            ECOUTER if arg == 0 => {
                if rx == 16 {
                    hw(6, 0)?;
                    rx = 0;
                }
                Ok(0)
            }
            RECEPTION if arg == 0 && rx == 16 => Err(WOULD_BLOCK),
            RECEPTION if arg == 0 => hw(8, 0),
            FERMER if arg == 0 => hw(4, 0),
            _ => Err(DENIED),
        };
        if op == RECEPTION
            && let Ok(bytes) = res
        {
            let frame =
                virtio_net::decode_rx(slot.get(..bytes).ok_or(REFUSED)?).map_err(|_| REFUSED)?;
            rx += 1;
            // Toute complétion consomme un des seize descripteurs avant le réarmement.
            // Un paquet destiné à un autre protocole ne doit pas interrompre la session.
            if !autorisee(frame) {
                lien.envoyer(DRIVER, &pair(WOULD_BLOCK, 0))?;
                continue;
            }
            let n = frame.len();
            lien.envoyer(DRIVER, &pair(OK, n as u64))?;
            lien.donnees(true, 1, &mut slot[12..12 + n])?;
        } else {
            lien.envoyer(
                DRIVER,
                &pair(res.err().unwrap_or(OK), res.unwrap_or(0) as u64),
            )?;
        }
        if op == FERMER {
            return res.map(|_| ());
        }
    }
}
