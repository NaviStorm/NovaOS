use crate::error::{Error, Result};
use alloc::boxed::Box;
use no_std_io2::io::{BufRead, Write};

pub trait Input: BufRead {
    fn close(self: Box<Self>) -> Result<i32> {
        Ok(0)
    }
}

pub trait Output: Write {
    fn close(mut self: Box<Self>) -> Result<i32> {
        self.flush()?;
        Ok(0)
    }
}

/// Only operations explicitly supplied by the adapter are available to scripts.
pub trait Host {
    fn input(&self, _name: &str, _pipe: bool) -> Result<Box<dyn Input>> {
        Err(Error::runtime("input redirection unavailable"))
    }
    fn output(&self, _name: &str, _append: bool, _pipe: bool) -> Result<Box<dyn Output>> {
        Err(Error::runtime("output redirection unavailable"))
    }
    fn system(&self, _command: &str) -> Result<i32> {
        Err(Error::runtime("system() unavailable"))
    }
    fn seconds(&self) -> Result<u64> {
        Err(Error::runtime("clock unavailable"))
    }
    fn pid(&self) -> Option<u32> {
        None
    }
}

pub struct Restricted;
impl Host for Restricted {}
