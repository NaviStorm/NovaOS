//! Inclus exclusivement dans le profil QEMU de lancement authentifié.
use super::rt;

#[derive(Default)]
pub(super) struct Login {
    tag: u64,
    session: Option<[u8; 32]>,
    receipt: Option<[u8; 16]>,
    identity: [u8; 32],
}

#[path = "redirect_lab.rs"]
mod redirect;
type Stage = (
    alloc::string::String,
    alloc::vec::Vec<alloc::string::String>,
);

impl Login {
    pub(super) fn verify_redirect(
        &mut self,
        fs: &mut nova_cli::fileapi::Client,
        argv: &[&str],
    ) -> rt::Result<u64> {
        use nova_cli::protocol as w;
        match argv {
            ["concurrence", path] => fs.probe_exclusive(path.as_bytes())?,
            ["retrait", src, out] => {
                let read = fs.open_at(0, src.as_bytes(), 0x80 | w::READABLE)?;
                let key = fs.open_at(0, out.as_bytes(), 0x80 | w::WRITABLE | w::CREATE)?;
                let mut one = [0u8; 1];
                if fs.read(read, 0, &mut one)? != 1 {
                    return Err(rt::REFUSED);
                }
                if fs.simple(w::STAT, read).err() != Some(rt::DENIED) {
                    return Err(rt::REFUSED);
                }
                let mut malformed = w::frame(w::CLOSE, fs.session, read);
                malformed[2] = 0xf0;
                if fs.request(malformed).err() != Some(rt::BAD_ARGUMENT) {
                    return Err(rt::REFUSED);
                }
                fs.write(key, 0, b"avant")?;
                fs.sync(key)?;
                fs.write(key, 5, b"en attente")?;
                self.close()?;
                if fs.read(read, 0, &mut one) != Err(rt::REVOKED)
                    || fs.sync(key) != Err(rt::REVOKED)
                    || fs.write(key, 0, b"interdit") != Err(rt::REVOKED)
                {
                    return Err(rt::REFUSED);
                }
                self.authenticate()?;
                let test = fs.open_at(0, out.as_bytes(), 0x80 | w::READABLE)?;
                let mut bytes = [0; 32];
                let n = fs.read(test, 0, &mut bytes)?;
                fs.close(test)?;
                if &bytes[..n] != b"avant" {
                    return Err(rt::REFUSED);
                }
            }
            _ => return Err(rt::BAD_ARGUMENT),
        }
        Ok(0)
    }
    pub(super) fn redirected(
        &mut self,
        fs: &mut nova_cli::fileapi::Client,
        stages: &[Stage],
        paths: &[Option<alloc::string::String>; 6],
    ) -> rt::Result<u64> {
        let mut files = redirect::Files::new(fs);
        let result = (|| {
            // Authentifier et résoudre chaque installation avant de créer un fichier.
            for (name, _) in stages {
                let mut value = [0; 72];
                value[..32].copy_from_slice(&self.session.ok_or(rt::DENIED)?);
                value[32] = name.len() as u8;
                value[33..33 + name.len()].copy_from_slice(name.as_bytes());
                self.request(b"NXPKVIEW", &value)?;
            }
            files.open(paths)?;
            match stages {
                [(name, args)] => {
                    let argv = args
                        .iter()
                        .map(|a| a.as_str())
                        .collect::<alloc::vec::Vec<_>>();
                    self.execute_io(name, &argv, Some(&mut files))
                }
                [left, right] => self.pipeline_io(left, right, Some(&mut files)),
                _ => Err(rt::BAD_ARGUMENT),
            }
        })();
        let close = files.close();
        result.and_then(|status| close.map(|()| status))
    }

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
            if option_env!("NOVA_DISPLAY_TRACE") == Some("1") && op.starts_with(b"NSHEX") {
                super::console::write(
                    super::tty::ERROR,
                    &alloc::format!(
                        "NXEXEC etape={} code={code}\n",
                        core::str::from_utf8(op).unwrap_or("?")
                    ),
                )?;
            }
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

    pub(super) fn ticket(
        &mut self,
        name: &str,
        argv: &[&str],
        ttl: u64,
        role: u8,
        control: u64,
    ) -> rt::Result<u64> {
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
        begin[104..112].copy_from_slice(&control.to_le_bytes());
        begin[119] = role;
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
    pub(super) fn execute(&mut self, name: &str, argv: &[&str]) -> rt::Result<u64> {
        self.execute_io(name, argv, None)
    }
    fn execute_io(
        &mut self,
        name: &str,
        argv: &[&str],
        io: Option<&mut redirect::Files>,
    ) -> rt::Result<u64> {
        let role = if io.as_ref().is_some_and(|io| io.has_input()) {
            5
        } else {
            1
        };
        let key = self.ticket(name, argv, 5000, role, 0)?;
        let result = (|| {
            let spawned = rt::call(super::tty::SPAWN, super::tty::EXEC_TICKET, key, 0);
            let (pid, right) = match Self::receipt(key) {
                Ok(pair) => pair,
                Err(_) if spawned.status != rt::OK => return Err(spawned.status),
                Err(err) => return Err(err),
            };
            // Le rendu peut échouer indépendamment du programme. Collecter une fois
            // l'enfant même en cas d'erreur d'affichage ou de retrait de la session.
            let rendered = self.drain(&[pid], io);
            let (fault, code) = nova_cli::process::wait(right)?;
            rendered?;
            Ok(if fault { 128 } else { code })
        })();
        let _ = rt::call(rt::IPC_CLOSE, key, 0, 0);
        result
    }

    pub(super) fn pipeline(
        &mut self,
        writer: &(
            alloc::string::String,
            alloc::vec::Vec<alloc::string::String>,
        ),
        reader: &(
            alloc::string::String,
            alloc::vec::Vec<alloc::string::String>,
        ),
    ) -> rt::Result<u64> {
        self.pipeline_io(writer, reader, None)
    }
    fn pipeline_io(
        &mut self,
        writer: &Stage,
        reader: &Stage,
        io: Option<&mut redirect::Files>,
    ) -> rt::Result<u64> {
        let role = if io.as_ref().is_some_and(|io| io.has_input()) {
            7
        } else {
            3
        };
        let control = rt::value(rt::call(super::tty::SPAWN, super::tty::PIPE_OPEN, 0, 0))?;
        let mut writer_wait = None;
        let mut reader_wait = None;
        let mut ticket = None;
        let result = (|| {
            let args = reader
                .1
                .iter()
                .map(|v| v.as_str())
                .collect::<alloc::vec::Vec<_>>();
            if self.ticket(&reader.0, &args, 5000, 2, control)? != 0 {
                return Err(rt::REFUSED);
            }
            let args = writer
                .1
                .iter()
                .map(|v| v.as_str())
                .collect::<alloc::vec::Vec<_>>();
            let key = self.ticket(&writer.0, &args, 5000, role, control)?;
            ticket = Some(key);
            let result = rt::call(super::tty::SPAWN, super::tty::EXEC_TICKET, key, 0);
            if result.status != rt::OK {
                return Err(result.status);
            }
            writer_wait = Some(result.value);
            let (producer, confirmed) = Self::receipt(key)?;
            if confirmed != result.value {
                return Err(rt::REFUSED);
            }
            rt::value(rt::call(rt::IPC_CLOSE, key, 0, 0))?;
            ticket = None;
            let mut request = [0u8; 40];
            request[..32].copy_from_slice(&self.session.ok_or(rt::REVOKED)?);
            request[32..].copy_from_slice(&control.to_le_bytes());
            let reply = self.request(b"NSHEXRD1", &request)?;
            let key = u64::from_le_bytes(reply[8..16].try_into().unwrap());
            ticket = Some(key);
            let result = rt::call(super::tty::SPAWN, super::tty::EXEC_TICKET, key, 0);
            if result.status != rt::OK {
                return Err(result.status);
            }
            reader_wait = Some(result.value);
            let (consumer, confirmed) = Self::receipt(key)?;
            if confirmed != result.value || consumer == producer {
                return Err(rt::REFUSED);
            }
            self.drain(&[producer, consumer], io)?;
            Ok((producer, consumer))
        })();
        if let Some(key) = ticket {
            let _ = rt::call(rt::IPC_CLOSE, key, 0, 0);
        }
        // La fermeture du contrôleur rend irrévocablement impossible une admission
        // en attente. Le retrait borné s'applique exclusivement à cette transaction.
        let closed = rt::value(rt::call(rt::IPC_CLOSE, control, 0, 0));
        let w = writer_wait.map(nova_cli::process::wait).transpose();
        let r = reader_wait.map(nova_cli::process::wait).transpose();
        let (producer, consumer) = result?;
        closed?;
        let status = |(fault, code)| if fault { 128 } else { code };
        let left = status(w?.ok_or(rt::REFUSED)?);
        let right = status(r?.ok_or(rt::REFUSED)?);
        if option_env!("NOVA_DISPLAY_TRACE") == Some("1") {
            super::console::write(
                super::tty::OUTPUT,
                &alloc::format!(
                    "NXPIPE producteur={producer} statut={left} lecteur={consumer} statut={right}\n"
                ),
            )?;
        }
        Ok(right)
    }

    fn drain(&mut self, pids: &[u64], mut files: Option<&mut redirect::Files>) -> rt::Result<()> {
        use super::stream_display::{Decoder, Display};
        if pids.is_empty() || pids.len() > 2 {
            return Err(rt::BAD_ARGUMENT);
        }
        let mut channels = [0; 4];
        for (stage, &pid) in pids.iter().enumerate() {
            for fd in 0..2 {
                channels[stage * 2 + fd] = rt::value(rt::call(
                    rt::IPC_NOTIFY,
                    rt::config(16),
                    pid,
                    0x4e58_4f55_5400 + fd as u64,
                ))?;
            }
        }
        let mut input = if files.as_ref().is_some_and(|f| f.has_input()) {
            rt::value(rt::call(
                rt::IPC_NOTIFY,
                rt::config(16),
                pids[0],
                0x4e58_4f55_5402,
            ))?
        } else {
            0
        };
        let mut decoders: [Decoder; 4] = core::array::from_fn(|_| Decoder::default());
        let mut display = Display::default();
        let mut failure = None;
        let mut text = alloc::string::String::with_capacity(1024);
        // Le témoin brut est activé exclusivement lors de la fabrication d'une ISO
        // de recette. Les ELF ne peuvent pas l'activer avec leurs arguments.
        let trace = option_env!("NOVA_DISPLAY_TRACE") == Some("1");
        let mut samples = [[0u8; 64]; 4];
        let mut lengths = [0usize; 4];
        while channels != [0; 4] {
            if input != 0 {
                let result = files.as_deref_mut().unwrap().feed(input);
                if !matches!(result, Ok(true)) {
                    if let Err(e) = result {
                        let reader_gone = e == rt::DENIED
                            && rt::value(rt::call(
                                rt::IPC_NOTIFY,
                                rt::config(16),
                                pids[0],
                                0x4e58_4f55_5402,
                            )) == Ok(0);
                        if !reader_gone {
                            failure.get_or_insert(e);
                        }
                    }
                    let _ = rt::call(rt::IPC_CLOSE, input, 0, 0);
                    input = 0;
                }
            }
            for i in 0..pids.len() * 2 {
                if channels[i] == 0 {
                    continue;
                }
                text.clear();
                match rt::receive_for(channels[i], 2) {
                    Ok(m)
                        if m.kind == 1
                            && m.tag == 0
                            && m.handle == 0
                            && m.aux == 0
                            && (1..=128).contains(&m.len) =>
                    {
                        let raw = &m.payload[..m.len as usize];
                        if trace {
                            let start = lengths[i].min(samples[i].len());
                            let count = raw.len().min(samples[i].len() - start);
                            samples[i][start..start + count].copy_from_slice(&raw[..count]);
                            lengths[i] = lengths[i].saturating_add(raw.len());
                        }
                        let fd = i / 2 * 3 + i % 2 + 1;
                        if let Some(f) = files.as_deref_mut().filter(|f| f.has(fd)) {
                            if failure.is_none()
                                && let Err(e) = f.write(fd, raw)
                            {
                                failure = Some(e);
                            }
                        } else {
                            decoders[i].frame(raw, &mut display, &mut text);
                        }
                    }
                    Err(rt::TIMED_OUT | rt::WOULD_BLOCK) => continue,
                    Err(rt::DENIED | rt::REVOKED | rt::CLOSED) => {
                        channels[i] = 0;
                        decoders[i].finish(&mut display, &mut text);
                    }
                    other => {
                        failure.get_or_insert(other.err().unwrap_or(rt::BAD_ARGUMENT));
                        channels[i] = 0;
                    }
                }
                if failure.is_none() && !text.is_empty() {
                    let port = [super::tty::OUTPUT, super::tty::ERROR][i % 2];
                    if let Err(code) = super::console::write(port, &text) {
                        failure = Some(code);
                    }
                }
            }
        }
        if input != 0 {
            let _ = rt::call(rt::IPC_CLOSE, input, 0, 0);
        }
        self.request(b"NSHLIVE1", &self.session.ok_or(rt::REVOKED)?)?;
        if trace {
            for i in 0..pids.len() * 2 {
                if pids.len() == 2 && i == 0 {
                    continue;
                }
                let pid = pids[i / 2];
                let n = lengths[i];
                super::console::write(
                    super::tty::OUTPUT,
                    &alloc::format!(
                        "NXFLUX pid={pid} fd={} octets={n} prefixe={}\n",
                        i % 2 + 1,
                        hex(&samples[i][..n.min(64)])
                    ),
                )?;
            }
        }
        failure.map_or(Ok(()), Err)
    }

    pub(super) fn ticket_probe(&mut self, args: &[&str]) -> rt::Result<u64> {
        let [mode, name, rest @ ..] = args else {
            return Err(rt::BAD_ARGUMENT);
        };
        let key = self.ticket(name, rest, 5000, 0, 0)?;
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

pub(super) fn installed_name<'a>(
    name: &'a str,
    fs: &mut nova_cli::fileapi::Client,
) -> rt::Result<Option<&'a str>> {
    // Une redirection ou un pipe ne change pas la priorité des noms internes.
    // Un chemin explicite continue de désigner l'exécutable installé et attesté.
    if !name.contains('/')
        && (nova_cli::documentation::lookup(name).is_some_and(|page| page.provider != "nova-cli")
            || matches!(name, "install" | "ticket-test" | "redir-test" | "relay-test" | "source" | "."))
    {
        return Ok(None);
    }
    let leaf = if let Some(leaf) = name.strip_prefix("/system/bin/") {
        leaf
    } else if let Some(leaf) = name.strip_prefix("./") {
        if nova_cli::directory::cwd(fs)? != "/system/bin" {
            return Ok(None);
        }
        leaf
    } else {
        name
    };
    Ok((!leaf.is_empty()
        && leaf.len() <= 39
        && leaf != "."
        && leaf != ".."
        && leaf
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-' | b'.')))
    .then_some(leaf))
}
