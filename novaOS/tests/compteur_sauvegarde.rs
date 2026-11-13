//! Banc isolé, lié uniquement par io-0166-sauvegarde.patch. Un seul vCPU.
//! La matrice relève les deux sens et sépare les dons de capacités des messages.
use crate::hal;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering::Relaxed};
static ACTIVE: AtomicBool = AtomicBool::new(false);
static EDITOR: AtomicU64 = AtomicU64::new(0);
static START: AtomicU64 = AtomicU64::new(0);
static LAST: AtomicU64 = AtomicU64::new(0);
static COUNTS: [AtomicU64; 25] = [const { AtomicU64::new(0) }; 25];
static GIFTS: AtomicU64 = AtomicU64::new(0);
static LOCAL: [AtomicU64; 7] = [const { AtomicU64::new(0) }; 7];
static UNKNOWN: AtomicU64 = AtomicU64::new(0);
static PRET: [AtomicU64; 6] = [const { AtomicU64::new(0) }; 6];
static BULK: [AtomicU64; 2] = [const { AtomicU64::new(0) }; 2];
static PHASE: [AtomicU64; 3] = [const { AtomicU64::new(0) }; 3];
static LOTS: [AtomicU64; 6] = [const { AtomicU64::new(0) }; 6];
static BARRIERES: [AtomicU64; 4] = [const { AtomicU64::new(0) }; 4];
static TEMPS_LOTS: [AtomicU64; 2] = [const { AtomicU64::new(0) }; 2];
static DUREE_MOTEUR: [AtomicU64; 4] = [const { AtomicU64::new(0) }; 4];
static ENTREE_MOTEUR: [AtomicU64; 4] = [const { AtomicU64::new(0) }; 4];
static ENTREE_LOT: AtomicU64 = AtomicU64::new(0);
static APPELS_FILE: [AtomicU64; 5] = [const { AtomicU64::new(0) }; 5];
static HW: [AtomicU64; 6] = [const { AtomicU64::new(0) }; 6];

pub fn delivery(from: u64, to: u64, kind: u32) {
    if !ACTIVE.load(Relaxed) {
        return;
    }
    if kind == 3 {
        GIFTS.fetch_add(1, Relaxed);
        return;
    }
    let index = |p| match p {
        1..=4 => Some(p as usize - 1),
        p if p == EDITOR.load(Relaxed) => Some(4),
        _ => None,
    };
    match (index(from), index(to), kind) {
        (Some(i), Some(j), 1) => {
            COUNTS[5 * i + j].fetch_add(1, Relaxed);
        }
        _ => unexpected(),
    }
}
pub fn unexpected() {
    if ACTIVE.load(Relaxed) {
        UNKNOWN.fetch_add(1, Relaxed);
    }
}
pub fn control(id: u64) {
    use crate::abi::*;
    if !ACTIVE.load(Relaxed) {
        return;
    }
    let i = match id {
        IPC_CREATE => 0,
        IPC_DUP => 1,
        IPC_CLOSE => 2,
        IPC_REVOKE => 3,
        IPC_NOTIFY => 4,
        IPC_CANCEL => 5,
        IPC_BOOTSTRAP => 6,
        _ => return,
    };
    LOCAL[i].fetch_add(1, Relaxed);
}
pub fn appel_file(id: u64) {
    if ACTIVE.load(Relaxed) && (0x10034..=0x10039).contains(&id) {
        PRET[(id - 0x10034) as usize].fetch_add(1, Relaxed);
    }
    if ACTIVE.load(Relaxed) && (0x1002e..=0x10032).contains(&id) {
        APPELS_FILE[(id - 0x1002e) as usize].fetch_add(1, Relaxed);
    }
}
pub fn lot(put: bool) {
    if ACTIVE.load(Relaxed) {
        BULK[usize::from(!put)].fetch_add(1, Relaxed);
    }
}
pub fn bloc(op: usize, d: u64) {
    if op < 3 && ACTIVE.load(Relaxed) {
        HW[2 * op].fetch_add(1, Relaxed);
        HW[2 * op + 1].fetch_add(d, Relaxed);
    }
}
pub fn marker(pid: u64, s: &[u8]) -> bool {
    let now = hal::timer::counter();
    if pid > 4 && s == b"SAVE start" {
        for c in COUNTS
            .iter()
            .chain(&BULK)
            .chain(&PRET)
            .chain(&LOCAL)
            .chain(&LOTS)
            .chain(&BARRIERES)
            .chain(&TEMPS_LOTS)
            .chain(&DUREE_MOTEUR)
            .chain(&PHASE)
            .chain(&APPELS_FILE)
            .chain(&HW)
            .chain([&GIFTS, &UNKNOWN])
        {
            c.store(0, Relaxed);
        }
        EDITOR.store(pid, Relaxed);
        START.store(now, Relaxed);
        ACTIVE.store(true, Relaxed);
        return true;
    }
    if !ACTIVE.load(Relaxed) {
        return false;
    }
    if pid == 2 && s.len() == 5 && &s[..3] == b"CPU" && s[3] < 4 && s[4] <= 1 {
        let i = usize::from(s[3]);
        if s[4] == 0 {
            ENTREE_MOTEUR[i].store(now, Relaxed);
        } else {
            DUREE_MOTEUR[i].fetch_add(now - ENTREE_MOTEUR[i].load(Relaxed), Relaxed);
        }
        return true;
    }
    if pid == 3 && s.len() == 13 && &s[..4] == b"LOT " && s[4] < 3 {
        let i = 2 * usize::from(s[4]);
        LOTS[i].fetch_add(1, Relaxed);
        LOTS[i + 1].fetch_add(u64::from_le_bytes(s[5..].try_into().unwrap()), Relaxed);
        return true;
    }
    if pid == 2
        && matches!(
            s,
            b"LOTS lire" | b"LOTS ecrire" | b"LOTS lu" | b"LOTS ecrit"
        )
    {
        if matches!(s, b"LOTS lire" | b"LOTS ecrire") {
            ENTREE_LOT.store(now, Relaxed);
        } else {
            let i = usize::from(s == b"LOTS ecrit");
            TEMPS_LOTS[i].fetch_add(now - ENTREE_LOT.load(Relaxed), Relaxed);
        }
        return true;
    }
    if pid == 2 && matches!(s, b"LOT flush debut" | b"LOT flush fin") {
        if let Some(slot) = BARRIERES.iter().find(|v| v.load(Relaxed) == 0) {
            slot.store(now - START.load(Relaxed), Relaxed);
        } else {
            unexpected();
        }
        return true;
    }
    if pid == 2 {
        let phase = match s {
            b"SAVE read" => None,
            b"SAVE prepare" => Some(0),
            b"SAVE commit" => Some(1),
            b"SAVE lot-end" => Some(2),
            _ => return false,
        };
        let last = LAST.swap(now, Relaxed);
        if let Some(i) = phase {
            PHASE[i].fetch_add(now - last, Relaxed);
        }
        return true;
    }
    if pid != EDITOR.load(Relaxed) || s != b"SAVE end" {
        return false;
    }
    ACTIVE.store(false, Relaxed);
    hal::log(format_args!(
        "SAVE pret_local={:?}",
        PRET.each_ref().map(|v| v.load(Relaxed))
    ));
    if DUREE_MOTEUR.iter().any(|t| t.load(Relaxed) > 0) {
        hal::log(format_args!(
            "SAVE durees_moteur={:?}",
            DUREE_MOTEUR.each_ref().map(|t| t.load(Relaxed))
        ));
    }
    if TEMPS_LOTS.iter().any(|t| t.load(Relaxed) > 0) {
        hal::log(format_args!(
            "SAVE durees_lots={:?}",
            TEMPS_LOTS.each_ref().map(|t| t.load(Relaxed))
        ));
    }
    if LOTS[2].load(Relaxed) > 0 {
        hal::log(format_args!(
            "SAVE groupes={:?} barrieres={:?}",
            LOTS.each_ref().map(|v| v.load(Relaxed)),
            BARRIERES.each_ref().map(|v| v.load(Relaxed))
        ));
    }
    if APPELS_FILE[0].load(Relaxed) > 0 {
        hal::log(format_args!(
            "SAVE appels_file={:?}",
            APPELS_FILE.each_ref().map(|v| v.load(Relaxed))
        ));
    }
    for (i, p) in [1, 2, 3, 4, pid].into_iter().enumerate() {
        let row = core::array::from_fn::<_, 5, _>(|j| COUNTS[5 * i + j].load(Relaxed));
        hal::log(format_args!("SAVE from={p} vers={row:?}"));
    }
    hal::log(format_args!(
        "SAVE duree={} dons={} autres={} put={} take={}",
        now - START.load(Relaxed),
        GIFTS.load(Relaxed),
        UNKNOWN.load(Relaxed),
        BULK[0].load(Relaxed),
        BULK[1].load(Relaxed)
    ));
    hal::log(format_args!(
        "SAVE controles_locaux={:?}",
        LOCAL.each_ref().map(|v| v.load(Relaxed))
    ));
    hal::log(format_args!(
        "SAVE phases={:?} bloc={:?}",
        PHASE.each_ref().map(|v| v.load(Relaxed)),
        HW.each_ref().map(|v| v.load(Relaxed))
    ));
    true
}
