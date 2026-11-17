//! Inclus exclusivement dans le profil QEMU de lancement authentifié.
use super::rt;

#[derive(Default)]
pub(super) struct Login {
    tag: u64,
    session: Option<[u8; 32]>,
    receipt: Option<[u8; 16]>,
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

    pub(super) fn authenticate(&mut self) -> rt::Result<()> {
        if self.session.is_none() {
            let request = u128::from(self.tag.checked_add(1).ok_or(rt::LIMIT)?).to_le_bytes();
            let reply = self.request(b"NSHOPEN1", &request)?;
            let id = reply[8..40].try_into().unwrap();
            if id == [0; 32] {
                return Err(rt::REFUSED);
            }
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
