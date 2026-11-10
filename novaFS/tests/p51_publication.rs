//! Le contrôle compte uniquement les lectures physiques de la charge témoin.
use novafs::device::{Persistence, SimDevice, SimFault};
use novafs::shared::SharedPool;
use novafs::{Block, Device, Error, Ids, Result};
use std::{cell::Cell, rc::Rc};

struct Compteur {
    dev: SimDevice,
    lectures: Rc<Cell<usize>>,
}

impl Device for Compteur {
    fn blocks(&self) -> u64 {
        self.dev.blocks()
    }
    fn read(&self, at: u64) -> Result<Block> {
        let bytes = self.dev.read(at)?;
        if bytes == [0xa7; 4096] {
            self.lectures.set(self.lectures.get() + 1);
        }
        Ok(bytes)
    }
    fn write(&mut self, at: u64, bytes: &Block) -> Result<()> {
        self.dev.write(at, bytes)
    }
    fn flush(&mut self) -> Result<()> {
        self.dev.flush()
    }
}

#[test]
fn donnees_propres_apres_f2_uniquement() -> Result<()> {
    for sector in [512, 4096] {
        let dev = SimDevice::new(2048, sector)?;
        let ctl = dev.control();
        let lectures = Rc::new(Cell::new(0));
        let disque = Compteur {
            dev,
            lectures: lectures.clone(),
        };
        let mut pool = SharedPool::format(disque, [4; 16])?;
        let o = pool.create(
            [5; 16],
            Ids {
                fs: [1; 16],
                lv: [2; 16],
            },
            256,
        )?;
        pool.create_file(o, "/f")?;
        pool.write_file(o, "/f", 0, &[0xa7; 16384])?;
        assert_eq!(
            lectures.replace(0),
            4,
            "la publication doit relire les quatre pages"
        );
        assert_eq!(pool.read_file(o, "/f", 0, 16384)?, [0xa7; 16384]);
        assert_eq!(lectures.get(), 0, "lecture immédiatement après F2");
        pool.reclaim_cache()?;
        assert_eq!(pool.read_file(o, "/f", 0, 16384)?, [0xa7; 16384]);
        assert!(
            lectures.replace(0) >= 4,
            "le cache peut être entièrement restitué"
        );
        let snap = pool.snapshot(o)?;
        let clone = pool.clone_snapshot(snap, [6; 16])?;
        pool.write_file(clone, "/f", 4095, &[0xbb; 2])?;
        assert_eq!(
            pool.read_file(clone, "/f", 4094, 4)?,
            [0xa7, 0xbb, 0xbb, 0xa7]
        );
        assert_eq!(pool.read_file(snap, "/f", 4094, 4)?, [0xa7; 4]);
        ctl.inject(SimFault::Flush {
            ordinal: 2,
            persist_newest: 0,
        })?;
        assert!(matches!(
            pool.write_file(o, "/f", 0, &[0xe2; 16384]),
            Err(Error::Publication { .. })
        ));
        assert!(matches!(
            pool.read_file(o, "/f", 0, 1),
            Err(Error::UnknownOutcome)
        ));
        let disque = pool.into_device();
        ctl.power_cut(Persistence::None)?;
        let pool = SharedPool::open(disque)?;
        assert_eq!(pool.read_file(o, "/f", 0, 16384)?, [0xa7; 16384]);
        assert_eq!(
            pool.read_file(clone, "/f", 4094, 4)?,
            [0xa7, 0xbb, 0xbb, 0xa7]
        );
    }
    Ok(())
}
