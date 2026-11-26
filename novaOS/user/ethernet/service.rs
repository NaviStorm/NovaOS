//! Service Ethernet de la fixture. Ses seuls interlocuteurs sont les deux
//! canaux du lanceur. Le protocole backend transporte des trames Ethernet.
#![no_std]
#![no_main]
#[allow(dead_code, unused_attributes)]
#[path = "../../tests/ethernet_v1/lib.rs"]
mod ethernet;
#[allow(dead_code)]
#[path = "../storage/runtime.rs"]
mod runtime;
pub use ethernet::wire;
mod backend;
mod canal;
use canal::*;
use ethernet::{
    Binding, Stage, Ticket,
    protocol::{Endpoint, failure},
};
use runtime::*;
const STATION: [u8; 6] = [0x52, 0x54, 0, 0x12, 0x34, 0x56];
const PAIR: [u8; 6] = [2, 0x4e, 0x56, 0, 0, 1];
fn eth<T>(x: core::result::Result<T, ethernet::Error>) -> Result<T> {
    x.map_err(|_| REFUSED)
}
struct Service {
    api: Endpoint,
    peer: Binding,
    client: Canal,
    io: Canal,
    active: [Option<Ticket>; backend::CREDITS],
    suffix: [Option<Ticket>; 8],
    ns: usize,
    ouverts: bool,
    perdu: bool,
    en_publication: usize,
    dernier: u64,
    admission: Option<(u64, u16)>,
    publications: u64,
}
impl Service {
    fn cmd(&self, op: u64, arg: u64) -> Result<u64> {
        self.io.envoyer(DRIVER, &pair(op, arg))?;
        let (s, v) = fields(&self.io.recevoir(DRIVER)?)?;
        if s == OK { Ok(v) } else { Err(s) }
    }
    fn publier(&mut self, n: usize) -> Result<()> {
        if n == 0 {
            return Ok(());
        }
        self.en_publication = n;
        require(self.cmd(backend::PUBLIER, n as u64)? == n as u64)?;
        let p = eth(self.api.port())?;
        for t in self.suffix[..n].iter().flatten() {
            eth(p
                .backend()
                .transition(*t, Stage::Publishing, Stage::Published))?;
        }
        self.en_publication = 0;
        self.suffix.copy_within(n..self.ns, 0);
        self.ns -= n;
        self.suffix[self.ns..].fill(None);
        self.publications += 1;
        Ok(())
    }
    fn preparer(&mut self, h: wire::Header) -> Result<()> {
        self.admission = Some((h.request, h.count));
        self.publier(self.ns)?;
        for i in 0..h.count as usize {
            let t = Ticket {
                binding: self.peer,
                request: h.request,
                index: i as u8,
            };
            let p = eth(self.api.port())?;
            let mut backend = p.backend();
            eth(backend.transition(t, Stage::Admitted, Stage::Publishing))?;
            let src = eth(backend.image(t))?;
            let mut frame = [0; FRAME];
            let n = src.len();
            frame[..n].copy_from_slice(src);
            let coupon = self
                .active
                .iter()
                .position(Option::is_none)
                .ok_or(WOULD_BLOCK)?;
            self.io.envoyer(
                DRIVER,
                &pair(backend::PREPARER, ((coupon as u64) << 32) | n as u64),
            )?;
            self.io.donnees(true, 1, &mut frame[..n])?;
            let (s, id) = fields(&self.io.recevoir(DRIVER)?)?;
            if s != OK {
                return Err(s);
            }
            require(id == coupon as u64)?;
            self.active[id as usize] = Some(t);
            self.suffix[self.ns] = Some(t);
            self.ns += 1;
        }
        let n = if (cfg(16) != 0 && h.request == 10) || matches!(cfg(16), 3 | 4 | 6) {
            self.ns.div_ceil(2)
        } else {
            self.ns
        };
        self.publier(n)?;
        self.dernier = h.request;
        self.admission = None;
        Ok(())
    }
    fn completions(&mut self) -> Result<()> {
        self.publier(self.ns)?;
        for _ in 0..8 {
            if self.active.iter().all(Option::is_none) {
                break;
            }
            let id = match self.cmd(backend::COMPLETION, 0) {
                Ok(v) if v < backend::CREDITS as u64 => v as usize,
                Err(WOULD_BLOCK) => break,
                Err(e) => return Err(e),
                _ => return Err(REFUSED),
            };
            let t = self.active[id].take().ok_or(REFUSED)?;
            eth(eth(self.api.port())?
                .backend()
                .transition(t, Stage::Published, Stage::Complete))?;
        }
        Ok(())
    }
    fn reception(&mut self) -> Result<()> {
        let p = eth(self.api.port())?;
        let mut temp = [0; 128];
        let header = wire::Header {
            op: wire::RX,
            reply: false,
            count: 0,
            epoch: self.peer.epoch,
            request: 1,
        };
        let mut query = [0; 128];
        let n = ethwire(header.encode(&mut query, &[]))?;
        let _ = p;
        let sz = eth(self.api.exchange(self.peer, &query[..n], &[], &mut temp))?;
        let (known, _) = ethwire(wire::Header::decode(&temp[..sz]))?;
        if known.count == 0 {
            self.cmd(backend::ECOUTER, 0)?;
        }
        for _ in usize::from(known.count)..8 {
            let n = match self.cmd(backend::RECEPTION, 0) {
                Ok(n) => n as usize,
                Err(WOULD_BLOCK) => break,
                Err(e) => return Err(e),
            };
            let mut frame = [0; FRAME];
            let dst = frame.get_mut(..n).ok_or(REFUSED)?;
            self.io.donnees(false, 1, dst)?;
            eth(eth(self.api.port())?.backend().receive(dst))?;
        }
        Ok(())
    }
    fn notifier(&mut self) -> Result<()> {
        let pending = match eth(self.api.port())?.take_notification(self.peer) {
            Ok(v) => v,
            Err(ethernet::Error::Closed) => None,
            Err(_) => return Err(REFUSED),
        };
        if let Some((_, bits)) = pending {
            value(call(IPC_NOTIFY, self.client.tx, bits as u64, 0))?;
            mark(59, 0);
        }
        Ok(())
    }
    fn servir(&mut self) -> Result<()> {
        let (version, offres) = fields(&self.client.recevoir(EVENT)?)?;
        require(version == 1 && offres & 1 != 0 && offres & !3 == 0)?;
        self.client.boite &= offres & 2 != 0;
        self.client
            .envoyer(EVENT, &pair(1, if self.client.boite { 2 } else { 1 }))?;
        loop {
            let msg = match receive_for(self.client.rx, 1) {
                Err(TIMED_OUT) if self.ouverts => {
                    if !self.perdu
                        && self.active.iter().any(Option::is_some)
                        && self.completions().is_err()
                    {
                        self.perdre_pilote()?;
                    }
                    if self.attente_rx()? {
                        self.reception()?;
                    }
                    self.notifier()?;
                    continue;
                }
                Err(TIMED_OUT) => continue,
                v => v?,
            };
            require(msg.kind == 1 && msg.tag == CONTROL && msg.len <= 128)?;
            let raw = &msg.payload[..msg.len as usize];
            let Ok((h, body)) = wire::Header::decode(raw) else {
                continue;
            };
            if h.reply || h.epoch != self.peer.epoch {
                self.client
                    .envoyer(CONTROL, &failure(h, ethernet::Error::Stale))?;
                continue;
            }
            mark(60, h.op as u64);
            let mut frames = [[0; FRAME]; 8];
            let mut lens = [0usize; 8];
            let mut permis = true;
            if h.op == wire::TX {
                for (i, d) in body.chunks_exact(12).enumerate() {
                    let n = wire::word(d, 2) as usize;
                    lens[i] = n;
                    self.client
                        .donnees(false, i as u64 + 1, &mut frames[i][..n])?;
                    permis &= frames[i][..6] == PAIR
                        && frames[i][6..12] == STATION
                        && frames[i][12..14] == [0x88, 0xb5];
                }
            }
            let slots: [&[u8]; 8] = core::array::from_fn(|i| &frames[i][..lens[i]]);
            let mut out = [0; 128];
            if self.perdu
                && !matches!(
                    h.op,
                    wire::QUERY | wire::ACK | wire::STATS | wire::ARM | wire::CLOSE
                )
            {
                self.client
                    .envoyer(CONTROL, &failure(h, ethernet::Error::Closed))?;
                continue;
            }
            if h.op == wire::RX && self.ouverts {
                self.completions()?;
                self.reception()?;
            }
            let result = if permis {
                self.api.exchange(
                    self.peer,
                    raw,
                    if h.op == wire::TX {
                        &slots[..h.count as usize]
                    } else {
                        &[]
                    },
                    &mut out,
                )
            } else {
                Err(ethernet::Error::Denied)
            };
            let n = match result {
                Ok(n) => n,
                Err(e) => {
                    self.client.envoyer(CONTROL, &failure(h, e))?;
                    continue;
                }
            };
            if h.op == wire::OPEN && !self.ouverts {
                self.cmd(backend::OUVRIR, 0)?;
                self.ouverts = true;
            }
            if h.op == wire::CLOSE {
                let status = if self.perdu {
                    Ok(0)
                } else {
                    self.cmd(backend::FERMER, 0)
                };
                self.client.envoyer(CONTROL, &out[..n])?;
                self.notifier()?;
                mark(61, self.publications);
                // Garder le reçu et la notification sur les racines du service
                // jusqu'à consommation, ou jusqu'à l'expiration de la rétention.
                for _ in 0..8 {
                    let msg = match receive_for(self.client.rx, 250) {
                        Ok(m) => m,
                        Err(TIMED_OUT | CLOSED | REVOKED) => break,
                        Err(e) => return Err(e),
                    };
                    if msg.kind == 1
                        && msg.tag == EVENT
                        && msg.len == 8
                        && msg.payload[..8] == h.request.to_le_bytes()
                    {
                        break;
                    }
                    if msg.kind != 1 || msg.tag != CONTROL {
                        continue;
                    }
                    let req = &msg.payload[..msg.len as usize];
                    if let Ok((r, _)) = wire::Header::decode(req) {
                        let sz = match self.api.exchange(self.peer, req, &[], &mut out) {
                            Ok(k) => k,
                            Err(e) => {
                                self.client.envoyer(CONTROL, &failure(r, e))?;
                                continue;
                            }
                        };
                        self.client.envoyer(CONTROL, &out[..sz])?;
                    }
                }
                return status.map(|_| ());
            }
            if h.op == wire::ARM {
                mark(59, u64::from(out[40] == 0));
            }
            let suppress =
                matches!(cfg(16), 1 | 2) && h.op == wire::TX && h.request == 2 && self.dernier < 2;
            if !suppress {
                self.client.envoyer(CONTROL, &out[..n])?;
            }
            if h.op == wire::TX && h.request > self.dernier {
                self.cmd(backend::ECOUTER, 0)?;
                if self.preparer(h).is_err() {
                    self.perdre_pilote()?;
                }
            }
            if h.op == wire::QUERY && !self.perdu && self.completions().is_err() {
                self.perdre_pilote()?;
            }
            if h.op == wire::RX {
                for d in out[32..n].chunks_exact(12) {
                    let i = wire::word(d, 0);
                    let slot = ethernet::Slot {
                        binding: self.peer,
                        index: i,
                        generation: u32::from_le_bytes(d[4..8].try_into().unwrap()),
                    };
                    let src = eth(eth(self.api.port())?.read_rx(self.peer, slot))?;
                    let mut data = [0; FRAME];
                    data[..src.len()].copy_from_slice(src);
                    let size = src.len();
                    self.client.donnees(true, i as u64 + 1, &mut data[..size])?;
                }
            }
            if self.ouverts {
                self.notifier()?;
            }
        }
    }
    fn perdre_pilote(&mut self) -> Result<()> {
        self.perdu = true;
        let p = eth(self.api.port())?;
        for t in self.active.iter().flatten() {
            let jamais_soumis = self.suffix[self.en_publication..self.ns].contains(&Some(*t));
            if jamais_soumis {
                eth(p.backend().cancel_unpublished(*t))?;
            } else {
                eth(p.backend().incertain(*t))?;
            }
        }
        if let Some((request, n)) = self.admission.take() {
            for i in 0..n {
                let t = Ticket {
                    binding: self.peer,
                    request,
                    index: i as u8,
                };
                if !self.active.contains(&Some(t)) {
                    eth(p.backend().cancel_unpublished(t))?;
                }
            }
        }
        Ok(())
    }
    fn attente_rx(&self) -> Result<bool> {
        // La réception sans requête est activée par un ARM explicite.
        Ok(cfg(59) != 0 && !self.perdu)
    }
}
fn ethwire<T>(r: core::result::Result<T, wire::WireError>) -> Result<T> {
    r.map_err(|_| BAD_ARGUMENT)
}
fn start() -> Result<()> {
    let mut boot = [0; 16];
    boot[..8].copy_from_slice(&cfg(7).to_le_bytes());
    boot[8..].copy_from_slice(&cfg(8).to_le_bytes());
    let peer = Binding {
        boot,
        port: 1,
        owner: cfg(5),
        incarnation: cfg(6),
        channel: cfg(9),
        epoch: cfg(6),
    };
    let api = Endpoint::new(peer, u8::try_from(cfg(17)).map_err(|_| DENIED)?, 0, STATION);
    let mut s = Service {
        api,
        peer,
        client: Canal::new(cfg(2), cfg(1), cfg(1))?,
        io: Canal::new(cfg(3), cfg(4), cfg(3))?,
        active: [None; backend::CREDITS],
        suffix: [None; 8],
        ns: 0,
        ouverts: false,
        perdu: false,
        en_publication: 0,
        dernier: 0,
        admission: None,
        publications: 0,
    };
    let result = s.servir();
    if result.is_err()
        && let Ok(p) = s.api.port()
    {
        let r = p.revoke();
        mark(62, r.uncertain as u64);
        mark(63, r.cancelled as u64);
    }
    result
}
#[unsafe(no_mangle)]
extern "C" fn run() -> ! {
    exit(start().err().unwrap_or(0))
}
