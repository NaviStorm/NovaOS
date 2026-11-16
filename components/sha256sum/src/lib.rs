#![no_std]
extern crate alloc;

use alloc::vec;
pub use sha2::{Digest, Sha256};

pub trait Source {
    type Stream;
    type Error;
    fn open(&mut self, name: &[u8]) -> Result<Self::Stream, Self::Error>;
    fn read(&mut self, stream: &mut Self::Stream, buf: &mut [u8]) -> Result<usize, Self::Error>;
    const BUFFER: usize;
}

pub fn digest<S: Source>(io: &mut S, name: &[u8]) -> Result<[u8; 32], S::Error> {
    let mut reader = io.open(name)?;
    let mut buffer = vec![0; S::BUFFER];
    let mut hash = Sha256::new();
    loop {
        let count = io.read(&mut reader, &mut buffer)?;
        if count == 0 {
            return Ok(hash.finalize().into());
        }
        hash.update(&buffer[..count]);
    }
}

mod check;
mod command;
mod manifest;
pub mod options;
pub use command::{Platform, run};
