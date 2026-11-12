//! Le terminal conserve l'entrée série. L'enfant ne reçoit que le dialogue de sa session.
#[path = "vi_source.rs"]
mod backing;
use super::{capabilities::Grant, paths};
use alloc::{collections::VecDeque, string::String, vec::Vec};
use backing::Source;
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
    source: Option<(&str, &mut backing::Pages, usize)>,
) -> rt::Result<()> {
    fs.simple(w::STAGE, 0)?;
    // La destination est sélectionnée en premier : un répertoire ne peut pas devenir une destination implicite de cp.
    fs.at(w::STAGE, 0, path.as_bytes(), w::SELECT_NEW)?;
    if let Some((path, _, _)) = &source {
        fs.at(w::STAGE, 0, path.as_bytes(), w::SELECT_FILE)?;
    }
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
        if let Some((_, pages, off)) = source {
            pages.copy_tail(&mut view, h, off, data.len() as u64)?;
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
    keys: VecDeque<u8>,
    active: bool,
    responsive: bool,
    size: Option<(u16, u16)>,
}
fn screen(action: u64, arg: u64) -> rt::Result<u64> {
    rt::value(rt::call(tty::CONSOLE, tty::SCREEN.into(), action, arg))
}

fn report(bytes: &[u8]) -> Option<(u16, u16)> {
    let fields = bytes.strip_prefix(b"\x1b[")?.strip_suffix(b"R")?;
    let sep = fields.iter().position(|&b| b == b';')?;
    let axis = |s: &[u8]| {
        if s.is_empty() || s.len() > 3 || !s.iter().all(u8::is_ascii_digit) {
            return None;
        }
        core::str::from_utf8(s)
            .ok()?
            .parse::<u16>()
            .ok()
            .filter(|n| *n > 0)
    };
    Some((axis(&fields[..sep])?, axis(&fields[sep + 1..])?))
}
impl Input {
    fn leave(&mut self) -> rt::Result<()> {
        if self.active {
            screen(tty::LEAVE_SCREEN, 0)?;
            self.active = false;
        }
        Ok(())
    }
    fn geometry(&mut self, refresh: bool) -> rt::Result<(u16, u16)> {
        if !self.active {
            screen(tty::ENTER_SCREEN, 0)?;
            self.active = true;
        }
        if refresh || self.size.is_none() {
            // Les frappes déjà en transit sont rendues dans leur ordre d'origine.
            let mut bytes = [0; 64];
            let mut start = 0;
            let mut len = 0;
            let space = bytes.len().saturating_sub(self.keys.len());
            self.keys.try_reserve(space).map_err(|_| rt::NO_MEMORY)?;
            if space >= 10 {
                self.responsive = false;
                screen(tty::QUERY_SIZE, 0)?;
                while len < space {
                    let b = match screen(tty::READ_TIMED, if len == 0 { 40 } else { 8 }) {
                        Ok(b) => b as u8,
                        Err(rt::TIMED_OUT) => break,
                        Err(e) => return Err(e),
                    };
                    if b == 27 {
                        start = len;
                    }
                    bytes[len] = b;
                    len += 1;
                    if b == b'R'
                        && let Some(dimensions) = report(&bytes[start..len])
                    {
                        self.responsive = true;
                        self.size = Some(dimensions);
                        len = start;
                        break;
                    }
                }
                self.keys.extend(&bytes[..len]);
            }
        }
        Ok(*self.size.get_or_insert((24, 80)))
    }

    fn event(&mut self) -> rt::Result<Option<u8>> {
        while self.responsive && self.keys.is_empty() {
            match screen(tty::READ_TIMED, 500) {
                Ok(b) => self.keys.push_back(b as u8),
                Err(rt::TIMED_OUT) => {
                    let previous = self.size;
                    if Some(self.geometry(true)?) != previous {
                        return Ok(None);
                    }
                }
                Err(e) => return Err(e),
            }
        }
        self.key().map(Some)
    }
    fn key(&mut self) -> rt::Result<u8> {
        loop {
            let b = match self.keys.pop_front() {
                Some(b) => b,
                None => rt::value(rt::call(tty::CONSOLE, tty::INPUT.into(), 0, 0))? as u8,
            };
            if core::mem::take(&mut self.skip_lf) && b == 10 {
                continue;
            }
            self.skip_lf = b == 13;
            return Ok(b);
        }
    }
    // Le shell est le seul lecteur de l'UART et ne reçoit jamais la réponse par IPC.
    fn consent(&mut self, path: &str) -> rt::Result<bool> {
        self.leave()?;
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

impl Drop for Input {
    fn drop(&mut self) {
        let _ = self.leave();
    }
}

struct Upload {
    name: Vec<u8>,
    text: Vec<u8>,
    name_len: usize,
    text_len: usize,
    suffix: Option<usize>,
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
    source: Option<Source>,
    path: &'a str,
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
        if let Some(Source::Pages(ref mut p)) = self.source {
            p.check(self.fs, self.path)?;
        }
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
            publish(self.fs, &path, b"tiers\n", None, false, None)?;
        }
        let revoke = self.trial == Trial::Revoke;
        self.trial = Trial::Normal;
        publish(
            self.fs,
            &path,
            &upload.text,
            (name != self.original).then_some(&mut self.input),
            revoke,
            match (&mut self.source, upload.suffix) {
                (Some(Source::Pages(p)), Some(off)) => Some((self.path, p, off)),
                (_, None) => None,
                _ => return Err(rt::BAD_ARGUMENT),
            },
        )
    }
    fn reply(&mut self, op: u8, data: &[u8], out: &mut [u8]) -> rt::Result<usize> {
        match (op, data) {
            (wire::INFO, []) => {
                out[0] = u8::from(self.source.is_some());
                let len = self.source.as_ref().map_or(0, Source::len);
                out[1..9].copy_from_slice(&(len as u64).to_le_bytes());
                Ok(9)
            }
            (wire::VERIFY, []) => {
                if let Some(Source::Pages(p)) = &mut self.source {
                    p.check(self.fs, self.path)?;
                }
                Ok(0)
            }
            (wire::READ, offset) => {
                let at = wire::length(offset)?;
                match self.source.as_mut() {
                    Some(s) => s.read(self.fs, at, out),
                    None if at == 0 => Ok(0),
                    None => Err(rt::BAD_ARGUMENT),
                }
            }
            (wire::VIEW, [refresh @ 0..=1]) => {
                if self.measure && core::mem::take(&mut self.first_key) {
                    console::edit(2)?;
                }
                let fresh = !self.input.active;
                let (rows, cols) = self.input.geometry(*refresh == 1)?;
                out[..2].copy_from_slice(&rows.to_le_bytes());
                out[2..4].copy_from_slice(&cols.to_le_bytes());
                out[4] = u8::from(fresh);
                Ok(5)
            }
            (wire::LOCATE, [r0, r1, c0, c1]) if self.input.active => {
                let r = u16::from_le_bytes([*r0, *r1]);
                let c = u16::from_le_bytes([*c0, *c1]);
                let (rows, cols) = self.input.size.ok_or(rt::BAD_ARGUMENT)?;
                if r == 0 || r > rows || c == 0 || c > cols {
                    return Err(rt::BAD_ARGUMENT);
                }
                screen(tty::POSITION, (u64::from(r) << 16) | u64::from(c))?;
                Ok(0)
            }
            (wire::CLEAR_TAIL, []) if self.input.active => {
                screen(tty::ERASE_EOL, 0)?;
                Ok(0)
            }
            (wire::KEY, [1]) => {
                if let Some(b) = self.input.event()? {
                    out[0] = b;
                    Ok(1)
                } else {
                    Ok(0)
                }
            }
            (wire::KEY, []) => {
                out[0] = self.input.key()?;
                Ok(1)
            }
            (wire::BEGIN, sizes) if matches!(sizes.len(), 10 | 18) => {
                self.upload = None;
                let name_len = usize::from(u16::from_le_bytes([sizes[0], sizes[1]]));
                let text_len = wire::length(&sizes[2..10])?;
                let suffix = if sizes.len() == 18 {
                    let off = wire::length(&sizes[10..])?;
                    match &self.source {
                        Some(src @ Source::Pages(_)) if off < src.len() => Some(off),
                        _ => return Err(rt::BAD_ARGUMENT),
                    }
                } else {
                    None
                };
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
                    suffix,
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
                    self.input.leave()?;
                    console::edit(2)?;
                }
                self.save()?;
                Ok(0)
            }
            (wire::FIN, []) => self.input.leave().map(|_| 0),
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
    let source = match Source::open(fs, &path) {
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
    let pinned = source.as_ref().and_then(|s| match s {
        Source::Pages(p) => Some(p.handle()),
        _ => None,
    });
    let mut ports = Grant::new();
    let mut child = None;
    let result = (|| {
        ports.channels.0 = rt::value(rt::call(rt::IPC_CREATE, 0, 0, 0))?;
        ports.channels.1 = rt::value(rt::call(rt::IPC_CREATE, 0, 0, 0))?;
        child = Some(process::spawn(Program::Vi, args, ports.channels, 0)?);
        Session {
            fs,
            original: name,
            path: &path,
            input: Input::default(),
            first_key: true,
            measure: test,
            source,
            upload: None,
            trial,
        }
        .serve(ports.channels)
    })();
    let closed = pinned.map_or(Ok(()), |h| fs.close(h));
    let released = ports.release(fs).and(closed);
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
