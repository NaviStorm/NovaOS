//! Bibliothèque native sur deux capacités privées. Les handles sont locaux à la session.
use crate::{protocol as w, runtime as rt};
pub struct Client {
    tx: u64,
    rx: u64,
    tag: u64,
    pub session: u64,
    disconnected: bool,
}
impl Client {
    pub fn new() -> rt::Result<Self> {
        let (tx, rx) = rt::channels()?;
        Ok(Self {
            tx,
            rx,
            tag: 0,
            session: 0,
            disconnected: false,
        })
    }
    pub fn request(&mut self, q: w::Frame) -> rt::Result<w::Frame> {
        if self.disconnected {
            return Err(rt::CLOSED);
        }
        self.tag = self.tag.checked_add(1).ok_or(rt::LIMIT)?;
        rt::send(self.tx, self.tag, &q)?;
        let r = rt::receive_for(self.rx, 15000)
            .and_then(|r| {
                if r.kind == 1 && r.tag == self.tag && r.len == 128 && r.payload[..2] == q[..2] {
                    Ok(r)
                } else {
                    Err(rt::BAD_ARGUMENT)
                }
            })
            .map_err(|e| {
                self.disconnected = true;
                if q[1] == w::SYNC || (q[1] == w::OPEN && q[2] & w::CREATE != 0) {
                    w::UNCERTAIN
                } else {
                    e
                }
            })?;
        if r.payload[3] != 0 {
            return Err(r.payload[3] as u64);
        }
        Ok(r.payload)
    }
    pub fn simple(&mut self, op: u8, h: u64) -> rt::Result<w::Frame> {
        self.request(w::frame(op, self.session, h))
    }
    pub fn mount(&mut self) -> rt::Result<()> {
        self.session = w::word(&self.simple(w::MOUNT, 0)?, 8);
        if self.session == 0 {
            Err(rt::BAD_ARGUMENT)
        } else {
            Ok(())
        }
    }
    pub fn info(&mut self) -> rt::Result<w::Frame> {
        self.simple(w::INFO, 0)
    }
    pub fn sync(&mut self, file: u64) -> rt::Result<u64> {
        Ok(w::word(&self.simple(w::SYNC, file)?, 32))
    }
    pub fn close(&mut self, file: u64) -> rt::Result<()> {
        self.simple(w::CLOSE, file).map(|_| ())
    }
    pub fn open(&mut self, path: &[u8], rights: u8) -> rt::Result<u64> {
        if path.len() > w::FRAGMENT {
            return Err(rt::LIMIT);
        }
        let mut q = w::frame(w::OPEN, self.session, 0);
        q[2] = rights;
        w::size(&mut q, path.len());
        q[w::PAYLOAD..w::PAYLOAD + path.len()].copy_from_slice(path);
        Ok(w::word(&self.request(q)?, 16))
    }
    pub fn read(&mut self, h: u64, off: u64, out: &mut [u8]) -> rt::Result<usize> {
        let mut done = 0;
        for chunk in out.chunks_mut(w::FRAGMENT) {
            let mut q = w::frame(w::READ, self.session, h);
            w::size(&mut q, chunk.len());
            w::set(
                &mut q,
                24,
                off.checked_add(done as u64).ok_or(rt::BAD_ARGUMENT)?,
            );
            let a = self.request(q)?;
            let n = w::length(&a);
            if n > chunk.len() {
                return Err(rt::BAD_ARGUMENT);
            }
            chunk[..n].copy_from_slice(&a[w::PAYLOAD..w::PAYLOAD + n]);
            done += n;
            if n != chunk.len() {
                break;
            }
        }
        Ok(done)
    }
    pub fn write(&mut self, h: u64, off: u64, bytes: &[u8]) -> rt::Result<()> {
        if bytes.is_empty()
            || bytes.len() > w::MAX_WRITE
            || off.checked_add(bytes.len() as u64).is_none()
        {
            return Err(rt::BAD_ARGUMENT);
        }
        let mut sent = 0;
        for chunk in bytes.chunks(w::FRAGMENT) {
            let mut q = w::frame(w::WRITE, self.session, h);
            q[2] = if sent == 0 { w::START } else { w::CONTINUE };
            w::size(&mut q, chunk.len());
            w::set(&mut q, 24, off + sent as u64);
            w::set(&mut q, 32, bytes.len() as u64);
            q[w::PAYLOAD..w::PAYLOAD + chunk.len()].copy_from_slice(chunk);
            sent += chunk.len();
            if w::word(&self.request(q)?, 32) != sent as u64 {
                return Err(rt::BAD_ARGUMENT);
            }
        }
        Ok(())
    }
}
