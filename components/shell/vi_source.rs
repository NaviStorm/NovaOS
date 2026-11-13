//! La lecture différée reste attachée à l'objet ouvert, jamais à un chemin rouvert.
use alloc::{string::String, vec::Vec};
use nova_cli::{fileapi::Client, protocol as w, runtime as rt, vi_wire as wire};

pub enum Source {
    Snapshot(String),
    Pages(Pages),
}
pub struct Pages {
    h: u64,
    id: u64,
    rev: u64,
    len: usize,
    start: Option<usize>,
    bytes: Vec<u8>,
    conflict: bool,
}
impl Source {
    pub fn open(fs: &mut Client, path: &str) -> rt::Result<Self> {
        let stat = fs.at(w::LOOKUP_AT, 0, path.as_bytes(), 0)?;
        let len = usize::try_from(w::word(&stat, w::PAYLOAD + 16)).map_err(|_| rt::LIMIT)?;
        if len <= wire::EAGER || stat[w::PAYLOAD + 24] != 1 {
            return super::paths::read_editor_text(fs, path).map(Self::Snapshot);
        }
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(w::READ_WINDOW)
            .map_err(|_| rt::NO_MEMORY)?;
        let h = fs.open_at(0, path.as_bytes(), w::READABLE)?;
        let mut p = Pages {
            h,
            id: w::word(&stat, w::PAYLOAD),
            rev: w::word(&stat, w::PAYLOAD + 8),
            len,
            start: None,
            bytes,
            conflict: false,
        };
        if let Err(e) = p.check(fs, path) {
            fs.close(h)?;
            return Err(e);
        }
        Ok(Self::Pages(p))
    }
    pub fn len(&self) -> usize {
        match self {
            Self::Snapshot(s) => s.len(),
            Self::Pages(p) => p.len,
        }
    }
    pub fn read(&mut self, fs: &mut Client, at: usize, out: &mut [u8]) -> rt::Result<usize> {
        let tail = match self {
            Self::Snapshot(s) => s.as_bytes().get(at..).ok_or(rt::BAD_ARGUMENT)?,
            Self::Pages(p) => p.slice(fs, at)?,
        };
        let n = tail.len().min(out.len());
        out[..n].copy_from_slice(&tail[..n]);
        Ok(n)
    }
}
impl Pages {
    pub fn handle(&self) -> u64 {
        self.h
    }
    pub fn check(&mut self, fs: &mut Client, path: &str) -> rt::Result<()> {
        if self.conflict {
            return Err(wire::CONFLICT);
        }
        let stat = fs.at(w::LOOKUP_AT, 0, path.as_bytes(), 0);
        match stat {
            Ok(s)
                if w::word(&s, w::PAYLOAD) == self.id
                    && w::word(&s, w::PAYLOAD + 8) == self.rev
                    && w::word(&s, w::PAYLOAD + 16) == self.len as u64 =>
            {
                Ok(())
            }
            Ok(_) | Err(w::NOT_FOUND | rt::REVOKED) => {
                self.conflict = true;
                Err(wire::CONFLICT)
            }
            Err(e) => Err(e),
        }
    }
    fn slice(&mut self, fs: &mut Client, at: usize) -> rt::Result<&[u8]> {
        if self.conflict {
            return Err(wire::CONFLICT);
        }
        if at > self.len {
            return Err(rt::BAD_ARGUMENT);
        }
        if at == self.len {
            return Ok(&[]);
        }
        let start = at / w::READ_WINDOW * w::READ_WINDOW;
        if self.start != Some(start) {
            self.start = None;
            self.bytes.resize((self.len - start).min(w::READ_WINDOW), 0);
            match fs.read_version(self.h, start as u64, &mut self.bytes, self.rev) {
                Ok(rev) => self.rev = rev,
                Err(rt::REVOKED | w::NOT_FOUND) => {
                    self.conflict = true;
                    return Err(wire::CONFLICT);
                }
                Err(e) => return Err(e),
            }
            self.start = Some(start);
        }
        Ok(&self.bytes[at - start..])
    }
}

impl Pages {
    /// Slot 2 : source en lecture seule. Le groupe conserve exclusivement la main
    /// jusqu'à la publication ou à l'abandon de la candidate du slot 1.
    pub fn copy_tail(
        &mut self,
        view: &mut Client,
        dest: u64,
        mut pos: usize,
        mut dst: u64,
    ) -> rt::Result<()> {
        let stat = view.simple(w::STAT, 2)?;
        if w::word(&stat, w::PAYLOAD) != self.id
            || w::word(&stat, w::PAYLOAD + 8) != self.rev
            || w::word(&stat, w::PAYLOAD + 16) != self.len as u64
        {
            self.conflict = true;
            return Err(wire::CONFLICT);
        }
        if pos > self.len {
            return Err(rt::BAD_ARGUMENT);
        }
        self.start = None;
        while pos < self.len {
            let n = (self.len - pos).min(w::MAX_WRITE);
            let mut q = w::frame(w::COPY_RANGE, view.session, dest);
            q[2] = w::TEXT_RANGE;
            w::set(&mut q, 24, pos as u64);
            w::set(&mut q, 32, dst);
            w::set(&mut q, 40, self.rev);
            w::set(&mut q, 48, n as u64);
            let a = match view.request(q) {
                Err(rt::REVOKED | w::NOT_FOUND) => {
                    self.conflict = true;
                    return Err(wire::CONFLICT);
                }
                r => r?,
            };
            let copied = usize::try_from(w::word(&a, 24)).map_err(|_| rt::BAD_ARGUMENT)?;
            if copied == 0 || copied > n {
                return Err(rt::BAD_ARGUMENT);
            }
            self.rev = w::word(&a, 32);
            pos += copied;
            dst = dst.checked_add(copied as u64).ok_or(rt::LIMIT)?;
        }
        Ok(())
    }
}
