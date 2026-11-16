use alloc::{format, string::String, vec::Vec};
use nova_cli::{protocol as fs, runtime as rt};
#[allow(dead_code)]
#[path = "../../../novaOS/abi/tmp.rs"]
mod tmp;
use super::native::{Reader, Stream};

#[derive(Clone, Copy)]
struct Port(u64);
impl Port {
    fn get() -> rt::Result<Self> {
        rt::value(rt::call(tmp::CAP, 0, 0, 0)).map(Self)
    }
    fn request(self, mut q: fs::Frame) -> rt::Result<fs::Frame> {
        rt::value(rt::call(
            tmp::CALL,
            self.0,
            q.as_mut_ptr() as u64,
            q.len() as u64,
        ))?;
        Ok(q)
    }
    fn named(self, op: u8, name: &str, rights: u8) -> rt::Result<u64> {
        let mut q = fs::frame(op, 1, 0);
        q[2] = rights;
        fs::size(&mut q, name.len());
        q[fs::PAYLOAD..fs::PAYLOAD + name.len()].copy_from_slice(name.as_bytes());
        Ok(fs::word(&self.request(q)?, 16))
    }
}
pub struct Run {
    port: Port,
    name: String,
}
struct Handle {
    port: Port,
    id: u64,
}
impl Drop for Handle {
    fn drop(&mut self) {
        let _ = self.port.request(fs::frame(fs::CLOSE, 1, self.id));
    }
}
impl Drop for Run {
    fn drop(&mut self) {
        let _ = self.port.named(fs::UNLINK_AT, &self.name, 0);
    }
}
pub struct Input {
    handle: Handle,
    offset: u64,
}
impl Stream for Input {
    fn read(&mut self, buf: &mut [u8]) -> rt::Result<usize> {
        let mut q = fs::frame(fs::READ, 1, self.handle.id);
        let size = buf.len().min(fs::FRAGMENT);
        fs::size(&mut q, size);
        fs::set(&mut q, 24, self.offset);
        let reply = self.handle.port.request(q)?;
        let n = fs::length(&reply);
        if n > size {
            return Err(fs::IO);
        }
        buf[..n].copy_from_slice(&reply[fs::PAYLOAD..fs::PAYLOAD + n]);
        self.offset += n as u64;
        Ok(n)
    }
}
impl Run {
    pub fn reader(&self) -> rt::Result<Reader<Input>> {
        let id = self.port.named(fs::OPEN_AT, &self.name, fs::READABLE)?;
        Ok(Reader::new(Input {
            handle: Handle {
                port: self.port,
                id,
            },
            offset: 0,
        }))
    }
    pub fn write(seq: usize, body: impl FnOnce(&mut Writer) -> rt::Result<()>) -> rt::Result<Self> {
        let port = Port::get()?;
        let name = format!("sort-{seq:016x}");
        match port.named(fs::LOOKUP_AT, &name, 0) {
            Err(fs::NOT_FOUND) => (),
            Ok(_) => return Err(fs::EXISTS),
            Err(e) => return Err(e),
        }
        let id = port.named(fs::OPEN_AT, &name, fs::CREATE | fs::READABLE | fs::WRITABLE)?;
        let run = Run { port, name };
        let mut writer = Writer {
            handle: Handle { port, id },
            offset: 0,
            buf: Vec::new(),
        };
        body(&mut writer)?;
        writer.flush()?;
        drop(writer);
        Ok(run)
    }
}
pub struct Writer {
    handle: Handle,
    offset: u64,
    buf: Vec<u8>,
}
impl Writer {
    fn flush(&mut self) -> rt::Result<()> {
        if self.buf.is_empty() {
            return Ok(());
        }
        let mut sent = 0;
        for chunk in self.buf.chunks(fs::FRAGMENT) {
            let mut q = fs::frame(fs::WRITE, 1, self.handle.id);
            q[2] = if sent == 0 { fs::START } else { fs::CONTINUE };
            fs::size(&mut q, chunk.len());
            fs::set(&mut q, 24, self.offset + sent as u64);
            fs::set(&mut q, 32, self.buf.len() as u64);
            q[fs::PAYLOAD..fs::PAYLOAD + chunk.len()].copy_from_slice(chunk);
            sent += chunk.len();
            if fs::word(&self.handle.port.request(q)?, 32) != sent as u64 {
                return Err(fs::IO);
            }
        }
        self.handle
            .port
            .request(fs::frame(fs::SYNC, 1, self.handle.id))?;
        self.offset += sent as u64;
        self.buf.clear();
        Ok(())
    }
    pub fn bytes(&mut self, mut bytes: &[u8]) -> rt::Result<()> {
        while !bytes.is_empty() {
            let n = (4096 - self.buf.len()).min(bytes.len());
            self.buf.extend_from_slice(&bytes[..n]);
            bytes = &bytes[n..];
            if self.buf.len() == 4096 {
                self.flush()?
            }
        }
        Ok(())
    }
}
