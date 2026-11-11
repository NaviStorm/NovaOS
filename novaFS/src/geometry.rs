use crate::{BLOCK_BYTES, Error, MAX_BLOCKS, Result};
const BLOCK: u64 = BLOCK_BYTES as u64;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Geometry {
    pub(crate) blocks: u64,
    pub(crate) sector: u32,
}

impl Geometry {
    pub fn new(blocks: u64, sector: u32) -> Result<Self> {
        if !matches!(sector, 512 | 4096) || !(1..=MAX_BLOCKS).contains(&blocks) {
            return Err(Error::Invalid("géométrie de l’image"));
        }
        Ok(Self { blocks, sector })
    }

    pub fn sector_size(self) -> u32 {
        self.sector
    }

    /// LBA natif et nombre de secteurs, pour une plage exprimée en blocs NovaFS.
    pub fn sectors(self, first: u64, count: u64) -> Result<(u64, u64)> {
        if count == 0 || first.checked_add(count).is_none_or(|end| end > self.blocks) {
            return Err(Error::Invalid("plage hors image"));
        }
        let ratio = BLOCK / u64::from(self.sector);
        Ok((first * ratio, count * ratio))
    }

    #[cfg(not(target_os = "none"))]
    pub(crate) fn offset(self, index: u64) -> Result<u64> {
        Ok(self.sectors(index, 1)?.0 * u64::from(self.sector))
    }
}
