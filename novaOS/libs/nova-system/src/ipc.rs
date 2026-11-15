pub use crate::abi::{GRANT, MANAGE, RECEIVE, ReceiveV1, SEND};
use crate::{Error, Result, abi, call, client_io};

/// Possession d’un handle IPC dans la table du processus courant.
pub struct Endpoint(u64);
impl Endpoint {
    pub fn create() -> Result<Self> {
        call(abi::IPC_CREATE, 0, 0, 0).map(Self)
    }

    /// # Safety
    /// Le droit local doit appartenir à l’appelant. Transférer sa responsabilité
    /// de fermeture une seule fois et exclure toute fermeture concurrente.
    pub unsafe fn from_raw_owned(raw: u64) -> Self {
        Self(raw)
    }
    pub fn raw(&self) -> u64 {
        self.0
    }
    pub fn duplicate(&self, rights: u32) -> Result<Self> {
        call(abi::IPC_DUP, self.0, rights.into(), 0).map(Self)
    }
    pub fn revoke(&self) -> Result<()> {
        call(abi::IPC_REVOKE, self.0, 0, 0).map(|_| ())
    }
    /// Délégation dérivée. La source conserve son droit. Les deux handles et
    /// le masque sont revalidés par le noyau lors de la remise au destinataire.
    pub fn give(&self, source: &Endpoint, rights: u32, tag: u64, ticks: u64) -> Result<()> {
        let msg = abi::GiveV1 {
            version: abi::VERSION,
            size: size_of::<abi::GiveV1>() as u32,
            timeout: ticks,
            source: source.0,
            tag,
            rights,
            mode: 1,
            reserved: 0,
        };
        call(
            abi::IPC_GIVE,
            self.0,
            (&raw const msg) as u64,
            msg.size.into(),
        )
        .map(|_| ())
    }
    pub fn send(&self, tag: u64, bytes: &[u8], timeout: u64) -> Result<()> {
        client_io::send_frame(self.0, tag, bytes, timeout, timeout != 0).map_err(Error)
    }
    pub fn receive(&self, timeout: u64) -> Result<ReceiveV1> {
        client_io::receive_for(self.0, timeout).map_err(Error)
    }
    pub fn close(mut self) -> Result<()> {
        let handle = core::mem::take(&mut self.0);
        call(abi::IPC_CLOSE, handle, 0, 0).map(|_| ())
    }
}
impl Drop for Endpoint {
    fn drop(&mut self) {
        if self.0 != 0 {
            let _ = call(abi::IPC_CLOSE, self.0, 0, 0);
        }
    }
}
