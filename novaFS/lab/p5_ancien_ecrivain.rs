//! Le banc compile exclusivement ce témoin contre les sources historiques b865c58.
use novafs::device::FileImage;
use novafs::{Block, Device, Error, Result, pool::Pool};
use std::fs::OpenOptions;

struct Observe(FileImage);
impl Device for Observe {
    fn blocks(&self) -> u64 {
        self.0.blocks()
    }
    fn read(&self, b: u64) -> Result<Block> {
        self.0.read(b)
    }
    fn write(&mut self, _: u64, _: &Block) -> Result<()> {
        panic!("écriture historique avant reconnaissance du format");
    }
    fn flush(&mut self) -> Result<()> {
        panic!("barrière historique sur un pool P5");
    }
}

#[test]
fn refus_du_pool_p5_avant_ecriture() -> Result<()> {
    let path = std::env::var_os("NOVAFS_P5_IMAGE").expect("image jetable du banc");
    let sector = std::env::var("NOVAFS_P5_SECTEUR").unwrap().parse().unwrap();
    let fd = OpenOptions::new().read(true).write(true).open(path)?;
    let size = fd.metadata()?.len() / 4096;
    let mut device = Observe(FileImage::from_file(fd, size, sector)?);
    assert!(matches!(Pool::import(&mut device), Err(Error::Corrupt(_))));
    assert!(matches!(Pool::format(&mut device), Err(Error::Invalid(_))));
    Ok(())
}
