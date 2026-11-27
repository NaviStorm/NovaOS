use crate::{Error, Result};
use alloc::{string::String, vec::Vec};

/// Somme conservatrice des tailles demandées, anciennes réservations incluses.
pub const MAX_DECODE_HEAP: usize = 256 * 1024;
pub(crate) struct Budget {
    left: usize,
}
impl Budget {
    pub fn new(limit: usize) -> Self {
        Self {
            left: limit.min(MAX_DECODE_HEAP),
        }
    }
    fn charge(&mut self, n: usize) -> Result<()> {
        self.left = self.left.checked_sub(n).ok_or(Error::Limit)?;
        Ok(())
    }
    pub fn reserve<T>(&mut self, v: &mut Vec<T>, total: usize) -> Result<()> {
        if total > v.capacity() {
            self.charge(total.checked_mul(size_of::<T>()).ok_or(Error::Limit)?)?;
            v.try_reserve_exact(total - v.len())
                .map_err(|_| Error::NoMemory)?;
        }
        Ok(())
    }
    pub fn push<T>(&mut self, v: &mut Vec<T>, value: T) -> Result<()> {
        if v.len() == v.capacity() {
            self.reserve(v, (2 * v.len()).max(2))?;
        }
        v.push(value);
        Ok(())
    }
    pub fn text(&mut self, s: &str) -> Result<String> {
        self.charge(s.len())?;
        copy_str(s)
    }
    pub fn string_capacity(&mut self, n: usize) -> Result<String> {
        self.charge(n)?;
        let mut s = String::new();
        s.try_reserve_exact(n).map_err(|_| Error::NoMemory)?;
        Ok(s)
    }
}
pub(crate) fn copy_str(s: &str) -> Result<String> {
    let mut t = String::new();
    t.try_reserve_exact(s.len()).map_err(|_| Error::NoMemory)?;
    t.push_str(s);
    Ok(t)
}
