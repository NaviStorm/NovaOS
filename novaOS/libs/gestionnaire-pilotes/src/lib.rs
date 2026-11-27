#![no_std]
#![forbid(unsafe_code)]
//! Simulation hors ligne. Les descriptions, autorités et confirmations sont fournies par le banc.
extern crate alloc;
pub mod inventory;
pub mod ipc;
pub mod lifecycle;
pub mod manager;
pub mod resources;
pub use novaos_catalogue as catalogue;
pub type Result<T> = core::result::Result<T, Error>;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    Format,
    Stale,
    Conflict,
    Denied,
    Full,
    Incomplete,
    Exhausted,
    Transition,
    Uncertain,
    Memory,
    Catalogue(catalogue::Error),
}
fn next(n: u64) -> Result<u64> {
    n.checked_add(1).ok_or(Error::Exhausted)
}
fn push<T>(v: &mut alloc::vec::Vec<T>, item: T, max: usize) -> Result<()> {
    if v.len() >= max {
        return Err(Error::Full);
    }
    v.try_reserve_exact(1).map_err(|_| Error::Memory)?;
    v.push(item);
    Ok(())
}

pub mod rights;

mod plan_wire;
