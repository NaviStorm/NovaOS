//! Microbanc interversions. Compiler ce même fichier contre les deux bibliothèques.
use novafs::{Block, Device, Engine, Ids, Result, device::FileImage};
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::time::Instant;

#[derive(Clone, Copy, Default)]
struct Calls {
    count: u64,
    bytes: u64,
}
thread_local! {
    static SAMPLE: Cell<Option<Calls>> = const { Cell::new(None) };
}
fn account(size: usize) {
    let _ = SAMPLE.try_with(|s| {
        if let Some(mut c) = s.get() {
            c.count += 1;
            c.bytes += size as u64;
            s.set(Some(c));
        }
    });
}
struct Meter;
#[global_allocator]
static ALLOC: Meter = Meter;
// Sûreté : System conserve seul la propriété et le contrat de chaque bloc.
// Le compteur TLS à initialisation constante ne fait aucune allocation.
unsafe impl GlobalAlloc for Meter {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        account(l.size());
        // Sûreté : transfert du Layout inchangé à l'allocateur système.
        unsafe { System.alloc(l) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        // Sûreté : le pointeur et le Layout sont ceux du bloc System vivant.
        unsafe { System.dealloc(p, l) }
    }
    unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 {
        account(l.size());
        // Sûreté : System initialise le bloc demandé avec ce même Layout.
        unsafe { System.alloc_zeroed(l) }
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        account(n);
        // Sûreté : transmission du bloc initial et de la taille demandée.
        unsafe { System.realloc(p, l, n) }
    }
}
struct Disk<'a> {
    image: FileImage,
    reads: &'a Cell<usize>,
}
impl Device for Disk<'_> {
    fn blocks(&self) -> u64 {
        self.image.blocks()
    }
    fn read(&self, b: u64) -> Result<Block> {
        self.reads.set(self.reads.get() + 1);
        self.image.read(b)
    }
    fn write(&mut self, b: u64, data: &Block) -> Result<()> {
        self.image.write(b, data)
    }
    fn flush(&mut self) -> Result<()> {
        self.image.flush()
    }
}

fn measure(name: &str, reads: &Cell<usize>, mut action: impl FnMut() -> Result<()>) -> Result<()> {
    const COUNT: u32 = 128;
    action()?;
    for series in 0..5 {
        reads.set(0);
        SAMPLE.set(Some(Calls::default()));
        let start = Instant::now();
        for _ in 0..COUNT {
            action()?;
        }
        let ns = start.elapsed().as_nanos();
        let calls = SAMPLE.take().unwrap();
        println!(
            "extent phase={name} serie={series} operations={COUNT} blocs_lus={} appels_alloc={} octets_demandes={} ns={ns}",
            reads.get(),
            calls.count,
            calls.bytes
        );
    }
    Ok(())
}

#[test]
#[ignore = "mesure comparative P3/P4, voir le rapport P4"]
fn extent_contigu() -> Result<()> {
    let reads = Cell::new(0);
    for sector in [512, 4096] {
        println!("secteur={sector}");
        let ids = Ids {
            fs: [0x53; 16],
            lv: [0x24; 16],
        };
        let mut fs = Engine::format(
            Disk {
                image: FileImage::create(384, sector)?,
                reads: &reads,
            },
            ids,
        )?;
        fs.create("/f")?;
        let data: Vec<u8> = (0..65536).map(|i| (i % 251) as u8).collect();
        fs.write("/f", 0, &data)?;
        assert_eq!(fs.extent_count("/f")?, 1);
        measure("lecture_64k", &reads, || {
            assert_eq!(fs.read("/f", 0, 65536)?, data);
            Ok(())
        })?;
        let receipt = fs.receipt();
        measure("preparer_64k", &reads, || {
            let plan = fs.prepare_write("/f", 0, &data)?;
            assert_eq!(plan.cost().data_blocks, 16);
            plan.cancel();
            Ok(())
        })?;
        assert_eq!(fs.receipt(), receipt);
        fs.verify()?;
    }
    Ok(())
}
