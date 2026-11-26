//! Compteurs du banc monoprocesseur. Aucun handle ou compteur n'est exporté aux acteurs.
use super::*;
#[path = "profil_detail.rs"]
pub(crate) mod detail;
const ZERO: AtomicU64 = AtomicU64::new(0);
static CASE: AtomicU64 = ZERO;
static CELL: AtomicU64 = ZERO;
static FRONT: [AtomicU64; 2] = [const { ZERO }; 2];
static HW: AtomicU64 = ZERO;
static SWITCH: AtomicU64 = ZERO;
static WAKES: AtomicU64 = ZERO;
const COLS: usize = 30;
static COST: [[AtomicU64; COLS]; 3] = [const { [const { ZERO }; COLS] }; 3];

pub(super) fn initialiser(case: u64, front: [u64; 2], hw: u64) {
    detail::reset();
    CASE.store(case, Relaxed);
    CELL.store(0, Relaxed);
    for (n, v) in FRONT.iter().zip(front) {
        n.store(v, Relaxed);
    }
    HW.store(hw, Relaxed);
    for row in &COST {
        for n in row {
            n.store(0, Relaxed);
        }
    }
}
pub(super) fn actif() -> bool {
    (12..=19).contains(&CASE.load(Relaxed))
}
fn inc(i: usize, n: u64) {
    if let Some(row) = CELL.load(Relaxed).checked_sub(1) {
        COST[row as usize][i].fetch_add(n, Relaxed);
    }
}
pub(super) fn debut(b: u64, wakes: u64) {
    if !actif() {
        return;
    }
    let cell = match b {
        1 => 1,
        4 => 2,
        8 => 3,
        _ => return,
    };
    CELL.store(cell, Relaxed);
    detail::entrer(0, 0);
    inc(0, 1); // l'entrée SEND du client précède le point de départ du chronomètre.
    SWITCH.store(crate::sched::profile::switches_bsp(), Relaxed);
    WAKES.store(wakes, Relaxed);
    if matches!(CASE.load(Relaxed), 16..=19) {
        hal::log(format_args!("E3B admission_rx {b}"));
    }
}
pub(super) fn appel(pid: u64, id: u64, args: [u64; 3], wakes: u64) {
    if CELL.load(Relaxed) == 0 {
        return;
    }
    let Some(actor) = ACTEURS.iter().position(|p| p.load(Relaxed) == pid) else {
        return;
    };
    if actor == 0 && id == abi::IPC_FEATURES {
        fin_lot();
        inc(
            7,
            crate::sched::profile::switches_bsp() - SWITCH.load(Relaxed),
        );
        inc(8, wakes - WAKES.load(Relaxed));
        CELL.store(0, Relaxed);
        return;
    }
    if id == abi::THREAD_YIELD {
        inc(9, 1);
        return;
    }
    if !matches!(
        id,
        abi::IPC_SEND | abi::IPC_RECEIVE | abi::IPC_PUT | abi::IPC_TAKE | abi::IPC_NOTIFY
    ) {
        return;
    }
    let front = actor == 0 || (actor == 1 && FRONT.iter().any(|v| v.load(Relaxed) == args[0]));
    let hw = actor == 2 && args[0] == HW.load(Relaxed);
    let classe = if front {
        0
    } else if hw {
        2
    } else {
        1
    };
    detail::entrer(actor, classe);
    inc(classe, 1);
    if id == abi::IPC_NOTIFY {
        inc(10, 1);
    }
}
pub(super) fn attente(pid: u64) {
    detail::bloque(pid);
    if CELL.load(Relaxed) != 0 && ACTEURS.iter().any(|p| p.load(Relaxed) == pid) {
        inc(6, 1);
    }
}
pub(crate) fn copie(pid: u64, len: u64) {
    if CELL.load(Relaxed) != 0 && ACTEURS.iter().any(|p| p.load(Relaxed) == pid) {
        inc(3, 1);
        inc(4, len);
    }
}
pub(crate) fn controle(pid: u64, tag: u64, payload: &[u8]) {
    detail::message(pid, tag, payload);
    if CELL.load(Relaxed) == 0
        || pid != CLIENT_PID.load(Relaxed)
        || tag != 0x45544831
        || payload.len() < 32
    {
        return;
    }
    match u16::from_le_bytes([payload[4], payload[5]]) {
        3 => inc(11, 1),
        4 => inc(12, 1),
        5 => inc(13, 1),
        6 => inc(14, 1),
        13 => inc(15, 1),
        _ => {}
    }
}
pub(super) fn courtier(op: u64, ticks: u64, result: Option<usize>) {
    let col = match op {
        2 => 16,
        7 => 18,
        8 => 20,
        6 => 22,
        32..=39 => 24,
        _ => 26,
    };
    inc(col, 1);
    inc(col + 1, ticks);
    if result.is_some() {
        if (32..=39).contains(&op) {
            inc(28, op - 31);
        }
        if op == 8 {
            inc(29, result.unwrap() as u64);
        }
    }
}
pub(super) fn bilan() {
    if !actif() {
        return;
    }
    detail::bilan();
    for (b, row) in [1, 4, 8].into_iter().zip(&COST) {
        let compte = row.each_ref().map(|n| n.load(Relaxed));
        hal::log(format_args!("E3B cout lot={b} compte={compte:?}"));
    }
}
