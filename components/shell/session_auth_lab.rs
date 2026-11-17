//! Inclus exclusivement dans le profil QEMU de lancement authentifié.
use super::rt;

#[derive(Default)]
pub(super) struct Login {
    tag: u64,
    session: Option<[u8; 32]>,
    receipt: Option<[u8; 16]>,
    identity: [u8; 32],
}

impl Login {
    fn request(&mut self, op: &[u8; 8], value: &[u8]) -> rt::Result<[u8; 128]> {
        let (tx, rx) = (rt::config(16), rt::config(17));
        if tx == 0 || rx == 0 {
            return Err(rt::DENIED);
        }
        self.tag = self.tag.checked_add(1).ok_or(rt::LIMIT)?;
        let mut data = [0; 128];
        data[..8].copy_from_slice(op);
        data[8..8 + value.len()].copy_from_slice(value);
        rt::send_for(tx, self.tag, &data, 5000)?;
        let unconfirmed = |reason| {
            if op == b"NXPKEND1" {
                nova_cli::protocol::UNCERTAIN
            } else {
                reason
            }
        };
        let r = rt::receive_for(rx, 5000).map_err(unconfirmed)?;
        if r.kind != 1 || r.tag != self.tag || r.len != 128 {
            return Err(unconfirmed(rt::REFUSED));
        }
        let code = u64::from_le_bytes(r.payload[..8].try_into().unwrap());
        if code != rt::OK {
            return Err(code);
        }
        Ok(r.payload)
    }

    /// Le transport de recette reçoit exactement deux chiffres hexadécimaux par octet.
    /// La liaison ne transporte que des données de paquet. La décision reste dans le provisionneur.
    pub(super) fn install(&mut self, args: &[&str]) -> rt::Result<u64> {
        if let ["--inspect", leaf] = args {
            let n = leaf.len();
            if n == 0 || n > 39 {
                return Err(rt::BAD_ARGUMENT);
            }
            let mut value = [0; 72];
            value[..32].copy_from_slice(&self.session.ok_or(rt::DENIED)?);
            value[32] = n as u8;
            value[33..33 + n].copy_from_slice(leaf.as_bytes());
            let answer = self.request(b"NXPKVIEW", &value);
            return match answer {
                Ok(answer) => {
                    super::console::write(
                        super::tty::OUTPUT,
                        &alloc::format!(
                            "NXPK objet {leaf} {} {}\n",
                            hex(&answer[8..24]),
                            hex(&answer[24..56])
                        ),
                    )?;
                    Ok(0)
                }
                Err(rt::DENIED) => Ok(126),
                Err(e) => Err(e),
            };
        }
        if matches!(args, ["--receipt"] | ["--receipt", _]) {
            let request = if let ["--receipt", raw] = args {
                let mut id = [0u8; 16];
                if raw.len() != id.len() * 2 || !raw.is_ascii() {
                    return Err(rt::BAD_ARGUMENT);
                }
                for (out, pair) in id.iter_mut().zip(raw.as_bytes().chunks_exact(2)) {
                    *out = u8::from_str_radix(core::str::from_utf8(pair).unwrap(), 16)
                        .map_err(|_| rt::BAD_ARGUMENT)?;
                }
                id
            } else {
                self.receipt.ok_or(rt::DENIED)?
            };
            let mut value = [0; 48];
            value[..32].copy_from_slice(&self.session.ok_or(rt::DENIED)?);
            value[32..].copy_from_slice(&request);
            let answer = self.request(b"NXPKHIST", &value)?;
            let generation = u64::from_le_bytes(answer[8..16].try_into().unwrap());
            super::console::write(
                super::tty::OUTPUT,
                &alloc::format!("NXPK generation {generation}\n"),
            )?;
            return Ok(if generation == 0 { 1 } else { 0 });
        }
        let ["--hex", count] = args else {
            return Err(rt::BAD_ARGUMENT);
        };
        let size = count.parse::<u32>().map_err(|_| rt::BAD_ARGUMENT)?;
        let mut start = [0; 36];
        start[..32].copy_from_slice(&self.session.ok_or(rt::DENIED)?);
        start[32..].copy_from_slice(&size.to_le_bytes());
        let answer = self.request(b"NXPKBEG1", &start)?;
        let request: [u8; 16] = answer[16..32].try_into().unwrap();
        self.receipt = Some(request);
        super::console::write(
            super::tty::OUTPUT,
            &alloc::format!("NXPK requete {}\n", hex(&request)),
        )?;
        let mut offset = 0;
        while offset < size {
            let n = (size - offset).min(96) as usize;
            let mut payload = [0; 120];
            payload[..4].copy_from_slice(&offset.to_le_bytes());
            payload[4..6].copy_from_slice(&(n as u16).to_le_bytes());
            payload[8..24].copy_from_slice(&request);
            super::console::write(super::tty::OUTPUT, "NXPK donnees\n")?;
            for byte in &mut payload[24..24 + n] {
                *byte = digit()? << 4 | digit()?;
            }
            self.request(b"NXPKDATA", &payload)?;
            offset += n as u32;
        }
        self.request(b"NXPKEND1", &start[..32])?;
        Ok(0)
    }

    pub(super) fn ticket(&mut self, name: &str, argv: &[&str], ttl: u64) -> rt::Result<u64> {
        let id = self.session.ok_or(rt::DENIED)?;
        if name.is_empty() || name.len() > 63 || argv.len() > super::tty::ARG_COUNT {
            return Err(rt::BAD_ARGUMENT);
        }
        let mut packed = alloc::vec::Vec::new();
        packed.push(argv.len() as u8);
        for arg in argv {
            let size = u16::try_from(arg.len()).map_err(|_| rt::LIMIT)?;
            if packed.len() + 2 + arg.len() > super::tty::ARG_MAX {
                return Err(rt::LIMIT);
            }
            packed.extend_from_slice(&size.to_le_bytes());
            packed.extend_from_slice(arg.as_bytes());
        }
        let mut begin = [0; 120];
        begin[..32].copy_from_slice(&id);
        begin[32..40].copy_from_slice(&(packed.len() as u64).to_le_bytes());
        begin[40] = name.len() as u8;
        begin[41..41 + name.len()].copy_from_slice(name.as_bytes());
        self.request(b"NSHEXBG1", &begin)?;
        for (i, part) in packed.chunks(79).enumerate() {
            let mut q = [0; 120];
            q[..32].copy_from_slice(&id);
            q[32..40].copy_from_slice(&((i * 79) as u64).to_le_bytes());
            q[40] = part.len() as u8;
            q[41..41 + part.len()].copy_from_slice(part);
            self.request(b"NSHEXARG", &q)?;
        }
        let mut q = [0; 40];
        q[..32].copy_from_slice(&id);
        q[32..].copy_from_slice(&ttl.to_le_bytes());
        let answer = self.request(b"NSHEXEND", &q)?;
        Ok(u64::from_le_bytes(answer[8..16].try_into().unwrap()))
    }
    fn receipt(key: u64) -> rt::Result<(u64, u64)> {
        let mut q = rt::SendV1 {
            version: rt::VERSION,
            size: 160,
            timeout: 0,
            tag: 1,
            len: 8,
            flags: 0,
            payload: [0; 128],
        };
        q.payload[..8].copy_from_slice(b"NXQUERY1");
        rt::value(rt::call(rt::IPC_SEND, key, (&raw const q) as u64, 160))?;
        let a = rt::receive_for(key, 250)?;
        if a.tag != 1 || a.kind != 1 || a.len != 32 || &a.payload[..8] != b"NXREPLY1" {
            return Err(rt::REFUSED);
        }
        let code = u64::from_le_bytes(a.payload[8..16].try_into().unwrap());
        if code != 0 {
            return Err(code);
        }
        Ok((
            u64::from_le_bytes(a.payload[16..24].try_into().unwrap()),
            u64::from_le_bytes(a.payload[24..32].try_into().unwrap()),
        ))
    }
    pub(super) fn ticket_probe(&mut self, args: &[&str]) -> rt::Result<u64> {
        let [mode, name, rest @ ..] = args else {
            return Err(rt::BAD_ARGUMENT);
        };
        let key = self.ticket(name, rest, 5000)?;
        let result = (|| {
            if *mode == "expire" {
                // Attendre sur la voie de réponse privée inactive. Les délais IPC sont
                // exprimés en ticks à 250 Hz, le contrat du ticket en millisecondes.
                super::console::write(
                    super::tty::OUTPUT,
                    "NXEXEC ticket remis avant expiration\n",
                )?;
                if rt::receive_for(rt::config(17), 1260).err() != Some(rt::TIMED_OUT) {
                    return Err(rt::REFUSED);
                }
                if rt::call(super::tty::SPAWN, super::tty::EXEC_TICKET, key, 0).status
                    != rt::TIMED_OUT
                {
                    return Err(rt::REFUSED);
                }
                super::console::write(super::tty::OUTPUT, "NXEXEC ticket expire, aucun enfant\n")?;
                return Ok(0);
            }
            if *mode == "revoque" {
                self.close()?;
                if !matches!(
                    rt::call(super::tty::SPAWN, super::tty::EXEC_TICKET, key, 0).status,
                    rt::REVOKED | rt::DENIED
                ) {
                    return Err(rt::REFUSED);
                }
                super::console::write(super::tty::OUTPUT, "NXEXEC ticket revoque, aucun enfant\n")?;
                return Ok(0);
            }
            if *mode != "recu" {
                return Err(rt::BAD_ARGUMENT);
            }
            let launch = rt::call(super::tty::SPAWN, super::tty::EXEC_TICKET, key, 0);
            if launch.status != rt::OK {
                return Err(launch.status);
            }
            // Oublier délibérément la valeur de retour et retrouver l'enfant sans le relancer.
            let (pid, right) = Self::receipt(key)?;
            if right == 0
                || pid == 0
                || rt::call(super::tty::SPAWN, super::tty::EXEC_TICKET, key, 0).status
                    != rt::REFUSED
            {
                return Err(rt::REFUSED);
            }
            let (fault, code) = nova_cli::process::wait(right)?;
            if fault || Self::receipt(key)? != (pid, 0) || nova_cli::process::wait(right).is_ok() {
                return Err(rt::REFUSED);
            }
            super::console::write(
                super::tty::OUTPUT,
                &alloc::format!("NXEXEC pid={pid} statut={code} collecte unique\n"),
            )?;
            Ok(code)
        })();
        let _ = rt::call(rt::IPC_CLOSE, key, 0, 0);
        result
    }
    pub(super) fn exercise_relay(&mut self, argv: &[&str]) -> rt::Result<u64> {
        if !argv.is_empty() {
            return Err(rt::BAD_ARGUMENT);
        }
        let id = self.session.ok_or(rt::DENIED)?;
        let prep = self.request(b"NSHRPRE1", &id)?;
        let deadline = u64::from_le_bytes(prep[8..16].try_into().unwrap());
        let control = rt::value(rt::call(super::tty::SPAWN, super::tty::PIPE_OPEN, 0, 0))?;
        let exercise = (|| {
            let mut ends = [0; 2];
            for (i, end) in ends.iter_mut().enumerate() {
                let mut q = [0; 128];
                q[..8].copy_from_slice(b"NXRSTAG1");
                q[8..40].copy_from_slice(&id);
                q[40..72].copy_from_slice(&self.identity);
                q[72..80].copy_from_slice(&deadline.to_le_bytes());
                q[80..88].copy_from_slice(&(2 + i as u64).to_le_bytes());
                q[88..96].copy_from_slice(&control.to_le_bytes());
                q[96..128].copy_from_slice(&self.identity);
                if self.request(b"NSHROFR1", &q[8..]) != Err(rt::DENIED) {
                    return Err(rt::REFUSED);
                }
                let mut invalid = q;
                invalid[96] ^= 1;
                if rt::send_for(control, 1, &invalid, 250) != Err(rt::DENIED) {
                    return Err(rt::REFUSED);
                }
                invalid = q;
                invalid[72..80].fill(0);
                if rt::send_for(control, 1, &invalid, 250) != Err(rt::TIMED_OUT) {
                    return Err(rt::REFUSED);
                }
                rt::send_for(control, 1, &q, 250)?;
                // Les deux opérations restent en concurrence pour le même emplacement.
                // La seconde demande l'autre extrémité, avec la même autorité valide.
                let mut competing = q;
                competing[80..88].copy_from_slice(&(3 - i as u64).to_le_bytes());
                if rt::send_for(control, 2, &competing, 250) != Err(rt::WOULD_BLOCK) {
                    return Err(rt::REFUSED);
                }
                let ack = self.request(b"NSHROFR1", &q[8..])?;
                *end = u64::from_le_bytes(ack[8..16].try_into().unwrap());
                if rt::call(rt::IPC_DUP, *end, rt::SEND.into(), 0).status != rt::DENIED {
                    return Err(rt::REFUSED);
                }
            }
            nova_cli::process::pipe_write(ends[0], b"flux brut").map_err(|(_, e)| e)?;
            let msg = rt::receive_for(ends[1], 250)?;
            if msg.tag != 0 || msg.len != 9 || &msg.payload[..9] != b"flux brut" {
                return Err(rt::REFUSED);
            }
            self.close()?;
            if nova_cli::process::pipe_write(ends[0], b"x") != Err((0, rt::DENIED))
                || rt::receive_for(ends[1], 0).err() != Some(rt::DENIED)
            {
                return Err(rt::REFUSED);
            }
            super::console::write(
                super::tty::OUTPUT,
                "NXRELAY flux exact, retrait de session et plafonds verifies\n",
            )?;
            Ok(0)
        })();
        let closed = rt::value(rt::call(rt::IPC_CLOSE, control, 0, 0));
        exercise.and_then(|value| closed.map(|_| value))
    }

    pub(super) fn authenticate(&mut self) -> rt::Result<()> {
        if self.session.is_none() {
            let request = u128::from(self.tag.checked_add(1).ok_or(rt::LIMIT)?).to_le_bytes();
            let reply = self.request(b"NSHOPEN1", &request)?;
            let id = reply[8..40].try_into().unwrap();
            if id == [0; 32] {
                return Err(rt::REFUSED);
            }
            self.identity.copy_from_slice(&reply[40..72]);
            self.session = Some(id);
        }
        let owner = self.request(b"NSHLIVE1", &self.session.ok_or(rt::DENIED)?)?;
        if owner[8..24] == [0; 16] {
            return Err(rt::DENIED);
        }
        Ok(())
    }

    pub(super) fn close(&mut self) -> rt::Result<()> {
        if let Some(id) = self.session.take() {
            self.request(b"NSHCLOS1", &id)?;
        }
        Ok(())
    }
}

fn digit() -> rt::Result<u8> {
    let b = rt::value(rt::call(
        super::tty::CONSOLE,
        super::tty::INPUT.into(),
        0,
        0,
    ))? as u8;
    match b {
        b'0'..=b'9' => Ok(b - b'0'),
        b'a'..=b'f' => Ok(b - b'a' + 10),
        b'A'..=b'F' => Ok(b - b'A' + 10),
        _ => Err(rt::BAD_ARGUMENT),
    }
}

fn hex(bytes: &[u8]) -> alloc::string::String {
    use core::fmt::Write;
    let mut result = alloc::string::String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(result, "{byte:02x}").unwrap();
    }
    result
}
