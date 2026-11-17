//! La route vient du lanceur. Aucun champ reçu de nsh ne sélectionne un signataire ou un PID.
use super::*;
use novaos_auth_session::sessions::Sessions;

struct Login {
    route: Route,
    verifier: Verifier,
    sessions: Sessions,
    signer_seq: u64,
    store_seq: u64,
    relay_seq: u64,
    relay_deadline: u64,
    relay_bound: bool,
    admission: Option<([u8; 16], [u8; 32], [u8; 32])>,
    live: Option<([u8; 32], [u8; 16])>,
}

impl Login {
    fn from_launcher() -> Self {
        Self {
            route: Route {
                client: identity(11),
                issuer: identity(7),
            },
            verifier: Verifier::new(),
            sessions: Sessions::new(),
            signer_seq: 0,
            store_seq: 0,
            relay_seq: 0,
            relay_deadline: 0,
            relay_bound: false,
            admission: None,
            live: None,
        }
    }
    fn signer(&mut self, op: &[u8; 8], challenge: [u8; 32]) -> rt::Result<[u8; 128]> {
        let mut q = [0; 128];
        q[..8].copy_from_slice(op);
        q[8..40].copy_from_slice(&self.route.client);
        q[40..72].copy_from_slice(&challenge);
        self.signer_seq = self.signer_seq.checked_add(1).ok_or(rt::LIMIT)?;
        let r = rpc(rt::config(17), rt::config(18), self.signer_seq, &q)?;
        if number(&r) != rt::OK {
            return Err(number(&r));
        }
        Ok(r)
    }
    fn current(&mut self, reference: [u8; 16]) -> rt::Result<Binding> {
        self.store_seq = self.store_seq.checked_add(1).ok_or(rt::LIMIT)?;
        binding(self.store_seq, reference)
    }
    fn open(&mut self, request: [u8; 16]) -> rt::Result<[u8; 32]> {
        if self.live.is_some() {
            return Err(rt::REFUSED);
        }
        let r = self.signer(b"NSHPUB01", [0; 32])?;
        let factor = Possession(
            VerifyingKey::from_bytes(r[8..40].try_into().unwrap()).map_err(|_| rt::DENIED)?,
        );
        let reference = Possession::reference_bytes(DOMAIN, factor.0.as_bytes());
        let selected = self.current(reference)?;
        let issued_at = now()?;
        let (token, digest) = self
            .verifier
            .begin(selected, self.route, request, *random()?, issued_at, TTL_MS)
            .map_err(|_| rt::REFUSED)?;
        let signed = self.signer(b"NSHSIG01", digest)?;
        let current = self.current(reference)?;
        let received_at = now()?;
        let proof = self
            .verifier
            .finish(
                token,
                self.route,
                received_at,
                Some(current),
                &factor,
                signed[8..72].try_into().unwrap(),
            )
            .map_err(|_| {
                marker(
                    if received_at >= issued_at.saturating_add(u64::from(TTL_MS)) {
                        b"NXLOGIN refus defi expire"
                    } else {
                        b"NXLOGIN refus preuve ou liaison"
                    },
                );
                rt::DENIED
            })?;
        let current = self.current(reference)?;
        let session_start = now()?;
        let id = self
            .sessions
            .open(proof, current, session_start, 60_000)
            .map_err(|_| rt::REFUSED)?;
        self.live = Some((id, reference));
        self.relay_deadline = session_start.checked_add(60_000).ok_or(rt::LIMIT)?;
        self.relay_bound = false;
        // Le gestionnaire peut ne pas avoir encore démarré le courtier.
        // L’authentification des commandes ordinaires garde son indépendance.
        let _ = self.bind_relay(id);
        Ok(id)
    }
    fn relay_control(&mut self, op: &[u8; 8], sid: [u8; 32], deadline: u64) -> rt::Result<()> {
        let port = rt::config(19);
        if port == 0 {
            return Err(rt::CLOSED);
        }
        self.relay_seq = self.relay_seq.checked_add(1).ok_or(rt::LIMIT)?;
        let mut q = [0; 128];
        q[..8].copy_from_slice(op);
        q[8..40].copy_from_slice(&sid);
        q[40..72].copy_from_slice(&self.route.client);
        q[72..80].copy_from_slice(&deadline.to_le_bytes());
        if op == b"NXRBIND1" {
            q[80..96].copy_from_slice(&self.inspect(sid)?);
        }
        rt::send_for(port, self.relay_seq, &q, 250)
    }
    fn bind_relay(&mut self, sid: [u8; 32]) -> rt::Result<()> {
        if !self.relay_bound {
            self.relay_control(b"NXRBIND1", sid, self.relay_deadline)?;
            self.relay_bound = true;
        }
        Ok(())
    }
    fn relay_offer(&mut self, msg: &[u8; 128], reply: &mut [u8; 128]) -> rt::Result<()> {
        let sid = msg[8..40].try_into().unwrap();
        self.inspect(sid)?;
        self.bind_relay(sid)?;
        let mut q = *msg;
        q[..8].copy_from_slice(b"NXROFFR1");
        q[40..72].copy_from_slice(&self.route.client);
        self.relay_seq = self.relay_seq.checked_add(1).ok_or(rt::LIMIT)?;
        let seq = self.relay_seq;
        let port = rt::config(19);
        rt::send_for(port, seq, &q, 250)?;
        rt::send_for(port, seq, b"NXRELAY1", 250)?;
        // Le mot 20 est privé au profil et publié avant le mot 19.
        let ack_port = unsafe {
            (&*(0x401000 as *const core::sync::atomic::AtomicU64).add(20))
                .load(core::sync::atomic::Ordering::Acquire)
        };
        let ack = rt::receive_for(ack_port, 500)?;
        if ack.kind != 1 || ack.tag != seq || ack.len != 16 {
            return Err(rt::REFUSED);
        }
        // L'accusé du courtier réveille le vérificateur. Le résultat faisant
        // autorité est celui mémorisé par le noyau pour le destinataire.
        let key = rt::value(rt::call(rt::IPC_NOTIFY, port, seq, 1))?;
        if number(&ack.payload) != rt::OK || number(&ack.payload[8..]) != key {
            return Err(rt::REFUSED);
        }
        reply[8..16].copy_from_slice(&key.to_le_bytes());
        Ok(())
    }
    fn admission(&mut self, msg: &[u8; 128], reply: &mut [u8; 128]) -> rt::Result<()> {
        let id: [u8; 32] = msg[8..40].try_into().unwrap();
        let current = self.inspected_binding(id)?;
        let holder = current.subject;
        self.bind_relay(id)?;
        let port = rt::config(5);
        match &msg[..8] {
            b"NSHEXBG1" => {
                if self.admission.is_some() {
                    return Err(rt::WOULD_BLOCK);
                }
                let len = number(&msg[40..48]);
                let n = msg[48] as usize;
                if n == 0
                    || n > 63
                    || msg[49 + n..112].iter().any(|b| *b != 0)
                    || msg[120..127] != [0; 7]
                    || !matches!(msg[127], 0 | 1 | 2 | 3 | 5 | 7)
                    || (msg[127] & 3 >= 2) != (number(&msg[112..120]) != 0)
                {
                    return Err(rt::BAD_ARGUMENT);
                }
                let nonce: [u8; 16] = random()?[..16].try_into().unwrap();
                let mut q = [0; 128];
                q[..8].copy_from_slice(b"NXLDOPEN");
                q[8..24].copy_from_slice(&nonce);
                q[24..56].copy_from_slice(&id);
                q[56..72].copy_from_slice(&holder);
                q[72..104].copy_from_slice(&current.head);
                q[104..112].copy_from_slice(&len.to_le_bytes());
                q[112] = msg[127];
                q[120..].copy_from_slice(&msg[112..120]);
                rt::send_for(port, 1, &q, 250)?;
                self.admission = Some((nonce, id, current.head));
                let mut f = [0; 128];
                f[..8].copy_from_slice(b"NXPKBEGX");
                f[8..24].copy_from_slice(&holder);
                f[24..56].copy_from_slice(&id);
                f[56..88].copy_from_slice(&current.head);
                f[88..104].copy_from_slice(&nonce);
                let mut name = [0; 128];
                name[..8].copy_from_slice(b"NXPKNAME");
                name[8..24].copy_from_slice(&nonce);
                name[24] = n as u8;
                name[25..25 + n].copy_from_slice(&msg[49..49 + n]);
                for request in [&f, &name] {
                    self.store_seq = self.store_seq.checked_add(1).ok_or(rt::LIMIT)?;
                    let ack = rpc(port, rt::config(6), self.store_seq, request)?;
                    if number(&ack) != rt::OK {
                        return Err(number(&ack));
                    }
                }
            }
            b"NSHEXARG" => {
                let (nonce, sid, head) = self.admission.ok_or(rt::DENIED)?;
                if id != sid || head != current.head {
                    return Err(rt::REVOKED);
                }
                let n = msg[48] as usize;
                if n == 0 || n > 79 || msg[49 + n..].iter().any(|b| *b != 0) {
                    return Err(rt::BAD_ARGUMENT);
                }
                let mut q = [0; 128];
                q[..8].copy_from_slice(b"NXLDARGV");
                q[8..24].copy_from_slice(&nonce);
                q[24..32].copy_from_slice(&msg[40..48]);
                q[32..40].copy_from_slice(&(n as u64).to_le_bytes());
                q[40..40 + n].copy_from_slice(&msg[49..49 + n]);
                rt::send_for(port, 1, &q, 250)?;
            }
            b"NSHEXEND" => {
                let (nonce, sid, head) = self.admission.ok_or(rt::DENIED)?;
                if id != sid || head != current.head {
                    return Err(rt::REVOKED);
                }
                let mut q = [0; 128];
                q[..8].copy_from_slice(b"NXLDISUE");
                q[8..24].copy_from_slice(&nonce);
                q[24..56].copy_from_slice(&head);
                q[56..88].copy_from_slice(&id);
                q[88..96].copy_from_slice(&msg[40..48]);
                let f = rt::SendV1 {
                    version: rt::VERSION,
                    size: 160,
                    timeout: 250,
                    tag: 1,
                    len: 128,
                    flags: 1,
                    payload: q,
                };
                let key = rt::value(rt::call(rt::IPC_SEND, port, (&raw const f) as u64, 160))?;
                self.admission = None;
                if key == 0 {
                    return Ok(());
                }
                self.offer_ticket(id, key, reply)?;
            }
            b"NSHEXRD1" => {
                let mut q = [0; 128];
                q[..8].copy_from_slice(b"NXLDTAKE");
                q[8..40].copy_from_slice(&id);
                q[40..72].copy_from_slice(&current.head);
                q[72..80].copy_from_slice(&msg[40..48]);
                let f = rt::SendV1 {
                    version: rt::VERSION,
                    size: 160,
                    timeout: 250,
                    tag: 1,
                    len: 128,
                    flags: 1,
                    payload: q,
                };
                let key = rt::value(rt::call(rt::IPC_SEND, port, (&raw const f) as u64, 160))?;
                self.offer_ticket(id, key, reply)?;
            }
            _ => return Err(rt::BAD_ARGUMENT),
        }
        Ok(())
    }
    fn offer_ticket(&mut self, id: [u8; 32], key: u64, reply: &mut [u8; 128]) -> rt::Result<()> {
        let mut offer = [0; 128];
        offer[..8].copy_from_slice(b"NSHROFR1");
        offer[8..40].copy_from_slice(&id);
        offer[72..80].copy_from_slice(&self.relay_deadline.to_le_bytes());
        offer[80..88].copy_from_slice(&1u64.to_le_bytes());
        offer[88..96].copy_from_slice(&key.to_le_bytes());
        offer[96..128].copy_from_slice(&self.route.client);
        self.relay_offer(&offer, reply)
    }

    fn abort_admission(&mut self) {
        if let Some((nonce, _, _)) = self.admission.take() {
            let mut q = [0; 128];
            q[..8].copy_from_slice(b"NXLDABRT");
            q[8..24].copy_from_slice(&nonce);
            let _ = rt::send_for(rt::config(5), 1, &q, 250);
        }
    }

    fn inspect(&mut self, id: [u8; 32]) -> rt::Result<[u8; 16]> {
        self.inspected_binding(id).map(|binding| binding.subject)
    }

    // L'identité vérifiée et la tête présentée à P9 proviennent de la même lecture.
    fn inspected_binding(&mut self, id: [u8; 32]) -> rt::Result<Binding> {
        let reference = self
            .live
            .filter(|(active, _)| active == &id)
            .ok_or(rt::DENIED)?
            .1;
        let current = self.current(reference)?;
        self.sessions
            .inspect(id, self.route, Some(current), now()?)
            .map_err(|_| rt::DENIED)?;
        Ok(current)
    }
    fn forward(&mut self, msg: &[u8; 128], reply: &mut [u8; 128]) -> rt::Result<()> {
        let mut q = [0; 128];
        match &msg[..8] {
            b"NXPKDATA" => q.copy_from_slice(msg),
            b"NXPKBEG1" | b"NXPKEND1" | b"NXPKVIEW" | b"NXPKHIST" => {
                let sid: [u8; 32] = msg[8..40].try_into().unwrap();
                let current = self.inspected_binding(sid)?;
                let subject = current.subject;
                q[..8].copy_from_slice(&msg[..8]);
                q[8..24].copy_from_slice(&subject);
                q[24..56].copy_from_slice(&sid);
                q[56..88].copy_from_slice(&current.head);
                if &msg[..8] == b"NXPKBEG1" {
                    let request: [u8; 16] = random()?[..16].try_into().unwrap();
                    q[88..104].copy_from_slice(&request);
                    q[104..108].copy_from_slice(&msg[40..44]);
                    reply[16..32].copy_from_slice(&request);
                } else if &msg[..8] == b"NXPKVIEW" {
                    q[88..128].copy_from_slice(&msg[40..80]);
                } else if &msg[..8] == b"NXPKHIST" {
                    q[88..104].copy_from_slice(&msg[40..56]);
                }
            }
            _ => return Err(rt::BAD_ARGUMENT),
        }
        self.store_seq = self.store_seq.checked_add(1).ok_or(rt::LIMIT)?;
        let ack = rpc(rt::config(5), rt::config(6), self.store_seq, &q).map_err(|e| {
            if &msg[..8] == b"NXPKEND1" {
                files::UNCERTAIN
            } else {
                e
            }
        })?;
        let status = number(&ack);
        if status != rt::OK {
            return Err(status);
        }
        if &msg[..8] == b"NXPKVIEW" {
            reply[8..56].copy_from_slice(&ack[8..56]);
        } else {
            reply[8..16].copy_from_slice(&ack[8..16]);
        }
        Ok(())
    }

    fn close(&mut self, id: [u8; 32]) -> rt::Result<()> {
        self.sessions
            .close(id, self.route)
            .map_err(|_| rt::DENIED)?;
        if self.relay_bound {
            match self.relay_control(b"NXRSTOP1", id, 0) {
                Ok(()) | Err(rt::CLOSED | rt::REVOKED) => (),
                Err(e) => return Err(e),
            }
        }
        self.relay_bound = false;
        self.abort_admission();
        self.live = None;
        self.store_seq = self.store_seq.checked_add(1).ok_or(rt::LIMIT)?;
        let mut drop = [0; 128];
        drop[..8].copy_from_slice(b"NXPKDROP");
        let result = rpc(rt::config(5), rt::config(6), self.store_seq, &drop)?;
        if number(&result) != 0 {
            return Err(number(&result));
        }
        Ok(())
    }
}

pub(super) fn serve() -> rt::Result<()> {
    // Le lanceur publie le mot 15 après les autres mots de la route et du signataire.
    while rt::config(15) == 0 {
        match rt::receive_for(rt::config(1), 25) {
            Err(rt::TIMED_OUT) => (),
            Ok(m) if m.kind == 1 && m.len == 1 && m.payload[0] == 0 => return Ok(()),
            _ => return Err(rt::CANCELLED),
        }
    }
    let mut login = Login::from_launcher();
    if login.route.client == [0; 32] || login.route.issuer == [0; 32] {
        return Err(rt::DENIED);
    }
    let mut last = 0;
    loop {
        let m = match rt::receive_for(rt::config(15), 1000) {
            Err(rt::TIMED_OUT) => continue,
            r => r?,
        };
        let mut reply = [0; 128];
        let result: rt::Result<()> = (|| {
            if m.kind != 1 || m.len != 128 || m.tag <= last {
                return Err(rt::BAD_ARGUMENT);
            }
            last = m.tag;
            match &m.payload[..8] {
                b"NSHOPEN1" if m.payload[24..].iter().all(|b| *b == 0) => {
                    let id = login.open(m.payload[8..24].try_into().unwrap())?;
                    reply[8..40].copy_from_slice(&id);
                    reply[40..72].copy_from_slice(&login.route.client);
                }
                b"NSHLIVE1" if m.payload[40..].iter().all(|b| *b == 0) => {
                    let owner = login.inspect(m.payload[8..40].try_into().unwrap())?;
                    reply[8..24].copy_from_slice(&owner);
                }
                b"NSHEXBG1" | b"NSHEXARG" | b"NSHEXEND" | b"NSHEXRD1" => {
                    if let Err(e) = login.admission(&m.payload, &mut reply) {
                        login.abort_admission();
                        return Err(e);
                    }
                }
                b"NSHRPRE1" => {
                    let sid = m.payload[8..40].try_into().unwrap();
                    login.inspect(sid)?;
                    login.bind_relay(sid)?;
                    let port = rt::config(19);
                    // Aucun cinquième canal, par création directe ni via le courtier.
                    if rt::call(rt::IPC_CREATE, 0, 0, 0).status != rt::LIMIT
                        || rt::call(rt::IPC_DUP, port, rt::SEND.into(), 0).status != rt::DENIED
                        || rt::send_for(port, 1, b"NXCREATE", 100) != Err(rt::DENIED)
                    {
                        return Err(rt::REFUSED);
                    }
                    reply[8..16].copy_from_slice(&login.relay_deadline.to_le_bytes());
                }
                b"NSHROFR1" => login.relay_offer(&m.payload, &mut reply)?,
                b"NSHCLOS1" if m.payload[40..].iter().all(|b| *b == 0) => {
                    login.close(m.payload[8..40].try_into().unwrap())?;
                }
                b"NXPKBEG1" | b"NXPKEND1" | b"NXPKDATA" | b"NXPKVIEW" | b"NXPKHIST" => {
                    login.forward(&m.payload, &mut reply)?
                }
                _ => return Err(rt::BAD_ARGUMENT),
            }
            Ok(())
        })();
        reply[..8].copy_from_slice(&result.err().unwrap_or(0).to_le_bytes());
        rt::send_for(rt::config(16), m.tag, &reply, 5000)?;
    }
}
