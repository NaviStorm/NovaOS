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
    groupes: bool,
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
        let mut lot = Trames::vide();
        let mut entries = [0u32; 8];
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
            let n = src.len();
            let coupon = self
                .active
                .iter()
                .position(Option::is_none)
                .ok_or(WOULD_BLOCK)?;
            if self.io.boite {
                if h.count > 1 {
                    lot.ajouter(n)?.copy_from_slice(src);
                }
                entries[i] = ((coupon as u32) << 16) | n as u32;
            } else {
                self.io.envoyer(
                    DRIVER,
                    &pair(backend::PREPARER, ((coupon as u64) << 32) | n as u64),
                )?;
                self.io.offrir(1, src)?;
                let (s, id) = fields(&self.io.recevoir(DRIVER)?)?;
                if s != OK {
                    return Err(s);
                }
                require(id == coupon as u64)?;
            }
            self.active[coupon] = Some(t);
            self.suffix[self.ns] = Some(t);
            self.ns += 1;
        }
        if self.io.boite {
            let mut msg = [0; 48];
            let size = backend::liste(
                backend::PREPARER_LOT,
                &entries[..usize::from(h.count)],
                &mut msg,
            );
            let port = eth(self.api.port())?;
            let backend = port.backend();
            let bytes = if h.count == 1 {
                eth(backend.image(self.suffix[0].ok_or(REFUSED)?))?
            } else {
                &lot.bytes[..lot.longueur()]
            };
            self.io
                .envoyer_lot(bytes, || self.io.envoyer(DRIVER, &msg[..size]))?;
            let (status, n) = fields(&self.io.recevoir(DRIVER)?)?;
            if status != OK {
                return Err(status);
            }
            require(n == u64::from(h.count))?;
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
        if self.io.boite {
            let active = self.active.iter().flatten().count();
            if active == 0 {
                return Ok(());
            }
            let mask = self.cmd(backend::COMPLETIONS, active as u64)?;
            require(mask < 256)?;
            for id in 0..8 {
                if mask & (1 << id) != 0 {
                    let t = self.active[id].take().ok_or(REFUSED)?;
                    eth(eth(self.api.port())?.backend().transition(
                        t,
                        Stage::Published,
                        Stage::Complete,
                    ))?;
                }
            }
            return Ok(());
        }
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
        let known = eth(eth(self.api.port())?.rx_occupes(self.peer))?;
        if known == 8 {
            return Ok(());
        }
        if self.io.boite {
            let arg = (8 - known) as u64 | (u64::from(known == 0) << 8);
            self.io
                .envoyer(DRIVER, &pair(backend::RECEPTION_LOT, arg))?;
            let msg = self.io.recevoir(DRIVER)?;
            require(msg.len >= 16)?;
            let n = u64::from_le_bytes(msg.payload[8..16].try_into().unwrap()) as usize;
            require(
                msg.payload[..8] == OK.to_le_bytes()
                    && n <= 8 - known
                    && msg.len as usize == 16 + 4 * n,
            )?;
            let mut lot = Trames::vide();
            for d in msg.payload[16..msg.len as usize].chunks_exact(4) {
                lot.ajouter(u32::from_le_bytes(d.try_into().unwrap()) as usize)?;
            }
            let length = lot.longueur();
            self.io.recevoir_lot(&mut lot.bytes[..length])?;
            for i in 0..n {
                eth(eth(self.api.port())?.backend().receive(lot.trame(i)))?;
            }
            return Ok(());
        }
        if known == 0 {
            self.cmd(backend::ECOUTER, 0)?;
        }
        for _ in known..8 {
            let n = match self.cmd(backend::RECEPTION, 0) {
                Ok(n) => n as usize,
                Err(WOULD_BLOCK) => break,
                Err(e) => return Err(e),
            };
            let mut frame = [0; FRAME];
            let dst = frame.get_mut(..n).ok_or(REFUSED)?;
            self.io.prendre(1, dst)?;
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
        // Répertoire privé du profil statique. Le client reçoit une description
        // Ethernet portable. Le bail matériel et les coordonnées PCI restent au pilote.
        let mut desc = [0u8; 32];
        desc[..8].copy_from_slice(&self.peer.port.to_le_bytes());
        desc[8..16].copy_from_slice(&self.peer.epoch.to_le_bytes());
        desc[16..18].copy_from_slice(&1u16.to_le_bytes());
        desc[18..20].copy_from_slice(&1500u16.to_le_bytes());
        desc[20] = cfg(17) as u8;
        desc[21] = 8;
        desc[22] = if self.client.boite { 3 } else { 1 };
        desc[24..30].copy_from_slice(&STATION);
        self.client.envoyer(CATALOGUE, &desc)?;
        let (version, offres) = fields(&self.client.recevoir(EVENT)?)?;
        require(version == 1 && offres & 1 != 0 && offres & !3 == 0)?;
        self.client.boite &= offres & 2 != 0;
        if !self.client.boite {
            eth(self.api.retirer_options(ethernet::BOITE_GROUPEE))?;
        }
        self.client
            .envoyer(EVENT, &pair(1, if self.client.boite { 2 } else { 1 }))?;
        let mut lot = Trames::vide();
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
            lot.vider();
            let mut permis = true;
            if h.op == wire::TX {
                if self.groupes {
                    let pret = self.client.recevoir(BYTES)?;
                    require(pret.len == 8 && pret.payload[..8] == h.request.to_le_bytes())?;
                }
                lot.recevoir(&self.client, body, self.groupes, true)?;
                permis = (0..usize::from(h.count)).all(|i| {
                    let f = lot.trame(i);
                    f[..6] == PAIR && f[6..12] == STATION && f[12..14] == [0x88, 0xb5]
                });
            }
            let slots: [&[u8]; 8] = core::array::from_fn(|i| {
                if h.op == wire::TX && i < usize::from(h.count) {
                    lot.trame(i)
                } else {
                    &[]
                }
            });
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
            let actualiser = h.op == wire::QUERY && self.ns == 0 && !self.perdu;
            if actualiser && self.completions().is_err() {
                self.perdre_pilote()?;
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
            if h.op == wire::NEGOTIATE {
                self.groupes = self.client.boite
                    && u64::from_le_bytes(out[32..40].try_into().unwrap()) & wire::BOITE_GROUPEE
                        != 0;
            }
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
            if !suppress && !(h.op == wire::RX && self.groupes) {
                self.client.envoyer(CONTROL, &out[..n])?;
            }
            if h.op == wire::TX && h.request > self.dernier {
                if !self.io.boite {
                    self.cmd(backend::ECOUTER, 0)?;
                }
                if self.preparer(h).is_err() {
                    self.perdre_pilote()?;
                }
            }
            if h.op == wire::QUERY && !actualiser && !self.perdu && self.completions().is_err() {
                self.perdre_pilote()?;
            }
            if h.op == wire::RX {
                lot.vider();
                for d in out[32..n].chunks_exact(12) {
                    let i = wire::word(d, 0);
                    let slot = ethernet::Slot {
                        binding: self.peer,
                        index: i,
                        generation: u32::from_le_bytes(d[4..8].try_into().unwrap()),
                    };
                    let src = eth(eth(self.api.port())?.read_rx(self.peer, slot))?;
                    if self.groupes && n == 44 {
                        self.client
                            .envoyer_lot(src, || self.client.envoyer(CONTROL, &out[..n]))?;
                        continue;
                    }
                    if self.groupes {
                        lot.ajouter(src.len())?.copy_from_slice(src);
                        continue;
                    }
                    self.client.offrir(i as u64 + 1, src)?;
                }
                if self.groupes && n != 44 {
                    self.client.envoyer_lot(&lot.bytes[..lot.longueur()], || {
                        self.client.envoyer(CONTROL, &out[..n])
                    })?;
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
        incarnation: cfg(18),
        channel: cfg(9),
        epoch: cfg(6),
    };
    let api = Endpoint::new(
        peer,
        u8::try_from(cfg(17)).map_err(|_| DENIED)?,
        ethernet::BOITE_GROUPEE,
        STATION,
    );
    let mut s = Service {
        api,
        peer,
        client: Canal::new(cfg(2), cfg(1), cfg(1))?,
        io: Canal::new(cfg(3), cfg(4), cfg(3))?,
        active: [None; backend::CREDITS],
        suffix: [None; 8],
        ns: 0,
        ouverts: false,
        groupes: false,
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
