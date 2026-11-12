//! Le terminal conserve l'entrée série. L'enfant ne reçoit que le dialogue de sa session.
use super::{capabilities::Grant, paths};
use alloc::{string::String, vec::Vec};
use nova_cli::{
    command::Program, console, fileapi::Client, process, protocol as w, runtime as rt, tty,
    vi_wire as wire,
};

#[derive(PartialEq, Eq)]
pub enum Trial {
    Normal,
    Revoke,
    Concurrent,
}

fn publish(
    fs: &mut Client,
    path: &str,
    data: &[u8],
    confirm: Option<&mut Input>,
    revoke: bool,
) -> rt::Result<()> {
    fs.simple(w::STAGE, 0)?;
    // Seule cible du groupe : un répertoire ne peut pas devenir une destination implicite de cp.
    fs.at(w::STAGE, 0, path.as_bytes(), w::SELECT_NEW)?;
    let mut grant = Grant::new();
    let result = (|| {
        grant.delegate(fs, w::GROUP, 0, &[])?;
        let mut ports = grant.channels;
        if revoke {
            ports.0 = rt::value(rt::call(rt::IPC_DUP, ports.0, rt::SEND.into(), 0))?;
            if rt::value(rt::call(rt::IPC_REVOKE, grant.channels.0, 0, 0))? != 2 {
                return Err(rt::REFUSED);
            }
        }
        let mut view = Client::scoped_ports(ports);
        if let Some(input) = confirm {
            if !input.consent(path)? {
                return Err(rt::DENIED);
            }
        }
        let opened = view.simple(w::CREATE_NEW, 1);
        if revoke {
            // Le noyau rend DENIED pour une nouvelle utilisation d'un handle déjà retiré.
            return match opened {
                Err(rt::DENIED | rt::REVOKED) => Err(rt::REVOKED),
                _ => Err(rt::REFUSED),
            };
        }
        let h = w::word(&opened?, 16);
        if data.is_empty() {
            view.sync(h)?;
        }
        for (index, chunk) in data.chunks(w::MAX_WRITE).enumerate() {
            view.write(h, (index * w::MAX_WRITE) as u64, chunk)?;
            view.sync(h)?;
        }
        view.close(h)?;
        view.simple(w::PUBLISH_COPY, 1)?;
        Ok(())
    })();
    let closed = grant.release(fs);
    result.and(closed)
}

#[derive(Default)]
struct Input {
    skip_lf: bool,
}
impl Input {
    fn key(&mut self) -> rt::Result<u8> {
        loop {
            let b = rt::value(rt::call(tty::CONSOLE, tty::INPUT.into(), 0, 0))? as u8;
            if core::mem::take(&mut self.skip_lf) && b == 10 {
                continue;
            }
            self.skip_lf = b == 13;
            return Ok(b);
        }
    }
    // Le shell est le seul lecteur de l'UART et ne reçoit jamais la réponse par IPC.
    fn consent(&mut self, path: &str) -> rt::Result<bool> {
        console::write(tty::OUTPUT, "\nnsh : créer « ")?;
        console::printable(tty::OUTPUT, path)?;
        console::write(tty::OUTPUT, " » ? Saisir OUI puis Entrée : ")?;
        let mut n = 0usize;
        let mut valid = true;
        loop {
            let b = self.key()?;
            if matches!(b, b'\r' | b'\n') {
                break;
            }
            valid &= b"OUI".get(n) == Some(&b);
            n = n.saturating_add(1);
            if n <= 3 && b.is_ascii_graphic() {
                console::write(tty::OUTPUT, core::str::from_utf8(&[b]).unwrap())?;
            }
        }
        console::write(tty::OUTPUT, "\n")?;
        Ok(valid && n == 3)
    }
}

struct Upload {
    name: Vec<u8>,
    text: Vec<u8>,
    name_len: usize,
    text_len: usize,
}
fn append(to: &mut Vec<u8>, part: &[u8], max: usize) -> rt::Result<()> {
    if part.is_empty() || part.len() > max - to.len() {
        return Err(rt::LIMIT);
    }
    if to.capacity() - to.len() < part.len() {
        let increment = to.len().clamp(128, 65536).min(max - to.len());
        to.try_reserve_exact(part.len().max(increment))
            .or_else(|_| to.try_reserve_exact(part.len()))
            .map_err(|_| rt::NO_MEMORY)?;
    }
    to.extend_from_slice(part);
    Ok(())
}

struct Session<'a> {
    fs: &'a mut Client,
    original: &'a str,
    input: Input,
    measure: bool,
    first_key: bool,
    source: Option<String>,
    upload: Option<Upload>,
    trial: Trial,
}
impl Session<'_> {
    fn save(&mut self) -> rt::Result<()> {
        let upload = self.upload.take().ok_or(rt::BAD_ARGUMENT)?;
        if upload.name.len() != upload.name_len || upload.text.len() != upload.text_len {
            return Err(rt::BAD_ARGUMENT);
        }
        let name = core::str::from_utf8(&upload.name).map_err(|_| rt::BAD_ARGUMENT)?;
        if name.is_empty()
            || !name.chars().all(console::safe)
            || matches!(name.rsplit('/').next(), Some("" | "." | ".."))
        {
            return Err(rt::BAD_ARGUMENT);
        }
        core::str::from_utf8(&upload.text).map_err(|_| rt::BAD_ARGUMENT)?;
        let path = paths::resolve(self.fs, name)?;
        // L'ancien fichier reste protégé même si un autre acteur retire son nom.
        if name == self.original && self.source.is_some() {
            return Err(w::EXISTS);
        }
        match self.fs.at(w::LOOKUP_AT, 0, path.as_bytes(), 0) {
            Ok(_) => return Err(w::EXISTS),
            Err(w::NOT_FOUND) => (),
            Err(e) => return Err(e),
        }

        // Injection d'un autre acquittement entre le dernier contrôle et CREATE_NEW.
        if self.trial == Trial::Concurrent {
            self.trial = Trial::Normal;
            publish(self.fs, &path, b"tiers\n", None, false)?;
        }
        let revoke = self.trial == Trial::Revoke;
        self.trial = Trial::Normal;
        publish(
            self.fs,
            &path,
            &upload.text,
            (name != self.original).then_some(&mut self.input),
            revoke,
        )
    }
    fn reply(&mut self, op: u8, data: &[u8], out: &mut [u8]) -> rt::Result<usize> {
        match (op, data) {
            (wire::INFO, []) => {
                out[0] = u8::from(self.source.is_some());
                let len = self.source.as_ref().map_or(0, String::len);
                out[1..9].copy_from_slice(&(len as u64).to_le_bytes());
                Ok(9)
            }
            (wire::READ, offset) => {
                let at = wire::length(offset)?;
                let bytes = self
                    .source
                    .as_deref()
                    .unwrap_or("")
                    .as_bytes()
                    .get(at..)
                    .ok_or(rt::BAD_ARGUMENT)?;
                let n = bytes.len().min(out.len());
                out[..n].copy_from_slice(&bytes[..n]);
                Ok(n)
            }
            (wire::KEY, []) => {
                if self.measure && core::mem::take(&mut self.first_key) {
                    console::edit(2)?;
                }
                out[0] = self.input.key()?;
                Ok(1)
            }
            (wire::BEGIN, sizes) if sizes.len() == 10 => {
                self.upload = None;
                let name_len = usize::from(u16::from_le_bytes([sizes[0], sizes[1]]));
                let text_len = wire::length(&sizes[2..])?;
                if name_len == 0 || name_len > w::PATH_MAX {
                    return Err(rt::BAD_ARGUMENT);
                }
                let name = Vec::new();
                let text = Vec::new();
                self.upload = Some(Upload {
                    name,
                    text,
                    name_len,
                    text_len,
                });
                Ok(0)
            }
            (wire::NAME | wire::TEXT, part) => {
                let state = self.upload.as_mut().ok_or(rt::BAD_ARGUMENT)?;
                if op == wire::NAME && state.text.is_empty() {
                    append(&mut state.name, part, state.name_len)?
                } else if op == wire::TEXT && state.name.len() == state.name_len {
                    append(&mut state.text, part, state.text_len)?
                } else {
                    return Err(rt::BAD_ARGUMENT);
                }
                Ok(0)
            }
            (wire::SAVE, []) => {
                if self.measure {
                    console::edit(2)?;
                }
                self.save()?;
                Ok(0)
            }
            (wire::FIN, []) => Ok(0),
            _ => Err(rt::BAD_ARGUMENT),
        }
    }
    fn serve(&mut self, channels: (u64, u64)) -> rt::Result<()> {
        let mut last = 0;
        loop {
            let request = rt::receive_for(channels.0, 15000)?;
            if request.kind != 1 || !(1..=128).contains(&request.len) || request.tag != last + 1 {
                return Err(rt::BAD_ARGUMENT);
            }
            last = request.tag;
            let op = request.payload[0];
            let data = &request.payload[1..request.len as usize];
            let mut frame = [0; 128];
            let n = match self.reply(op, data, &mut frame[1..]) {
                Ok(n) => n,
                Err(e) => {
                    self.upload = None;
                    frame[0] = e as u8;
                    0
                }
            };
            rt::send_for(channels.1, last, &frame[..1 + n], 15000)?;
            if op == wire::FIN && data.is_empty() {
                return Ok(());
            }
        }
    }
}
pub fn run(
    fs: &mut Client,
    name: &str,
    args: &[String],
    test: bool,
    trial: Trial,
) -> rt::Result<u64> {
    if !name.chars().all(console::safe) {
        return Err(rt::BAD_ARGUMENT);
    }
    let path = paths::resolve(fs, name)?;
    let source = match paths::read_text(fs, &path, usize::MAX) {
        Ok(s) => Some(s),
        Err(w::NOT_FOUND) => {
            let (parent, leaf) = path.rsplit_once('/').unwrap_or((".", &path));
            if leaf.is_empty() || matches!(leaf, "." | "..") {
                return Err(rt::BAD_ARGUMENT);
            }
            let parent = if parent.is_empty() { "/" } else { parent };
            let h = fs.directory(0, parent.as_bytes(), w::TRAVERSE)?;
            fs.simple(w::DIR_CLOSE, h)?;
            None
        }
        Err(e) => return Err(e),
    };
    let mut ports = Grant::new();
    let mut child = None;
    let result = (|| {
        ports.channels.0 = rt::value(rt::call(rt::IPC_CREATE, 0, 0, 0))?;
        ports.channels.1 = rt::value(rt::call(rt::IPC_CREATE, 0, 0, 0))?;
        child = Some(process::spawn(Program::Vi, args, ports.channels, 0)?);
        Session {
            fs,
            original: name,
            input: Input::default(),
            first_key: true,
            measure: test,
            source,
            upload: None,
            trial,
        }
        .serve(ports.channels)
    })();
    let released = ports.release(fs);
    let Some(child) = child else {
        result?;
        return Err(rt::REFUSED);
    };
    let (fault, status) = process::wait(child)?;
    if test {
        console::write(
            tty::OUTPUT,
            &alloc::format!(
                "[statut {} {}]\n",
                if fault { "faute" } else { "sortie" },
                status
            ),
        )?;
    }
    if fault {
        return Ok(128);
    }
    result.and(released)?;
    Ok(status)
}
