//! La paire de manifestes qui a autorisé la compression doit rester celle
//! du montage contrôlé, même si une source d'import change de génération.
use novafs::device::{Event, SimDevice};
use novafs::shared::{ObjectKind, SharedPool};
use novafs::{Block, Compression, Device, Engine, Error, Ids, Result};
use std::cell::Cell;

struct SourceChangeante {
    courante: SimDevice,
    anterieure: SimDevice,
    lectures_ancre: Cell<u32>,
}

impl Device for SourceChangeante {
    fn blocks(&self) -> u64 {
        self.courante.blocks()
    }
    fn read(&self, b: u64) -> Result<Block> {
        if b == 0 {
            self.lectures_ancre.set(self.lectures_ancre.get() + 1);
        }
        if self.lectures_ancre.get() >= 2 {
            self.anterieure.read(b)
        } else {
            self.courante.read(b)
        }
    }
    fn write(&mut self, _: u64, _: &Block) -> Result<()> {
        Err(Error::ReadOnly)
    }
    fn flush(&mut self) -> Result<()> {
        Err(Error::ReadOnly)
    }
}

#[test]
fn import_refuse_un_predecesseur_valide_devenu_obsolete() -> Result<()> {
    for sector in [512, 4096] {
        let ids = Ids {
            fs: [1; 16],
            lv: [2; 16],
        };
        let dev = SimDevice::new(256, sector)?;
        let ctl = dev.control();
        let mut fs = Engine::format(dev, ids)?;
        let anterieure = ctl.snapshot().device();
        fs.create("/a")?;
        fs.create("/b")?;
        assert_eq!(fs.generation(), 3);
        let source = SourceChangeante {
            courante: fs.into_device(),
            anterieure,
            lectures_ancre: Cell::new(0),
        };
        let dev = SimDevice::new(2048, sector)?;
        let controles = dev.control();
        let mut destination = SharedPool::format_p6(dev, [77; 16])?;
        controles.trace(None, None);
        let result = destination.import_with_compression(
            [3; 16],
            ids,
            &source,
            ObjectKind::Origin,
            Compression::Lz4,
        );
        assert!(matches!(
            result,
            Err(Error::Corrupt(
                "prédécesseur modifié pendant la classification"
            ))
        ));
        assert_eq!(source.lectures_ancre.get(), 2);
        assert!(
            controles
                .events()
                .iter()
                .all(|e| !matches!(e, Event::WriteSubmitted(_) | Event::FlushSubmitted(_)))
        );
        assert!(
            SharedPool::open(destination.into_device())?
                .list()
                .is_empty()
        );
    }
    Ok(())
}
