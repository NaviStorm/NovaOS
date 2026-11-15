//! Seul le port privé du courtier apporte une identité attestée. L'écrivain P9
//! est sérialisé avec les requêtes classiques et les temporaires.
use super::*;
#[allow(dead_code)]
#[path = "../../../abi/cache.rs"]
mod wire;
use cache_store::{Key, Policy, Store};

fn status(e: Error) -> u64 {
    match e {
        Error::Invalid(cache_store::STALE) => rt::REVOKED,
        Error::Invalid(cache_store::UNVERIFIED) => rt::REFUSED,
        e => namespace::error(e),
    }
}
impl Session<'_, '_> {
    fn cache_operation(&mut self, e: &mut [u8; wire::FRAME]) -> rt::Result<()> {
        let op = wire::get(e, wire::OP);
        let g = wire::get(e, wire::GEN);
        let key: Key = e[..32].try_into().unwrap();
        if op == wire::RECOVER && e[128] == 1 {
            // Restauration ou filiation non confirmée par l'autorité hors volume :
            // même une tombe structurellement valide reste sans droit de retrait.
            self.cache = None;
            return Ok(());
        }
        if self.failed.is_some() {
            // La cause mémorisée concerne l'opération précédente. Cette
            // nouvelle demande ne sera jamais remise à l'écrivain condamné.
            return Err(rt::CLOSED);
        }
        if self.pending.is_some() || self.temporary.any_pending() {
            return Err(w::BUSY);
        }
        if !self.writable {
            return Err(rt::DENIED);
        }
        let fs = self.fs.as_mut().ok_or(rt::CLOSED)?;
        if op == wire::RECOVER {
            self.cache = None;
            shape(e[128] == 0)?;
            let limits = [
                wire::get(e, 96),
                wire::get(e, 104),
                wire::get(e, 112),
                wire::get(e, 120),
            ];
            let policy = if limits == [0; 4] {
                Policy::for_volume(fs).map_err(status)?
            } else {
                Policy {
                    cache: limits[0],
                    user: limits[1],
                    global: limits[2],
                    reserve: limits[3],
                }
            };
            self.clock.sample()?;
            let s = Store::mount(fs, policy).map_err(status)?;
            let audit = s.recover(fs).map_err(status)?;
            if audit.quarantined != 0 {
                let b = b"CACHE P9 proprietes en quarantaine";
                rt::call(rt::DEBUG_WRITE, b.as_ptr() as u64, b.len() as u64, 0);
            }
            if audit.purged != 0 {
                let b = b"CACHE P9 reprise purge";
                rt::call(rt::DEBUG_WRITE, b.as_ptr() as u64, b.len() as u64, 0);
            }
            self.cache = Some(s);
            return Ok(());
        }
        let s = self.cache.ok_or(rt::DENIED)?;
        self.clock.sample()?;
        match op {
            wire::OPEN => {
                let id = s.open(fs, &key).map_err(status)?;
                wire::put(e, wire::GEN, id);
            }
            wire::READ | wire::WRITE => {
                let n = usize::from(e[wire::NAME_LEN]);
                let len = usize::from(e[wire::DATA_LEN]);
                shape(
                    (1..=48).contains(&n)
                        && len <= 64
                        && e[98..104] == [0; 6]
                        && e[wire::NAME + n..wire::DATA].iter().all(|&v| v == 0),
                )?;
                let off = wire::get(e, wire::OFFSET);
                let nm = core::str::from_utf8(&e[wire::NAME..wire::NAME + n])
                    .map_err(|_| rt::BAD_ARGUMENT)?;
                if op == wire::WRITE {
                    s.write(fs, &key, g, nm, off, &e[wire::DATA..wire::DATA + len])
                        .map_err(status)?;
                } else {
                    let b = s.read(fs, &key, g, nm, off, len).map_err(status)?;
                    e[wire::DATA..].fill(0);
                    e[wire::DATA..wire::DATA + b.len()].copy_from_slice(&b);
                    e[wire::DATA_LEN] = b.len() as u8;
                }
            }
            wire::PURGE => {
                if rt::config(13) == 80 {
                    s.begin_purge(fs, &key, g).map_err(status)?;
                    crate::qualification::stop(b"CACHE-P4 purge-intention-durable")
                }
                s.purge(fs, &key, g).map_err(status)?;
            }
            wire::PURGE_USER | wire::PURGE_SYSTEM => {
                let user = (op == wire::PURGE_USER).then(|| key[..16].try_into().unwrap());
                let n = s.purge_scope(fs, user).map_err(status)?;
                wire::put(e, wire::DATA, n);
            }
            wire::PRESSURE => {
                let (candidate, g) = s.pressure(fs).map_err(status)?.ok_or(rt::WOULD_BLOCK)?;
                e[..32].copy_from_slice(&candidate);
                wire::put(e, wire::GEN, g);
            }
            wire::AUTO_PURGE => {
                if !s.auto_purge(fs, &key, g).map_err(status)? {
                    return Err(rt::WOULD_BLOCK);
                }
            }
            _ => return Err(rt::BAD_ARGUMENT),
        }
        Ok(())
    }
    pub(super) fn serve_cache(&mut self, rx: u64) -> rt::Result<bool> {
        if !self.temporary.enabled {
            return Ok(false);
        }
        for _ in 0..8 {
            let mut e = [0; wire::FRAME];
            let ticket = match rt::value(rt::call(
                wire::CONTROL,
                wire::NEXT,
                rx,
                e.as_mut_ptr() as u64,
            )) {
                Err(rt::WOULD_BLOCK | rt::DENIED | rt::UNKNOWN_CALL) => return Ok(false),
                result => result?,
            };
            let race = rt::config(13) == 78
                && wire::get(&e, wire::OP) == wire::WRITE
                && e[wire::NAME_LEN] == 6
                && &e[wire::NAME..wire::NAME + 6] == b"course";
            let result = (|| {
                if race {
                    let start = rt::value(rt::call(rt::CLOCK_UTC, 0, 0, 0))?;
                    let mut ready = false;
                    for _ in 0..2_000_000 {
                        match rt::value(rt::call(
                            wire::CONTROL,
                            wire::TEST_PURGE_PENDING,
                            ticket,
                            0,
                        )) {
                            Ok(_) => {
                                ready = true;
                                break;
                            }
                            Err(rt::WOULD_BLOCK) => (),
                            Err(x) => return Err(x),
                        }
                        let now = rt::value(rt::call(rt::CLOCK_UTC, 0, 0, 0))?;
                        if now.saturating_sub(start) >= 30 {
                            break;
                        }
                        rt::call(rt::THREAD_YIELD, 0, 0, 0);
                    }
                    if !ready {
                        return Err(rt::TIMED_OUT);
                    }
                }
                self.cache_operation(&mut e)
            })();
            let result = result.err().unwrap_or(rt::OK);
            if matches!(result, w::UNCERTAIN | w::IO | w::CORRUPT) {
                self.failed = Some(result)
            }
            wire::put(&mut e, wire::STATUS, result);
            match rt::value(rt::call(
                wire::CONTROL,
                wire::COMPLETE,
                ticket,
                e.as_ptr() as u64,
            )) {
                Ok(_) | Err(rt::REVOKED) => (),
                Err(x) => return Err(x),
            }
        }
        Ok(true)
    }
}
