//! Décodage de la voie de contrôle. Le pair provient de l'endpoint de réception.
use super::*;
use wire::{ENTRY, Header};

fn u64_at(b: &[u8], i: usize) -> u64 {
    u64::from_le_bytes(b[i..i + 8].try_into().unwrap())
}
fn field(b: &mut [u8], i: usize, v: u64) {
    b[i..i + 8].copy_from_slice(&v.to_le_bytes());
}
fn number(stage: Stage) -> u16 {
    match stage {
        Stage::Admitted => 0,
        Stage::Publishing => 1,
        Stage::Published => 2,
        Stage::Complete => 3,
        Stage::Cancelled => 4,
        Stage::Unknown => 5,
    }
}
fn receipt(out: &mut [u8], i: usize, id: u64, stage: Stage) {
    let r = &mut out[i * ENTRY..(i + 1) * ENTRY];
    r[..2].copy_from_slice(&(i as u16).to_le_bytes());
    r[2..4].copy_from_slice(&number(stage).to_le_bytes());
    field(r, 4, id);
}
pub struct Endpoint {
    binding: Binding,
    granted: u8,
    available: u64,
    station: [u8; 6],
    version: Option<Version>,
    port: Option<Port>,
    close_receipt: Option<u64>,
}
impl Endpoint {
    /// Appelé seulement après réservation d'une instance active par le lanceur.
    pub fn new(binding: Binding, granted: u8, available: u64, station: [u8; 6]) -> Self {
        Self {
            binding,
            granted,
            available,
            station,
            version: None,
            port: None,
            close_receipt: None,
        }
    }
    pub fn retirer_options(&mut self, masque: u64) -> Result<()> {
        if self.version.is_some() {
            return Err(Error::State);
        }
        self.available &= !masque;
        Ok(())
    }
    pub fn port(&mut self) -> Result<&mut Port> {
        self.port.as_mut().ok_or(Error::State)
    }
    pub fn exchange(
        &mut self,
        peer: Binding,
        request: &[u8],
        slots: &[&[u8]],
        out: &mut [u8; 128],
    ) -> Result<usize> {
        if peer != self.binding {
            return Err(Error::Denied);
        }
        let (mut h, body) = Header::decode(request).map_err(|_| Error::Invalid)?;
        if h.reply || h.epoch != peer.epoch {
            return Err(Error::Stale);
        }
        if h.op != wire::TX && !slots.is_empty() {
            return Err(Error::Invalid);
        }
        if self.port.as_ref().is_some_and(|p| p.closed) {
            if h.op == wire::CLOSE && self.close_receipt == Some(h.request) {
                h.reply = true;
                return h.encode(out, &[]).map_err(|_| Error::Invalid);
            }
            return Err(Error::Closed);
        }
        let mut answer = [0u8; 96];
        let n = match h.op {
            wire::NEGOTIATE => {
                if self.port.is_some() {
                    return Err(Error::State);
                }
                let v = negotiate(1, 0, u64_at(body, 0), u64_at(body, 8), self.available)?;
                self.version = Some(v);
                field(&mut answer, 0, v.features);
                field(&mut answer, 8, self.available & OPTIONS);
                16
            }
            wire::OPEN => {
                if body[1..] != [0; 7] {
                    return Err(Error::Invalid);
                }
                let v = self.version.ok_or(Error::State)?;
                if let Some(p) = &self.port {
                    if p.rights != body[0] {
                        return Err(Error::Conflict);
                    }
                } else {
                    self.port = Some(Port::open(peer, self.granted, body[0], v, self.station)?);
                }
                answer[..4].copy_from_slice(&[body[0], LIMIT as u8, 1, 1]);
                answer[4..6].copy_from_slice(&1500u16.to_le_bytes());
                answer[6..12].copy_from_slice(&self.station);
                16
            }
            wire::TX => {
                let port = self.port()?;
                if slots.len() != usize::from(h.count) {
                    return Err(Error::Invalid);
                }
                let empty = Frame {
                    slot: Slot {
                        binding: peer,
                        index: 0,
                        generation: 0,
                    },
                    bytes: &[],
                };
                let mut frames = [empty; LIMIT];
                for (i, d) in body.chunks_exact(ENTRY).enumerate() {
                    if slots[i].len() != usize::from(wire::word(d, 2)) {
                        return Err(Error::Invalid);
                    }
                    frames[i] = Frame {
                        slot: Slot {
                            binding: peer,
                            index: wire::word(d, 0),
                            generation: u32::from_le_bytes(d[4..8].try_into().unwrap()),
                        },
                        bytes: slots[i],
                    };
                }
                port.submit(peer, h.request, &frames[..slots.len()])?;
                for i in 0..slots.len() {
                    receipt(
                        &mut answer,
                        i,
                        h.request,
                        port.status(peer, port.ticket(h.request, i))?,
                    );
                }
                slots.len() * ENTRY
            }
            wire::QUERY => {
                let p = self.port()?;
                p.authorize(peer, TX_RIGHT)?;
                let id = u64_at(body, 0);
                let b = p.batch(id)?;
                for (i, r) in b.frames[..b.n].iter().enumerate() {
                    receipt(&mut answer, i, id, r.stage);
                }
                h.count = b.n as u16;
                b.n * ENTRY
            }
            wire::RX => {
                let p = self.port()?;
                p.authorize(peer, RX_RIGHT)?;
                for (index, s) in p.rx.iter().enumerate().filter(|(_, s)| s.len != 0) {
                    let at = usize::from(h.count) * ENTRY;
                    answer[at..at + ENTRY].copy_from_slice(&wire::descriptor(
                        index as u16,
                        s.len as u16,
                        s.generation,
                    ));
                    h.count += 1;
                }
                usize::from(h.count) * ENTRY
            }
            wire::RELEASE => {
                let p = self.port()?;
                p.authorize(peer, RX_RIGHT)?;
                let slot = |d: &[u8]| Slot {
                    binding: peer,
                    index: wire::word(d, 0),
                    generation: u32::from_le_bytes(d[4..8].try_into().unwrap()),
                };
                for d in body.chunks_exact(ENTRY) {
                    let s = slot(d);
                    if p.rx[usize::from(s.index)].generation != s.generation {
                        return Err(Error::Stale);
                    }
                }
                for d in body.chunks_exact(ENTRY) {
                    p.release(peer, slot(d))?;
                }
                h.count = 0;
                0
            }
            wire::ACK => {
                self.port()?.ack(peer, u64_at(body, 0))?;
                0
            }
            wire::ARM => {
                let seen = u64_at(body, 0);
                let p = self.port()?;
                p.arm(peer, seen)?;
                let (rev, bits) = p.events_since(peer, seen)?;
                field(&mut answer, 0, rev);
                answer[8] = bits;
                16
            }
            wire::JOIN | wire::LEAVE => {
                self.port()?
                    .multicast(peer, body.try_into().unwrap(), h.op == wire::JOIN)?;
                0
            }
            wire::LINK => {
                let (l, _) = self.port()?.observe(peer)?;
                answer[0] = match l.carrier {
                    Carrier::Unknown => 0,
                    Carrier::Down => 1,
                    Carrier::Up => 2,
                };
                answer[1] = match l.duplex {
                    Duplex::Unknown => 0,
                    Duplex::Half => 1,
                    Duplex::Full => 2,
                };
                field(&mut answer, 8, l.bps.unwrap_or(0));
                16
            }
            wire::SET_LINK => {
                if body[2..8] != [0; 6] {
                    return Err(Error::Invalid);
                }
                let carrier = match body[0] {
                    0 => Carrier::Unknown,
                    1 => Carrier::Down,
                    2 => Carrier::Up,
                    _ => return Err(Error::Invalid),
                };
                let duplex = match body[1] {
                    0 => Duplex::Unknown,
                    1 => Duplex::Half,
                    2 => Duplex::Full,
                    _ => return Err(Error::Invalid),
                };
                let speed = u64_at(body, 8);
                self.port()?.configure_link(
                    peer,
                    Link {
                        carrier,
                        duplex,
                        bps: (speed != 0).then_some(speed),
                    },
                )?;
                0
            }
            wire::STATS => {
                let (_, s) = self.port()?.observe(peer)?;
                for (i, v) in [
                    s.admitted,
                    s.published,
                    s.complete,
                    s.rx,
                    s.drops,
                    s.returned,
                    s.full,
                    s.cancelled,
                ]
                .into_iter()
                .enumerate()
                {
                    field(&mut answer, i * 8, v);
                }
                64
            }
            wire::CLOSE => {
                self.port()?.close(peer)?;
                self.close_receipt = Some(h.request);
                0
            }
            _ => return Err(Error::Invalid),
        };
        h.reply = true;
        h.encode(out, &answer[..n]).map_err(|_| Error::Invalid)
    }
}
pub fn read_failure(data: &[u8]) -> Result<(Header, Error)> {
    if data.len() != 40
        || wire::word(data, 0) != 1
        || wire::word(data, 2) != 0
        || wire::word(data, 6) != 3
        || wire::word(data, 8) != 40
        || data[10..16] != [0; 6]
        || data[33..] != [0; 7]
    {
        return Err(Error::Invalid);
    }
    let h = Header {
        op: wire::word(data, 4),
        reply: true,
        count: 0,
        epoch: u64_at(data, 16),
        request: u64_at(data, 24),
    };
    if !(1..=14).contains(&h.op) || h.epoch == 0 || h.request == 0 {
        return Err(Error::Invalid);
    }
    let cause = match data[32] {
        1 => Error::Denied,
        2 => Error::Stale,
        3 => Error::Version,
        4 => Error::Feature,
        5 => Error::Invalid,
        6 => Error::Full,
        7 => Error::Conflict,
        8 => Error::Closed,
        9 => Error::State,
        10 => Error::LinkDown,
        11 => Error::Exhausted,
        12 => Error::NotAdmitted,
        _ => return Err(Error::Invalid),
    };
    Ok((h, cause))
}
/// Enveloppe d'erreur privée distincte des statuts noyau. Jamais de données de slot.
pub fn failure(h: Header, error: Error) -> [u8; 40] {
    let mut out = [0; 40];
    out[0..2].copy_from_slice(&1u16.to_le_bytes());
    out[4..6].copy_from_slice(&h.op.to_le_bytes());
    out[6] = 3;
    out[8] = 40;
    field(&mut out, 16, h.epoch);
    field(&mut out, 24, h.request);
    out[32] = match error {
        Error::Denied => 1,
        Error::Stale => 2,
        Error::Version => 3,
        Error::Feature => 4,
        Error::Invalid => 5,
        Error::Full => 6,
        Error::Conflict => 7,
        Error::Closed => 8,
        Error::State => 9,
        Error::LinkDown => 10,
        Error::Exhausted => 11,
        Error::NotAdmitted => 12,
    };
    out
}
