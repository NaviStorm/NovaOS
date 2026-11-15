//! Vue de fichiers ouverte sur deux capacités explicitement déléguées.
pub use crate::protocol::{DIR_RO, ENUMERATE, FILE_READ, FILE_WRITE, READABLE, TRAVERSE, WRITABLE};
use crate::{Error, Result, file_client::Client, ipc::Endpoint, protocol};

pub struct Session {
    client: Client,
    _requests: Endpoint,
    _replies: Endpoint,
}

impl Session {
    /// La paire doit être dédiée à cette session pour corréler ses réponses.
    pub fn delegated(requests: Endpoint, replies: Endpoint) -> Self {
        Self {
            client: Client::scoped_ports((requests.raw(), replies.raw())),
            _requests: requests,
            _replies: replies,
        }
    }

    pub fn open(&mut self, base: u64, name: &[u8], rights: u8) -> Result<File<'_>> {
        let key = self.client.open_at(base, name, rights).map_err(Error)?;
        Ok(File {
            client: &mut self.client,
            key: Some(key),
        })
    }

    pub fn directory(&mut self, base: u64, name: &[u8], rights: u8) -> Result<Directory<'_>> {
        let key = self.client.directory(base, name, rights).map_err(Error)?;
        Ok(Directory {
            client: &mut self.client,
            key: Some(key),
        })
    }
}

pub struct File<'a> {
    client: &'a mut Client,
    key: Option<u64>,
}
impl File<'_> {
    pub fn read(&mut self, offset: u64, bytes: &mut [u8]) -> Result<usize> {
        self.client
            .read(self.key.unwrap(), offset, bytes)
            .map_err(Error)
    }
    pub fn write(&mut self, offset: u64, bytes: &[u8]) -> Result<()> {
        self.client
            .write(self.key.unwrap(), offset, bytes)
            .map_err(Error)
    }
    pub fn sync(&mut self) -> Result<u64> {
        self.client.sync(self.key.unwrap()).map_err(Error)
    }
    pub fn close(mut self) -> Result<()> {
        self.client.close(self.key.take().unwrap()).map_err(Error)
    }
}
impl Drop for File<'_> {
    fn drop(&mut self) {
        if let Some(key) = self.key.take() {
            let _ = self.client.close(key);
        }
    }
}

pub struct Directory<'a> {
    client: &'a mut Client,
    key: Option<u64>,
}
impl Directory<'_> {
    /// Fragment de nom au format du protocole fichier v2. Le curseur et la
    /// génération retournée par le service accompagnent chaque continuation.
    pub fn read_fragment(
        &mut self,
        after: &[u8],
        offset: u64,
        generation: u64,
    ) -> Result<[u8; 128]> {
        self.client
            .readdir_fragment(self.key.unwrap(), after, offset, generation)
            .map_err(Error)
    }
    pub fn close(mut self) -> Result<()> {
        self.client
            .simple(protocol::DIR_CLOSE, self.key.take().unwrap())
            .map(|_| ())
            .map_err(Error)
    }
}
impl Drop for Directory<'_> {
    fn drop(&mut self) {
        if let Some(key) = self.key.take() {
            let _ = self.client.simple(protocol::DIR_CLOSE, key);
        }
    }
}
