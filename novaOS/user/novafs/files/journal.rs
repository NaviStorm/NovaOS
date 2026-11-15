//! Canal privé renouvelé lors du redémarrage du service journal.
use super::*;
use journal_store::{Journal, wire};
use wire::{get, put};

#[derive(Default)]
pub(super) struct State {
    pair: (u64, u64),
    last: u64,
}

fn pair() -> (u64, u64) {
    (rt::config(16), rt::config(17))
}

impl Session<'_, '_> {
    fn journal_op(&mut self, q: &Frame, a: &mut Frame) -> rt::Result<()> {
        let op = get(q, 1);
        let arg = get(q, 2);
        let value = get(q, 3);
        shape(get(q, 0) == wire::MAGIC)?;
        if op != wire::APPEND {
            shape(q[32..].iter().all(|&b| b == 0))?;
        }
        if self.failed.is_some() {
            return Err(rt::CLOSED);
        }
        let fs = self.fs.as_mut().ok_or(rt::CLOSED)?;
        let j = Journal::load(fs).map_err(error)?;
        match op {
            wire::STATUS => {
                shape(arg == 0 && value == 0)?;
                let s = j.status();
                for (i, n) in [
                    s.first,
                    s.next,
                    s.segments,
                    s.retention,
                    s.revision,
                    s.quota,
                    s.bytes,
                ]
                .iter()
                .enumerate()
                {
                    put(a, 4 + i, *n);
                }
            }
            wire::READ => {
                shape(value == 0)?;
                a[32..].copy_from_slice(&j.read(arg).map_err(error)?);
            }
            wire::APPEND | wire::ROTATE | wire::RETAIN | wire::PURGE => {
                if !self.writable {
                    return Err(rt::DENIED);
                }
                if self.pending.is_some() || self.temporary.any_pending() {
                    return Err(w::BUSY);
                }
                self.clock.sample()?;
                let result = if op == wire::APPEND {
                    shape(value == 0)?;
                    j.append(fs, arg, q[32..].try_into().unwrap())
                } else {
                    j.administer(fs, op, arg, value)
                };
                put(a, 3, result.map_err(error)?);
            }
            _ => return Err(rt::BAD_ARGUMENT),
        }
        Ok(())
    }

    pub(super) fn serve_journal(&mut self) -> rt::Result<bool> {
        if !self.temporary.enabled {
            return Ok(false);
        }
        let ports = pair();
        if ports != self.journal.pair {
            self.journal = State {
                pair: ports,
                last: 0,
            };
        }
        if ports.0 == 0 || ports.1 == 0 {
            return Ok(false);
        }
        let msg = match rt::receive_for(ports.0, 0) {
            Ok(m) => m,
            Err(rt::WOULD_BLOCK | rt::REVOKED | rt::CLOSED | rt::DENIED) => return Ok(false),
            Err(e) => return Err(e),
        };
        if pair() != ports {
            return Ok(true);
        }
        let op = get(&msg.payload, 1);
        let mut a = wire::header(op, 0, 0);
        let status = if msg.kind != 1 || msg.len != 128 || msg.tag <= self.journal.last {
            rt::BAD_ARGUMENT
        } else {
            self.journal.last = msg.tag;
            self.journal_op(&msg.payload, &mut a)
                .err()
                .unwrap_or(rt::OK)
        };
        put(&mut a, 2, status);
        // Le handle capturé appartient exclusivement à l'incarnation ayant émis cette demande.
        match rt::send_frame(ports.1, msg.tag, &a, 0, false) {
            Ok(()) | Err(rt::WOULD_BLOCK | rt::REVOKED | rt::CLOSED | rt::DENIED) => Ok(true),
            Err(e) => Err(e),
        }
    }
}
