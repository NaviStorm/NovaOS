//! Vue de blocs NovaFS sur la session sectorielle IPC. Toute erreur de transport ferme cette vue.
use crate::medium::Block;
use crate::runtime::{BAD_ARGUMENT, NO_MEMORY, REFUSED};
use core::cell::{Cell, RefCell};
use novafs::{Device, Error, Result, geometry::Geometry};

/// Traduction à la frontière de la session de diagnostic v1.
pub(crate) fn status(error: Error) -> u64 {
    match error {
        Error::Io(novafs::io::Error(novafs::io::ErrorKind::Status(status))) => status,
        Error::Invalid(_) => BAD_ARGUMENT,
        Error::NoMemory => NO_MEMORY,
        _ => REFUSED,
    }
}

pub(crate) struct IpcDevice<'a> {
    session: RefCell<&'a mut Block>,
    geometry: Geometry,
    count: u64,
    failed: Cell<bool>,
    qualification: crate::qualification::FaultProbe,
    progress: Cell<Option<(u64, u64, u64)>>,
}
impl<'a> IpcDevice<'a> {
    pub(crate) fn new(session: &'a mut Block, size: u32, sectors: u64) -> Result<Self> {
        if !matches!(size, 512 | 4096) {
            return Err(Error::Invalid("secteur IPC"));
        }
        let ratio = 4096 / u64::from(size);
        if sectors % ratio != 0 {
            return Err(Error::Invalid("fin de support non alignée"));
        }
        let count = sectors / ratio;
        Ok(Self {
            session: RefCell::new(session),
            geometry: Geometry::new(count, size)?,
            count,
            failed: Cell::new(false),
            qualification: Default::default(),
            progress: Cell::new(None),
        })
    }
    pub(crate) fn close(&self) -> Result<()> {
        self.exchange(Block::close)
    }
    pub(crate) fn report_mount<T>(&self, port: u64, tag: u64, f: impl FnOnce() -> T) -> T {
        self.progress.set(Some((port, tag, 0)));
        let r = f();
        self.progress.set(None);
        r
    }
    pub(crate) fn start_probe(&self) {
        self.qualification.arm()
    }
    fn exchange<T>(&self, f: impl FnOnce(&mut Block) -> core::result::Result<T, u64>) -> Result<T> {
        if self.failed.get() {
            return Err(Error::Unavailable);
        }
        let mut session = self.session.try_borrow_mut().map_err(|_| Error::Busy)?;
        f(&mut session).map_err(|status| {
            self.failed.set(true);
            Error::Io(novafs::io::Error(novafs::io::ErrorKind::Status(status)))
        })
    }
    fn transfer(&self, b: u64, page: &mut novafs::Block, write: bool) -> Result<()> {
        let (first, _) = self.geometry.sectors(b, 1)?;
        let metadata = page.starts_with(b"NOVAP0FS");
        self.exchange(|s| {
            for (i, sector) in page
                .chunks_exact_mut(self.geometry.sector_size() as usize)
                .enumerate()
            {
                s.transfer(
                    sector,
                    first.checked_add(i as u64).ok_or(BAD_ARGUMENT)?,
                    write,
                )?;
                self.qualification.sector(b, write, metadata);
            }
            Ok(())
        })
    }
}
impl Device for &IpcDevice<'_> {
    fn blocks(&self) -> u64 {
        self.count
    }
    fn read(&self, block: u64) -> Result<novafs::Block> {
        let mut page = [0; 4096];
        let cap = crate::runtime::config(10);
        if cap == 0 {
            self.transfer(block, &mut page, false)?;
        } else {
            let (first, _) = self.geometry.sectors(block, 1)?;
            self.exchange(|s| s.read_page(first, cap, &mut page))?;
        }
        if let Some((port, tag, n)) = self.progress.get() {
            let n = n
                .checked_add(1)
                .ok_or(Error::Limit("compteur de montage"))?;
            if n.is_multiple_of(crate::protocol::MOUNT_STEP) {
                let f = crate::protocol::mount_progress(n);
                crate::runtime::send(port, tag, &f).map_err(|status| {
                    Error::Io(novafs::io::Error(novafs::io::ErrorKind::Status(status)))
                })?;
            }
            self.progress.set(Some((port, tag, n)));
        }
        Ok(page)
    }
    fn write(&mut self, block: u64, data: &novafs::Block) -> Result<()> {
        let cap = crate::runtime::config(10);
        if cap == 0 {
            return self.transfer(block, &mut *zeroize::Zeroizing::new(*data), true);
        }
        let (first, _) = self.geometry.sectors(block, 1)?;
        self.exchange(|s| s.write_page(first, cap, data))?;
        self.qualification
            .sector(block, true, data.starts_with(b"NOVAP0FS"));
        Ok(())
    }
    fn flush(&mut self) -> Result<()> {
        self.qualification.before_flush();
        self.exchange(Block::flush)?;
        self.qualification.after_flush();
        Ok(())
    }
}
