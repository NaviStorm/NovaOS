//! Flux d'archive et espace de vérification privé. Seul le Device de destination publie.
#[cfg(not(target_os = "none"))]
pub(crate) use crate::device::FileImage as Scratch;
#[cfg(target_os = "none")]
pub use native::*;
#[cfg(not(target_os = "none"))]
pub use std::io::{Error, ErrorKind, Read, Write};
#[cfg(target_os = "none")]
mod native {
    use crate::{Block, Device, Result, geometry::Geometry};
    use alloc::vec::Vec;
    use zeroize::Zeroizing;
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum ErrorKind {
        UnexpectedEof,
        Transport,
        Status(u64),
    }
    #[derive(Debug)]
    pub struct Error(pub ErrorKind);
    impl Error {
        pub fn kind(&self) -> ErrorKind {
            self.0
        }
    }
    impl core::fmt::Display for Error {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            write!(f, "{:?}", self.0)
        }
    }
    impl core::error::Error for Error {}
    pub trait Read {
        fn read(&mut self, out: &mut [u8]) -> core::result::Result<usize, Error>;
        fn read_exact(&mut self, mut out: &mut [u8]) -> core::result::Result<(), Error> {
            while !out.is_empty() {
                let n = self.read(out)?;
                if n == 0 {
                    return Err(Error(ErrorKind::UnexpectedEof));
                }
                out = out.get_mut(n..).ok_or(Error(ErrorKind::Transport))?;
            }
            Ok(())
        }
    }
    pub trait Write {
        fn write_all(&mut self, data: &[u8]) -> core::result::Result<(), Error>;
        fn flush(&mut self) -> core::result::Result<(), Error>;
    }
    pub(crate) struct Scratch(Zeroizing<Vec<Block>>);
    impl Scratch {
        pub(crate) fn create(blocks: u64, sector: u32) -> Result<Self> {
            Geometry::new(blocks, sector)?;
            let n = usize::try_from(blocks).map_err(|_| crate::Error::NoMemory)?;
            let mut pages = Vec::new();
            pages
                .try_reserve_exact(n)
                .map_err(|_| crate::Error::NoMemory)?;
            pages.resize(n, [0; 4096]);
            Ok(Self(Zeroizing::new(pages)))
        }
    }
    impl Device for Scratch {
        fn blocks(&self) -> u64 {
            self.0.len() as u64
        }
        fn read(&self, index: u64) -> Result<Block> {
            self.0
                .get(index as usize)
                .copied()
                .ok_or(crate::Error::Invalid("bloc temporaire"))
        }
        fn write(&mut self, index: u64, data: &Block) -> Result<()> {
            *self
                .0
                .get_mut(index as usize)
                .ok_or(crate::Error::Invalid("bloc temporaire"))? = *data;
            Ok(())
        }
        fn flush(&mut self) -> Result<()> {
            Err(crate::Error::Unavailable)
        }
    }
}
