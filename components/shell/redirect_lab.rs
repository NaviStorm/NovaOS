//! Handles privés de nsh. Les enfants ne reçoivent que leurs extrémités de flux.
use alloc::string::String;
use nova_cli::{fileapi::Client, protocol as w, runtime as rt};

pub(super) struct Files<'a> {
    fs: &'a mut Client,
    handles: [Option<u64>; 6],
    offset: [u64; 6],
    payload: [u8; 128],
    pending: usize,
}
impl<'a> Files<'a> {
    pub(super) fn new(fs: &'a mut Client) -> Self {
        Self {
            fs,
            handles: [None; 6],
            offset: [0; 6],
            payload: [0; 128],
            pending: 0,
        }
    }
    pub(super) fn open(&mut self, paths: &[Option<String>; 6]) -> rt::Result<()> {
        for (fd, path) in paths.iter().enumerate() {
            let Some(path) = path else { continue };
            let rights = if fd % 3 == 0 {
                w::READABLE
            } else {
                w::CREATE | w::WRITABLE
            };
            self.handles[fd] = Some(self.fs.open_at(0, path.as_bytes(), 0x80 | rights)?);
        }
        Ok(())
    }
    pub(super) fn has(&self, fd: usize) -> bool {
        self.handles[fd].is_some()
    }
    pub(super) fn has_input(&self) -> bool {
        self.has(0)
    }
    pub(super) fn write(&mut self, fd: usize, bytes: &[u8]) -> rt::Result<()> {
        let key = self.handles[fd].ok_or(rt::REVOKED)?;
        let next = self.offset[fd]
            .checked_add(bytes.len() as u64)
            .ok_or(rt::LIMIT)?;
        self.fs.write(key, self.offset[fd], bytes)?;
        self.fs.sync(key)?;
        self.offset[fd] = next;
        Ok(())
    }
    pub(super) fn feed(&mut self, port: u64) -> rt::Result<bool> {
        if self.pending == 0 {
            let key = self.handles[0].ok_or(rt::REVOKED)?;
            self.pending = self
                .fs
                .read(key, self.offset[0], &mut self.payload[..w::FRAGMENT])?;
            if self.pending == 0 {
                return Ok(false);
            }
        }
        let mut frame = rt::SendV1 {
            version: rt::VERSION,
            size: 160,
            timeout: 0,
            tag: 0,
            len: self.pending as u32,
            flags: 0,
            payload: [0; 128],
        };
        frame.payload[..self.pending].copy_from_slice(&self.payload[..self.pending]);
        match rt::value(rt::call(rt::IPC_SEND, port, (&raw const frame) as u64, 160)) {
            Ok(n) if n == self.pending as u64 => {
                self.offset[0] = self.offset[0]
                    .checked_add(self.pending as u64)
                    .ok_or(rt::LIMIT)?;
                self.pending = 0;
                Ok(true)
            }
            Ok(_) => Err(rt::REFUSED),
            Err(rt::WOULD_BLOCK) => Ok(true),
            Err(rt::CLOSED) => Ok(false),
            Err(code) => Err(code),
        }
    }
    pub(super) fn close(&mut self) -> rt::Result<()> {
        let mut result = Ok(());
        for slot in &mut self.handles {
            if let Some(key) = slot.take()
                && let Err(code) = self.fs.close(key)
                && result.is_ok()
            {
                result = Err(code);
            }
        }
        result
    }
}
