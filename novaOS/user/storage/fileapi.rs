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
    /// Vue temporaire fournie au processus enfant par le lanceur.
    pub fn scoped() -> rt::Result<Self> {
        let mut c = Self::new()?;
        c.session = 1;
        Ok(c)
    }
    pub fn give(&self, source: u64, rights: u32) -> rt::Result<()> {
        let offer = rt::GiveV1 {
            version: rt::VERSION,
            size: 48,
            timeout: 15000,
            source,
            tag: u64::from(rights),
            rights,
            mode: 1,
            reserved: 0,
        };
        rt::value(rt::call(
            rt::IPC_GIVE,
            self.tx,
            (&raw const offer) as u64,
            48,
        ))
        .map(|_| ())
    }
    pub fn delegate(&mut self, kind: u8, handle: u64, leaf: &[u8]) -> rt::Result<()> {
        if !leaf.is_empty() {
            self.path_fragment(0, leaf)?;
        }
        let mut q = w::frame(w::DELEGATE, self.session, handle);
        q[2] = kind;
        self.request(q).map(|_| ())
    }
    pub fn request(&mut self, q: w::Frame) -> rt::Result<w::Frame> {
        if self.disconnected {
            return Err(rt::CLOSED);
        }
        self.tag = self.tag.checked_add(1).ok_or(rt::LIMIT)?;
        rt::send_for(self.tx, self.tag, &q, 15000)?;
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
                if matches!(
                    q[1],
                    w::SYNC | w::MKDIR_AT | w::RMDIR | w::RENAME_AT | w::UNLINK_AT
                ) || (matches!(q[1], w::OPEN | w::OPEN_AT) && q[2] & w::CREATE != 0)
                {
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

impl Client {
    fn path_fragment(&mut self, slot: u8, bytes: &[u8]) -> rt::Result<()> {
        if bytes.is_empty() || bytes.len() > w::PATH_MAX {
            return Err(rt::LIMIT);
        }
        for (i, part) in bytes.chunks(w::FRAGMENT).enumerate() {
            let mut q = w::frame(w::PATH_FRAGMENT, self.session, 0);
            q[2] = slot;
            w::size(&mut q, part.len());
            w::set(&mut q, 24, (i * w::FRAGMENT) as u64);
            w::set(&mut q, 32, bytes.len() as u64);
            q[w::PAYLOAD..w::PAYLOAD + part.len()].copy_from_slice(part);
            self.request(q)?;
        }
        Ok(())
    }
    pub fn at(&mut self, op: u8, dir: u64, name: &[u8], rights: u8) -> rt::Result<w::Frame> {
        let mut q = w::frame(op, self.session, dir);
        q[2] = rights;
        if name.is_empty() {
            return Err(rt::BAD_ARGUMENT);
        }
        if name.len() <= w::FRAGMENT {
            w::size(&mut q, name.len());
            q[w::PAYLOAD..w::PAYLOAD + name.len()].copy_from_slice(name);
        } else {
            self.path_fragment(0, name)?;
        }
        self.request(q)
    }
    pub fn rename(&mut self, src: u64, name: &[u8], dest: u64, to: &[u8]) -> rt::Result<()> {
        self.path_fragment(0, name)?;
        self.path_fragment(1, to)?;
        let mut q = w::frame(w::RENAME_AT, self.session, src);
        w::set(&mut q, 32, dest);
        self.request(q).map(|_| ())
    }
    pub fn directory(&mut self, base: u64, path: &[u8], rights: u8) -> rt::Result<u64> {
        Ok(w::word(&self.at(w::DIR_OPEN, base, path, rights)?, 16))
    }
    pub fn open_at(&mut self, base: u64, path: &[u8], rights: u8) -> rt::Result<u64> {
        Ok(w::word(&self.at(w::OPEN_AT, base, path, rights)?, 16))
    }
    pub fn readdir_fragment(
        &mut self,
        dir: u64,
        after: &[u8],
        offset: u64,
        generation: u64,
    ) -> rt::Result<w::Frame> {
        let mut q = w::frame(w::READDIR, self.session, dir);
        if after.len() > w::FRAGMENT {
            self.path_fragment(0, after)?;
        } else {
            w::size(&mut q, after.len());
            q[w::PAYLOAD..w::PAYLOAD + after.len()].copy_from_slice(after);
        }
        w::set(&mut q, 24, offset);
        w::set(&mut q, 32, generation);
        self.request(q)
    }

    pub fn cwd_fragment(&mut self, pos: u64, generation: u64) -> rt::Result<w::Frame> {
        let mut q = w::frame(w::GETCWD, self.session, 0);
        w::set(&mut q, 24, pos);
        w::set(&mut q, 32, generation);
        self.request(q)
    }
}
