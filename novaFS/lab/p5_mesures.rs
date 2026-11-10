//! Même charge de lecture/écriture sur le coordinateur de référence et le pool P5.
//! p5-mesures.py compile ce banc face au commit P4.1 puis face à la version courante.
#[path = "heap_meter.rs"]
mod heap_meter;
use novafs::device::FileImage;
use novafs::{Block, Device, Ids, Result};
use std::cell::Cell;
use std::collections::BTreeMap;
#[cfg(p5)]
use std::io::{Seek, SeekFrom};
use std::rc::Rc;
use std::sync::atomic::Ordering;
use std::time::Instant;

const DISK: u64 = 8192;
const IDS: Ids = Ids {
    fs: [8; 16],
    lv: [9; 16],
};
const IO: usize = 16 * 1024;
const N: u64 = 128;
const STACK: usize = 2 * 1024 * 1024;

#[derive(Default)]
struct Counts {
    writes: Cell<u64>,
    flush: Cell<u64>,
    time: Cell<u64>,
    last_sync: Cell<[u64; 2]>,
}
struct Measured {
    device: FileImage,
    counts: Rc<Counts>,
}
impl Device for Measured {
    fn blocks(&self) -> u64 {
        self.device.blocks()
    }
    fn read(&self, b: u64) -> Result<Block> {
        self.device.read(b)
    }
    fn write(&mut self, b: u64, data: &Block) -> Result<()> {
        self.device.write(b, data)?;
        self.counts.writes.set(self.counts.writes.get() + 1);
        Ok(())
    }
    fn flush(&mut self) -> Result<()> {
        let start = Instant::now();
        self.device.flush()?;
        let elapsed = start.elapsed().as_nanos() as u64;
        self.counts.time.set(self.counts.time.get() + elapsed);
        self.counts
            .last_sync
            .set([self.counts.last_sync.get()[1], elapsed]);
        self.counts.flush.set(self.counts.flush.get() + 1);
        Ok(())
    }
}
struct Lab {
    counts: Rc<Counts>,
    samples: BTreeMap<&'static str, Vec<[u64; 7]>>,
}
impl Lab {
    fn new() -> Self {
        Self {
            counts: Rc::default(),
            samples: BTreeMap::new(),
        }
    }
    fn device(&self, sector: u32) -> Result<Measured> {
        Ok(Measured {
            device: FileImage::create(DISK, sector)?,
            counts: self.counts.clone(),
        })
    }
    fn observe<T>(
        &mut self,
        op: &'static str,
        bytes: u64,
        f: impl FnOnce() -> Result<T>,
    ) -> Result<T> {
        let (writes, flush, time) = (
            self.counts.writes.get(),
            self.counts.flush.get(),
            self.counts.time.get(),
        );
        let start = Instant::now();
        let result = f()?;
        let ns = start.elapsed().as_nanos() as u64;
        let syncs = self.counts.flush.get() - flush;
        let [f1, f2] = if syncs >= 2 {
            self.counts.last_sync.get()
        } else {
            [0, 0]
        };
        self.samples.entry(op).or_default().push([
            ns,
            bytes,
            self.counts.writes.get() - writes,
            syncs,
            self.counts.time.get() - time,
            f1,
            f2,
        ]);
        Ok(result)
    }
}

#[cfg(not(p5))]
fn campaign(lab: &mut Lab, sector: u32) -> Result<()> {
    use novafs::Engine;
    use novafs::pool::{Pool, PoolCoordinator, Type};
    let mut pool = Pool::format(lab.device(sector)?)?;
    pool.create(IDS.lv, Some(IDS.fs), 1)?;
    {
        let mut fs = Engine::format(pool.view(IDS.lv, Type::Preparing, IDS.fs)?, IDS)?;
        fs.create("/f")?;
        for j in 0..4 {
            fs.write("/f", j * IO as u64, &[0x36; IO])?;
        }
    }
    pool.mark_ready(IDS.lv, IDS.fs)?;
    let c = PoolCoordinator::mount(pool)?;
    for j in 0..N {
        let data = [j as u8; IO];
        let offset = (j % 4) * IO as u64;
        lab.observe("write", IO as u64, || {
            let mut ticket = c.request_write(IDS.lv)?;
            ticket.prepare("/f", offset, &data)?;
            ticket.commit()
        })?;
        lab.observe("read", IO as u64, || {
            assert_eq!(c.request_read(IDS.lv)?.read("/f", offset, IO)?, data);
            Ok(())
        })?;
    }
    Ok(())
}

#[cfg(p5)]
fn campaign(lab: &mut Lab, sector: u32) -> Result<()> {
    use novafs::shared::{SharedCoordinator, SharedPool};
    use novafs::shared_export::{export_view, restore_view};
    let mut pool = SharedPool::format(lab.device(sector)?, [17; 16])?;
    let id = pool.create([19; 16], IDS, 256)?;
    pool.create_file(id, "/f")?;
    for j in 0..4 {
        pool.write_file(id, "/f", j * IO as u64, &[0x36; IO])?;
    }
    let coordinator = SharedCoordinator::mount(pool)?;
    for j in 0..N {
        let data = [j as u8; IO];
        let offset = (j % 4) * IO as u64;
        lab.observe("write", IO as u64, || {
            let mut ticket = coordinator.request_write(id)?;
            ticket.prepare("/f", offset, &data)?;
            ticket.commit()
        })?;
        lab.observe("read", IO as u64, || {
            assert_eq!(coordinator.request_read(id)?.read("/f", offset, IO)?, data);
            Ok(())
        })?;
    }
    let mut pool = coordinator.into_pool()?;
    for j in 0..N {
        let snap = lab.observe("snapshot", 0, || pool.snapshot(id))?;
        let clone = lab.observe("clone", 0, || pool.clone_snapshot(snap, [22; 16]))?;
        let bytes = [(j + 1) as u8; IO];
        lab.observe("write_cow", IO as u64, || {
            pool.write_file(clone, "/f", 0, &bytes)
        })?;
        assert_eq!(pool.read_file(snap, "/f", 0, IO)?, [124; IO]);
        if j % 8 == 0 {
            let mut file = tempfile::tempfile()?;
            lab.observe("export", 4 * IO as u64, || {
                export_view(&pool, clone, sector, &mut file)
            })?;
            file.seek(SeekFrom::Start(0))?;
            let mut dest = SharedPool::format(lab.device(sector)?, [55; 16])?;
            let new = lab.observe("restore", 4 * IO as u64, || {
                restore_view(&mut dest, &mut file, [13; 16])
            })?;
            assert_eq!(dest.read_file(new, "/f", 0, IO)?, bytes);
        }
        lab.observe("drop_snapshot", 0, || pool.remove(snap))?;
        lab.observe("drop_clone", 0, || pool.remove(clone))?;
        if j % 8 == 0 {
            let disk = pool.into_device();
            let p = lab.observe("check", 0, || SharedPool::open(disk))?;
            pool = lab.observe("remount_advance", 0, || {
                let mut mounted = SharedPool::open(p.into_device())?;
                mounted.advance()?;
                Ok(mounted)
            })?;
        }
    }
    Ok(())
}
fn main() -> Result<()> {
    let sector: u32 = std::env::args().nth(1).unwrap().parse().unwrap();
    let main_stack = heap_meter::current_stack_size();
    let worker = std::thread::Builder::new()
        .stack_size(STACK)
        .spawn(move || -> Result<()> {
            let mut lab = Lab::new();
            let stack = heap_meter::current_stack_size();
            campaign(&mut lab, sector)?;
            let heap = heap_meter::PEAK.load(Ordering::Relaxed);
            for (label, runs) in &lab.samples {
                for &[time, bytes, writes, flush, ns, f1, f2] in runs {
                    println!("sample {label} {time} {bytes} {writes} {flush} {ns} {f1} {f2}");
                }
            }
            println!("memory {heap} {stack} {main_stack}");
            Ok(())
        })?;
    worker.join().expect("thread du banc")
}
