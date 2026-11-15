//! Le canal de configuration relie uniquement le gestionnaire au serveur NovaFS.
//! Aucune opération de ce protocole n’est dispatchée sur un port VFS public.
use super::*;
#[path = "../../services/config_wire.rs"]
mod wire;
use wire::{get, put};

#[derive(Default)]
pub(super) struct State {
    last: u64,
    upload: Option<(u64, usize, Vec<u8>)>,
    read: Option<(u64, config_store::Record)>,
    closed: bool,
}
impl Session<'_, '_> {
    fn config_op(&mut self, q: &Frame, a: &mut Frame) -> rt::Result<()> {
        let op = get(q, 8);
        let gen_id = get(q, 24);
        let pos = usize::try_from(get(q, 32)).map_err(|_| rt::BAD_ARGUMENT)?;
        let len = usize::try_from(get(q, 40)).map_err(|_| rt::BAD_ARGUMENT)?;
        shape(get(q, 0) == wire::VERSION && get(q, 16) == 0)?;
        if self.failed.is_some() {
            return Err(rt::CLOSED);
        }
        let fs = self.fs.as_mut().ok_or(rt::CLOSED)?;
        let state = &mut self.configuration;
        if op != wire::PART {
            shape(q[wire::DATA..].iter().all(|&b| b == 0))?;
        }
        match op {
            wire::LOAD | wire::PREVIOUS => {
                shape(len == 0)?;
                if pos == 0 {
                    shape(gen_id == 0)?;
                    state.read = None;
                    state.read = config_store::load(fs, op == wire::PREVIOUS)
                        .map_err(error)?
                        .map(|r| (op, r));
                    if state.read.is_none() {
                        return Ok(());
                    }
                }
                let (saved_op, r) = state.read.as_ref().ok_or(rt::REVOKED)?;
                if op != *saved_op || (pos != 0 && gen_id != r.generation) {
                    return Err(rt::REVOKED);
                }
                shape(pos < r.data.len())?;
                let n = (r.data.len() - pos).min(wire::CHUNK);
                a[wire::DATA..wire::DATA + n].copy_from_slice(&r.data[pos..pos + n]);
                put(a, 24, r.generation);
                put(a, 32, pos as u64);
                put(a, 40, r.data.len() as u64);
                if pos + n == r.data.len() {
                    state.read = None;
                }
            }
            wire::BEGIN => {
                shape(pos == 0 && (1..=config_store::LIMIT).contains(&len))?;
                state.upload = None;
                let current = config_store::load(fs, false)
                    .map_err(error)?
                    .map_or(0, |r| r.generation);
                if gen_id != current {
                    return Err(w::BUSY);
                }
                let mut v = Vec::new();
                v.try_reserve_exact(len).map_err(|_| rt::NO_MEMORY)?;
                state.upload = Some((gen_id, len, v));
            }
            wire::PART => {
                shape(
                    (1..=wire::CHUNK).contains(&len)
                        && q[wire::DATA + len..].iter().all(|&b| b == 0),
                )?;
                let (g, size, v) = state.upload.as_mut().ok_or(rt::REFUSED)?;
                shape(
                    gen_id == *g
                        && pos == v.len()
                        && pos.checked_add(len).is_some_and(|end| end <= *size),
                )?;
                v.extend_from_slice(&q[wire::DATA..wire::DATA + len]);
            }
            wire::COMMIT => {
                shape(pos == 0 && len == 0)?;
                let (g, size, v) = state.upload.take().ok_or(rt::REFUSED)?;
                shape(gen_id == g && v.len() == size)?;
                if self.pending.is_some() || self.temporary.any_pending() {
                    return Err(w::BUSY);
                }
                if !self.writable {
                    return Err(rt::DENIED);
                }
                self.clock.sample()?;
                let fresh = config_store::save(fs, g, &v).map_err(|e| match e {
                    Error::Invalid(config_store::CONFLICT) => w::BUSY,
                    e => error(e),
                });
                if let Err(e @ (w::IO | w::UNCERTAIN)) = fresh {
                    self.failed = Some(e);
                }
                put(a, 24, fresh?);
                state.read = None;
            }
            _ => return Err(rt::BAD_ARGUMENT),
        }
        Ok(())
    }

    pub(super) fn serve_configuration(&mut self) -> rt::Result<bool> {
        let rx = rt::config(14);
        let tx = rt::config(15);
        if !self.temporary.enabled || self.configuration.closed || rx == 0 || tx == 0 {
            return Ok(false);
        }
        let msg = match rt::receive_for(rx, 0) {
            Ok(m) => m,
            Err(rt::WOULD_BLOCK) => return Ok(false),
            Err(rt::REVOKED | rt::CLOSED | rt::DENIED) => {
                self.configuration = State {
                    closed: true,
                    ..State::default()
                };
                return Ok(false);
            }
            Err(e) => return Err(e),
        };
        let op = get(&msg.payload, 8);
        let mut a = wire::request(op, 0, 0, 0);
        let status = if msg.kind != 1 || msg.len != 128 || msg.tag <= self.configuration.last {
            rt::BAD_ARGUMENT
        } else {
            self.configuration.last = msg.tag;
            self.config_op(&msg.payload, &mut a).err().unwrap_or(rt::OK)
        };
        put(&mut a, 16, status);
        match rt::send_frame(tx, msg.tag, &a, 0, false) {
            Ok(()) | Err(rt::WOULD_BLOCK | rt::CLOSED | rt::DENIED | rt::REVOKED) => Ok(true),
            Err(e) => Err(e),
        }
    }
}
