//! Bail privé sur un seul emplacement matériel. Son identité associe PID et génération.
//! Le chemin IRQ et la mort d'un processus n'attendent jamais le mutex du transport.
use super::*;
#[allow(dead_code)]
#[path = "../../../../abi/block.rs"]
pub(crate) mod wire;
use crate::{abi, sched::sync::Mutex};
pub(crate) use wire::PORTAL;
static OWNER: AtomicU64 = AtomicU64::new(0);
static EPOCH: AtomicU64 = AtomicU64::new(0);
static ACTIF: AtomicU64 = AtomicU64::new(0);
static SESSION: Mutex<Session> = Mutex::new(Session {
    device: None,
    quarantine: false,
    rights: 0,
    phase: Phase::Empty,
});
#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Empty,
    Pending(u64),
    Readable(u64, usize),
    Writing(u64, usize),
    Closed,
}
struct Session {
    device: Option<Bail>,
    quarantine: bool,
    rights: u8,
    phase: Phase,
}
pub(crate) fn begin_session(rights: u8) -> Result<u64> {
    begin_with(rights, Transport::open)
}
pub(super) fn begin_with(rights: u8, open: impl FnOnce() -> Result<Bail>) -> Result<u64> {
    let mut s = SESSION.lock();
    if s.quarantine || s.device.is_some() || OWNER.load(Ordering::Acquire) != 0 {
        return Err(Error::Quarantine);
    }
    let epoch = EPOCH
        .try_update(Ordering::AcqRel, Ordering::Acquire, |v| v.checked_add(1))
        .map_err(|_| Error::State)?
        + 1;
    match open() {
        Ok(d) => {
            s.device = Some(d);
            s.phase = Phase::Empty;
            s.rights = rights
                & (if s.device.as_ref().unwrap().ro {
                    !2
                } else {
                    255
                });
            ACTIF.store(epoch, Ordering::Release);
            Ok(epoch)
        }
        Err(e) => {
            if e != Error::Absent {
                s.quarantine = true
            }
            Err(e)
        }
    }
}
/// Le lanceur a construit le processus, dont le PID ne sera jamais réutilisé.
pub(crate) fn attach(pid: u64, epoch: u64) -> bool {
    pid != 0
        && epoch != 0
        && ACTIF.load(Ordering::Acquire) == epoch
        && OWNER
            .compare_exchange(0, pid, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
}
pub(crate) fn admit(pid: u64, key: u64) -> bool {
    pid != 0
        && OWNER.load(Ordering::Acquire) == pid
        && key != 0
        && ACTIF.load(Ordering::Acquire) == key
}
pub(crate) fn died(pid: u64) {
    // La fermeture par le superviseur et la dernière validation avant le kick
    // doivent s'ordonner même si le superviseur s'exécute sur un autre CPU.
    let _gate = crate::sched::Guard::new();
    if pid != 0
        && OWNER
            .compare_exchange(pid, 0, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    {
        ACTIF.store(0, Ordering::Release);
    }
}
pub(crate) fn retire() -> Result<()> {
    let mut s = SESSION.lock();
    {
        let _gate = crate::sched::Guard::new();
        ACTIF.store(0, Ordering::Release);
        OWNER.store(0, Ordering::Release);
    }
    s.phase = Phase::Closed;
    let retrait = match s.device.take() {
        Some(d) => d.close(),
        None => ownership::reprendre(),
    };
    if let Err(err) = retrait {
        s.quarantine = true;
        return Err(err);
    }
    s.quarantine = false;
    Ok(())
}
pub(crate) fn command(pid: u64, key: u64, w: &mut wire::Wire) -> core::result::Result<(), u64> {
    use wire::*;
    if !admit(pid, key) {
        return Err(abi::DENIED);
    }
    let mut s = SESSION.lock();
    if !admit(pid, key) || s.quarantine {
        return Err(abi::REVOKED);
    }
    let before = metrics::stamp();
    let hw = matches!(
        (w[1], w[2]),
        (wire::READ, wire::BEGIN) | (wire::WRITE, wire::END) | (wire::FLUSH, wire::BEGIN)
    );
    let mut inputs = *w;
    if hw {
        inputs[32..].fill(0)
    }
    if !syntax(&inputs) {
        return Err(abi::BAD_ARGUMENT);
    }
    let right = match w[1] {
        INFO | READ | DISCOVER => 1,
        WRITE => 2,
        FLUSH => 4,
        CLOSE => 8,
        _ => return Err(abi::BAD_ARGUMENT),
    };
    if s.rights & right == 0 {
        return Err(abi::DENIED);
    }
    let rights = s.rights;
    let phase = s.phase;
    if phase == Phase::Closed {
        return Err(abi::CLOSED);
    }
    let d = s.device.as_deref_mut().ok_or(abi::CLOSED)?;
    let op = w[1];
    let l = lba(w);
    if hw {
        if !descriptors(d.size, w) {
            return Err(abi::BAD_ARGUMENT);
        }
        metrics::record(3, before);
        w[32..].fill(0);
    }
    match (op, w[2], phase) {
        (DISCOVER, BEGIN, Phase::Empty) => {
            *w = discovery(number(w, 40) & IMPLEMENTED, true);
        }
        (INFO, BEGIN, Phase::Empty) => {
            put(w, 4, d.size as u32);
            put(w, 16, u32::from(rights));
            w[8..16].copy_from_slice(&d.count.to_le_bytes());
        }
        (READ, BEGIN, Phase::Empty) => {
            if l >= d.count {
                return Err(abi::BAD_ARGUMENT);
            }
            metrics::begin();
            if d.entree_autorisee(d.first + l, 0, d.size, || admit(pid, key))
                .is_err()
            {
                s.phase = Phase::Closed;
                return Err(abi::REFUSED);
            }
            s.phase = Phase::Pending(l);
        }
        (READ, END, Phase::Pending(old)) if l == old => {
            if d.finish_read().is_err() {
                s.phase = Phase::Closed;
                return Err(abi::REFUSED);
            }
            s.phase = Phase::Readable(l, 0);
        }
        (READ, PIECE, Phase::Readable(old, at)) if l == old => {
            let n = number(w, 4) as usize;
            if number(w, 16) as usize != at || at.checked_add(n).is_none_or(|end| end > d.size) {
                return Err(abi::BAD_ARGUMENT);
            }
            let mark = metrics::stamp();
            d.p.0[20].mem.bytes(at, &mut w[32..32 + n]);
            metrics::record(4, mark);
            let next = at + n;
            s.phase = if next == d.size {
                metrics::end(d.size);
                Phase::Empty
            } else {
                Phase::Readable(l, next)
            };
        }
        (WRITE, BEGIN, Phase::Empty) => {
            if l >= d.count {
                return Err(abi::BAD_ARGUMENT);
            }
            metrics::begin();
            s.phase = Phase::Writing(l, 0);
        }
        (WRITE, PIECE, Phase::Writing(old, at)) if l == old => {
            let n = number(w, 4) as usize;
            if number(w, 16) as usize != at || at.checked_add(n).is_none_or(|end| end > d.size) {
                return Err(abi::BAD_ARGUMENT);
            }
            let mark = metrics::stamp();
            d.p.0[22].mem.put(at, &w[32..32 + n]);
            metrics::record(5, mark);
            w[32..].fill(0);
            s.phase = Phase::Writing(l, at + n);
        }
        (WRITE, END, Phase::Writing(old, at)) if l == old && at == d.size => {
            if d.sortie_autorisee(d.first + l, false, d.size, || admit(pid, key))
                .is_err()
            {
                s.phase = Phase::Closed;
                return Err(abi::REFUSED);
            }
            metrics::end(d.size);
            s.phase = Phase::Empty;
        }
        (FLUSH, BEGIN, Phase::Empty) => {
            if d.sortie_autorisee(0, true, d.size, || admit(pid, key))
                .is_err()
            {
                s.phase = Phase::Closed;
                return Err(abi::REFUSED);
            }
        }
        (CLOSE, BEGIN, Phase::Empty) => s.phase = Phase::Closed,
        _ => return Err(abi::BAD_ARGUMENT),
    }
    Ok(())
}

/// Portail page : les adresses de la chaîne sont construites dans le noyau.
pub(crate) fn page_command(
    pid: u64,
    key: u64,
    w: &wire::Wire,
    page: &mut [u8; 4096],
) -> core::result::Result<(), u64> {
    let mut s = SESSION.lock();
    if !admit(pid, key) || s.quarantine {
        return Err(abi::REVOKED);
    }
    if !wire::syntax(w) || w[2] != wire::PAGE_PHASE {
        return Err(abi::BAD_ARGUMENT);
    }
    let write = w[1] == wire::WRITE;
    if s.rights & (if write { 2 } else { 1 }) == 0 {
        return Err(abi::DENIED);
    }
    if s.phase != Phase::Empty {
        return Err(abi::REFUSED);
    }
    let d = s.device.as_deref_mut().ok_or(abi::CLOSED)?;
    let lba = wire::lba(w);
    let sectors = (4096 / d.size) as u64;
    if lba.checked_add(sectors).is_none_or(|end| end > d.count) {
        return Err(abi::BAD_ARGUMENT);
    }
    let start = d.first.checked_add(lba).ok_or(abi::BAD_ARGUMENT)?;
    metrics::begin();
    let result = if write {
        d.p.0[22].mem.write_page(page);
        d.sortie_autorisee(start, false, 4096, || admit(pid, key))
    } else {
        d.entree_autorisee(start, 0, 4096, || admit(pid, key))
            .and_then(|()| d.finish(4097))
            .map(|()| d.p.0[20].mem.read_page(page))
    };
    if result.is_err() {
        s.phase = Phase::Closed;
        return Err(abi::REFUSED);
    }
    metrics::end(4096);
    Ok(())
}

pub(crate) fn private_frames() -> Result<[u64; 24]> {
    let s = SESSION.lock();
    let d = s.device.as_ref().ok_or(Error::State)?;
    let f = d.p.0.map(|f| f.pa);
    #[cfg(target_arch = "x86_64")]
    {
        let mut f = f;
        d.unit.witnesses(&mut f);
        return Ok(f);
    }
    #[cfg(target_arch = "aarch64")]
    {
        Ok(f)
    }
}

pub(crate) fn inflight() -> bool {
    let s = SESSION.lock();
    s.device
        .as_ref()
        .is_some_and(|d| d.pending.is_some() && d.q.pages[2].mem.r16(2) != d.q.index)
}

/// Décoder la proposition EL0/Ring3. L'ordre exact exclut cycles, INDIRECT,
/// adresses libres, troncatures et destinations MSI. Le transport matérialise
/// uniquement cette forme dans ses propres pages ; il ne garde aucun emprunt utilisateur.
fn descriptors(size: usize, w: &wire::Wire) -> bool {
    let flush = w[1] == wire::FLUSH;
    let read = w[1] == wire::READ;
    let Some(sector) = wire::lba(w).checked_mul((size / 512) as u64) else {
        return false;
    };
    if wire::number(w, 32)
        != (if flush {
            4
        } else if read {
            0
        } else {
            1
        })
        || wire::number(w, 36) != 0
        || u64::from_le_bytes(w[40..48].try_into().unwrap()) != sector
    {
        return false;
    }
    let seg = |i: usize, id: u8, flags: u8, next: u8, count: u32| {
        let at = 48 + 8 * i;
        w[at..at + 4] == [id, flags, next, 0] && wire::number(w, at + 4) == count
    };
    if !seg(0, 1, 1, 1, 16) {
        return false;
    }
    if flush {
        seg(1, 3, 2, 0, 1) && w[64..].iter().all(|&b| b == 0)
    } else {
        seg(
            1,
            if read { 2 } else { 4 },
            if read { 3 } else { 1 },
            2,
            size as u32,
        ) && seg(2, 3, 2, 0, 1)
            && w[72..].iter().all(|&b| b == 0)
    }
}

pub(crate) fn publication_unique() -> bool {
    let s = SESSION.lock();
    s.device
        .as_ref()
        .is_some_and(|d| d.q.index == 1 && d.p.0[17].mem.r16(2) == 1)
}
