struct Client {
    key: SigningKey,
    seq: u64,
}
impl Client {
    fn frame(&self, op: u8) -> [u8; 128] {
        let mut q = [0; 128];
        q[0] = op;
        q[8..40].copy_from_slice(self.key.verifying_key().as_bytes());
        q
    }
    fn send(&mut self, q: &[u8; 128]) -> rt::Result<[u8; 128]> {
        self.seq += 1;
        rpc(rt::config(7), rt::config(8), self.seq, q)
    }
    fn directory(
        &mut self,
        session: [u8; 32],
        op: u8,
        key: u64,
        rights: u8,
        request: u64,
    ) -> rt::Result<(u64, u64)> {
        let mut data = [0; 128];
        data[0] = op;
        data[1] = rights;
        data[8..16].copy_from_slice(&key.to_le_bytes());
        data[16..24].copy_from_slice(&request.to_le_bytes());
        let tag = self.seq.checked_add(1).ok_or(rt::LIMIT)?;
        rt::send_frame(rt::config(9), tag, &data, 0, false)?;
        let mut auth = self.frame(11);
        auth[40..72].copy_from_slice(&session);
        let uncertain = matches!(op, 6 | 7);
        let result = match self.send(&auth) {
            Ok(r) => r,
            Err(_) if uncertain => return Ok((files::UNCERTAIN, 0)),
            Err(e) => return Err(e),
        };
        if number(&result) != 0 {
            return Ok((number(&result), 0));
        }
        // Après l'acquittement d'attestation, seule la réponse VFS fait foi.
        let reply = rt::receive_for(rt::config(10), 250);
        match reply {
            Ok(m)
                if m.kind == 1
                    && m.len == 128
                    && m.tag == tag
                    && &m.payload[120..] == b"DIRREPLY" =>
            {
                Ok((number(&m.payload), number(&m.payload[8..])))
            }
            _ if uncertain => {
                marker(b"NA3 reponse VFS manquante issue incertaine");
                Ok((files::UNCERTAIN, 0))
            }
            Err(e) => Err(e),
            Ok(_) => Err(rt::BAD_ARGUMENT),
        }
    }
    fn challenge(&mut self) -> rt::Result<[u8; 128]> {
        let mut q = self.frame(1);
        q[40..56].copy_from_slice(&random()?[..16]);
        self.send(&q)
    }
    fn answer(&self, a: &[u8; 128]) -> [u8; 128] {
        let mut q = self.frame(2);
        q[40..48].copy_from_slice(&a[8..16]);
        q[48..112].copy_from_slice(&self.key.sign(&a[16..48]).to_bytes());
        q
    }
    fn login(&mut self) -> rt::Result<([u8; 16], [u8; 32], [u8; 128])> {
        let a = self.challenge()?;
        check(number(&a) == 0)?;
        let signed = self.answer(&a);
        let r = self.send(&signed)?;
        check(number(&r) == 0 && r[8..24] != [0; 16] && r[24..56] != [0; 32])?;
        Ok((
            r[8..24].try_into().unwrap(),
            r[24..56].try_into().unwrap(),
            signed,
        ))
    }
    fn session(&mut self, op: u8, id: [u8; 32], arg: [u8; 16]) -> rt::Result<[u8; 128]> {
        self.named_session(
            op,
            id,
            arg,
            if op == 5 {
                b"alice"
            } else if op == 8 {
                b"alice-b"
            } else {
                b""
            },
        )
    }
    fn named_session(
        &mut self,
        op: u8,
        id: [u8; 32],
        arg: [u8; 16],
        name: &[u8],
    ) -> rt::Result<[u8; 128]> {
        if name.len() > 24 {
            return Err(rt::BAD_ARGUMENT);
        }
        let mut q = self.frame(op);
        q[40..72].copy_from_slice(&id);
        q[72..88].copy_from_slice(&arg);
        if op >= 5 {
            q[88..104].copy_from_slice(&random()?[..16]);
        }
        q[104..104 + name.len()].copy_from_slice(name);
        self.send(&q)
    }
}
