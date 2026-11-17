//! La route vient du lanceur. Aucun champ reçu de nsh ne sélectionne un signataire ou un PID.
use super::*;
use novaos_auth_session::sessions::Sessions;

struct Login {
    route: Route,
    verifier: Verifier,
    sessions: Sessions,
    signer_seq: u64,
    store_seq: u64,
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
        let (token, digest) = self
            .verifier
            .begin(selected, self.route, request, *random()?, now()?, TTL_MS)
            .map_err(|_| rt::REFUSED)?;
        let signed = self.signer(b"NSHSIG01", digest)?;
        let current = self.current(reference)?;
        let proof = self
            .verifier
            .finish(
                token,
                self.route,
                now()?,
                Some(current),
                &factor,
                signed[8..72].try_into().unwrap(),
            )
            .map_err(|_| rt::DENIED)?;
        let current = self.current(reference)?;
        let id = self
            .sessions
            .open(proof, current, now()?, 60_000)
            .map_err(|_| rt::REFUSED)?;
        self.live = Some((id, reference));
        Ok(id)
    }
    fn inspect(&mut self, id: [u8; 32]) -> rt::Result<[u8; 16]> {
        let reference = self
            .live
            .filter(|(active, _)| active == &id)
            .ok_or(rt::DENIED)?
            .1;
        let current = self.current(reference)?;
        self.sessions
            .inspect(id, self.route, Some(current), now()?)
            .map_err(|_| rt::DENIED)
    }
    fn forward(&mut self, msg: &[u8; 128], reply: &mut [u8; 128]) -> rt::Result<()> {
        let mut q = [0; 128];
        match &msg[..8] {
            b"NXPKDATA" => q.copy_from_slice(msg),
            b"NXPKBEG1" | b"NXPKEND1" | b"NXPKVIEW" | b"NXPKHIST" => {
                let sid: [u8; 32] = msg[8..40].try_into().unwrap();
                let subject = self.inspect(sid)?;
                let current = self.current(self.live.ok_or(rt::DENIED)?.1)?;
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
                }
                b"NSHLIVE1" if m.payload[40..].iter().all(|b| *b == 0) => {
                    let owner = login.inspect(m.payload[8..40].try_into().unwrap())?;
                    reply[8..24].copy_from_slice(&owner);
                }
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
