use super::{Invalid, Launch};
use nova_cli::{fileapi::Client, process, protocol as fs, runtime as rt};

impl Launch<'static> {
    pub fn current() -> rt::Result<Self> {
        // DATA est une page privée de 4096 octets. Le lanceur l’initialise avant
        // la publication, ce runtime ne la modifie pas pendant la vie du processus.
        let page = unsafe { core::slice::from_raw_parts(0x401000 as *const u8, 4096) };
        Self::decode(page).map_err(|e| match e {
            Invalid::Memory => rt::NO_MEMORY,
            Invalid::Limit => rt::LIMIT,
            Invalid::Format => rt::BAD_ARGUMENT,
        })
    }
}

impl Launch<'_> {
    pub fn read_stdin(&self, buf: &mut [u8]) -> rt::Result<usize> {
        if self.stdin == 0 {
            return Err(rt::DENIED);
        }
        process::pipe_read(self.stdin, buf)
    }
    pub fn write_stdout(&self, buf: &[u8]) -> rt::Result<()> {
        write(self.stdout, buf)
    }
    pub fn write_stderr(&self, buf: &[u8]) -> rt::Result<()> {
        write(self.stderr, buf)
    }

    /// Le service doit attester la lignée du canal, le cwd et l’objet à chaque accès.
    /// La fermeture est tentée aussi en cas d’échec de la lecture ou du consommateur.
    pub fn with_input<T>(
        &self,
        path: &[u8],
        use_input: impl FnOnce(&mut dyn FnMut(&mut [u8]) -> rt::Result<usize>) -> rt::Result<T>,
    ) -> rt::Result<T> {
        if path == b"-" {
            return use_input(&mut |buf| self.read_stdin(buf));
        }
        if self.files.0 == 0 {
            return Err(rt::DENIED);
        }
        let mut client = Client::scoped_ports(self.files);
        let handle = client.open_at(0, path, fs::READABLE)?;
        let result = (|| {
            let stat = client.simple(fs::STAT, handle)?;
            let size = fs::word(&stat, fs::PAYLOAD + 16);
            let mut offset = 0;
            use_input(&mut |buf| {
                let wanted = (size - offset).min(buf.len() as u64) as usize;
                if wanted == 0 {
                    return Ok(0);
                }
                let n = client.read(handle, offset, &mut buf[..wanted])?;
                if n == 0 || n > wanted {
                    return Err(fs::IO);
                }
                offset += n as u64;
                Ok(n)
            })
        })();
        let closed = client.close(handle);
        let value = result?;
        closed?;
        Ok(value)
    }
}
fn write(key: u64, bytes: &[u8]) -> rt::Result<()> {
    if key == 0 {
        return Err(rt::DENIED);
    }
    process::pipe_write(key, bytes)
        .map(|_| ())
        .map_err(|(_, reason)| reason)
}
