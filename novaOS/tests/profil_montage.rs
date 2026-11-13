//! Mesure du montage P9 en lecture seule sur un fichier ordinaire du laboratoire.
//! Le chronomètre autour du backend inclut le coût de mesure ; aucune purge du cache hôte.
use novafs::{Block, Device, Engine, Ids, Mode, Result, device::FileImage};
use std::{cell::Cell, fs::File, time::Instant};

struct Mesure {
    image: FileImage,
    lectures: Cell<u64>,
    io_ns: Cell<u128>,
}
impl Device for &Mesure {
    fn blocks(&self) -> u64 {
        self.image.blocks()
    }
    fn read(&self, block: u64) -> Result<Block> {
        let debut = Instant::now();
        let r = self.image.read(block);
        self.io_ns
            .set(self.io_ns.get() + debut.elapsed().as_nanos());
        self.lectures.set(self.lectures.get() + 1);
        r
    }
    fn write(&mut self, _: u64, _: &Block) -> Result<()> {
        Err(novafs::Error::ReadOnly)
    }
    fn flush(&mut self) -> Result<()> {
        Err(novafs::Error::ReadOnly)
    }
}
fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let path = args.next().ok_or("image requise")?;
    let mib: u64 = args
        .next()
        .and_then(|v| v.to_str()?.parse().ok())
        .ok_or("taille requise")?;
    let sector: u32 = args
        .next()
        .and_then(|v| v.to_str()?.parse().ok())
        .ok_or("secteur requis")?;
    if args.next().is_some() || !matches!(mib, 16 | 100 | 1024) || !matches!(sector, 512 | 4096) {
        return Err("profil dense non reconnu".into());
    }
    let file = File::open(path)?;
    let size = mib * 1048576;
    let image = FileImage::from_file(file, size * 2 / 4096, sector)?;
    let m = Mesure {
        image,
        lectures: Cell::default(),
        io_ns: Cell::default(),
    };
    let start = Instant::now();
    let fs = Engine::mount_current(
        &m,
        Ids {
            fs: [0x31; 16],
            lv: [0x62; 16],
        },
        Mode::ReadOnly,
    )?;
    let ns = start.elapsed().as_nanos();
    let reads = m.lectures.get();
    let io = m.io_ns.get();
    assert_eq!(fs.stat(&format!("/f{size}"))?.size, size);
    println!(
        "{{\"mio\":{mib},\"secteur\":{sector},\"montage_ns\":{ns},\"backend_ns\":{io},\"lectures_4k\":{reads}}}"
    );
    Ok(())
}
