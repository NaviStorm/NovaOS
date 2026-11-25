//! Témoin logiciel dans l'invité. Aucun accès au périphérique ou à ses anneaux.
use super::*;
use core::hint::black_box;

pub fn run() -> bool {
    let offered = black_box(u64::MAX);
    if features(offered) != Ok(VERSION_1 | ACCESS_PLATFORM | MAC)
        || features(offered & !ACCESS_PLATFORM) != Err(Error::Features)
    {
        return false;
    }
    let frame = black_box([0xa5u8; 60]);
    let mut copy = [0xff; 128];
    let Ok(bytes) = encode_tx(&frame, &mut copy) else {
        return false;
    };
    if bytes != HEADER + frame.len() || copy[..HEADER] != [0; HEADER] {
        return false;
    }
    copy[10] = 1; // un descripteur de réception dans la fixture logicielle
    if decode_rx(&copy[..bytes]) != Ok(frame.as_slice()) {
        return false;
    }
    for direction in [Direction::Receive, Direction::Transmit] {
        let mut q = Queue::new(direction);
        let mut used = 0u16;
        for _ in 0..black_box(6250) {
            for id in 0..SLOTS {
                if q.reserve() != Ok(id) {
                    return false;
                }
            }
            used = used.wrapping_add(SLOTS);
            if q.reserve() != Err(Error::Full) || q.available_completions(used) != Ok(SLOTS) {
                return false;
            }
            let written = if direction == Direction::Receive {
                bytes as u32
            } else {
                0
            };
            for id in (0..SLOTS).rev() {
                if q.complete(u32::from(black_box(id)), written).is_err() {
                    return false;
                }
            }
            if q.pending() != 0 || q.available_completions(used) != Ok(0) {
                return false;
            }
        }
        if q.complete(0, 0) != Err(Error::Quarantined) || q.reserve() != Err(Error::Quarantined) {
            return false;
        }
    }
    true
}
