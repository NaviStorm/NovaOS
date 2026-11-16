//! Le descripteur de qualification juxtapose deux fichiers hôte. L'Engine
//! monte exclusivement le premier. Ce type n'adresse que les douze pages du second.
use super::*;
pub(crate) struct AuthoritySpan<'a, 'b>(&'a IpcDevice<'b>);
pub(super) const VOLUME: u64 = 512;
const PAGES: u64 = 12;
impl<'a> IpcDevice<'a> {
    pub(crate) fn identity_authority(&self) -> Result<AuthoritySpan<'_, 'a>> {
        if crate::runtime::config(13) != 0x4e4133 || self.count != VOLUME + PAGES {
            return Err(Error::Unavailable);
        }
        Ok(AuthoritySpan(self))
    }
}
impl Device for AuthoritySpan<'_, '_> {
    fn blocks(&self) -> u64 {
        PAGES
    }
    fn read(&self, n: u64) -> Result<novafs::Block> {
        if n >= PAGES {
            return Err(Error::Unavailable);
        }
        let mut page = [0; 4096];
        self.0.transfer(VOLUME + n, &mut page, false)?;
        Ok(page)
    }
    fn write(&mut self, n: u64, data: &novafs::Block) -> Result<()> {
        if n >= PAGES {
            return Err(Error::Unavailable);
        }
        self.0
            .transfer(VOLUME + n, &mut *zeroize::Zeroizing::new(*data), true)
    }
    fn flush(&mut self) -> Result<()> {
        self.0.exchange(Block::flush)
    }
}
