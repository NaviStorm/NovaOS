//! Même client FIFO et même oracle sur les trois versions Darwin. Le code
//! de mesure se trouve à l'extérieur des bibliothèques mesurées.
#[path = "heap_meter.rs"]
mod heap_meter;
#[path = "thread_clock.rs"]
mod thread_clock;
use novafs::device::FileImage;
use novafs::{Block, Device, Error, Ids, Result};
use std::cell::Cell;
use std::os::fd::AsRawFd;
use std::rc::Rc;
use std::sync::atomic::Ordering;
use std::time::Instant;

const IMAGE: u64 = 8192;
const FILE: usize = 512 * 1024;
const IDS: Ids = Ids {
    fs: [8; 16],
    lv: [9; 16],
};
const PER_PASS: usize = 8;

#[derive(Clone, Copy, Default)]
struct Io {
    // Données applicatives, métadonnées du contenu, cartes de traduction, catalogue/sélecteur.
    reads: [u64; 4],
    writes: [u64; 4],
    read_ns: u64,
    write_ns: u64,
    syncs: u64,
    sync_ns: u64,
    barriers: [u64; 2],
}

struct Disk {
    image: FileImage,
    io: Rc<Cell<Io>>,
}

fn page_class(b: &Block) -> usize {
    match &b[..8] {
        b"NOVAP0FS" => 1,
        b"NP5FSROT" | b"NP5FSLEF" => 2,
        b"NP5POOLS" | b"NP5POOLC" | b"NP5POOLH" | b"NP0POOLC" | b"NP0POOLA" => 3,
        _ => 0,
    }
}

impl Device for Disk {
    fn blocks(&self) -> u64 {
        self.image.blocks()
    }
    fn read(&self, b: u64) -> Result<Block> {
        let now = Instant::now();
        let bytes = self.image.read(b)?;
        let elapsed = now.elapsed().as_nanos() as u64;
        let mut io = self.io.get();
        io.reads[page_class(&bytes)] += 1;
        io.read_ns += elapsed;
        self.io.set(io);
        Ok(bytes)
    }
    fn write(&mut self, b: u64, bytes: &Block) -> Result<()> {
        let now = Instant::now();
        self.image.write(b, bytes)?;
        let elapsed = now.elapsed().as_nanos() as u64;
        let mut io = self.io.get();
        io.writes[page_class(bytes)] += 1;
        io.write_ns += elapsed;
        self.io.set(io);
        Ok(())
    }
    fn flush(&mut self) -> Result<()> {
        let now = Instant::now();
        self.image.flush()?;
        let elapsed = now.elapsed().as_nanos() as u64;
        let mut io = self.io.get();
        io.syncs += 1;
        io.sync_ns += elapsed;
        io.barriers = [io.barriers[1], elapsed];
        self.io.set(io);
        Ok(())
    }
}

fn disk(sector: u32, nocache: bool, io: Rc<Cell<Io>>) -> Result<Disk> {
    let file = tempfile::tempfile()?;
    if nocache {
        unsafe extern "C" {
            fn fcntl(fd: i32, cmd: i32, ...) -> i32;
        }
        // Sûreté : descripteur vivant de ce fichier anonyme ; F_NOCACHE du SDK
        // Darwin reçoit un int et n'utilise aucun pointeur.
        if unsafe { fcntl(file.as_raw_fd(), 48, 1i32) } == -1 {
            return Err(std::io::Error::last_os_error().into());
        }
    }
    file.set_len(IMAGE * 4096)?;
    Ok(Disk {
        image: FileImage::from_file(file, IMAGE, sector)?,
        io,
    })
}

#[cfg(not(shared))]
struct Client(novafs::pool::PoolCoordinator<Disk>);
#[cfg(not(shared))]
impl Client {
    fn setup(d: Disk, cfg: &str, bytes: &[u8]) -> Result<Self> {
        assert_eq!(cfg, "origine");
        let mut p = novafs::pool::Pool::format(d)?;
        p.create_for([19; 16], IDS.lv, Some(IDS.fs), 4)?;
        let mut fs =
            novafs::Engine::format(p.view(IDS.lv, novafs::pool::Type::Preparing, IDS.fs)?, IDS)?;
        fs.create("/f")?;
        fs.create("/marque")?;
        for (j, chunk) in bytes.chunks(65536).enumerate() {
            fs.write("/f", j as u64 * 65536, chunk)?;
        }
        fs.unmount()?;
        p.mark_ready(IDS.lv, IDS.fs)?;
        Ok(Self(novafs::pool::PoolCoordinator::mount(p)?))
    }
    fn read(&self, offset: usize, size: usize) -> Result<Vec<u8>> {
        self.0.request_read(IDS.lv)?.read("/f", offset as u64, size)
    }
    fn write(&self, offset: usize, bytes: &[u8]) -> Result<u64> {
        let mut t = self.0.request_write(IDS.lv)?;
        let now = Instant::now();
        t.prepare("/f", offset as u64, bytes)?;
        let prepare = now.elapsed().as_nanos() as u64;
        t.commit()?;
        Ok(prepare)
    }
    fn clear(&self) -> Result<()> {
        self.0.reclaim_caches().map(|_| ())
    }
    fn verify_peers(&self, _: &[u8]) -> Result<()> {
        Ok(())
    }
}

#[cfg(shared)]
struct Client {
    coord: novafs::shared::SharedCoordinator<Disk>,
    target: u64,
    peers: Vec<u64>,
}
#[cfg(shared)]
impl Client {
    fn setup(d: Disk, cfg: &str, bytes: &[u8]) -> Result<Self> {
        let mut p = novafs::shared::SharedPool::format(d, [17; 16])?;
        let origin = p.create([19; 16], IDS, 1024)?;
        p.create_file(origin, "/f")?;
        p.create_file(origin, "/marque")?;
        for (j, chunk) in bytes.chunks(65536).enumerate() {
            p.write_file(origin, "/f", j as u64 * 65536, chunk)?;
        }
        let mut peers = Vec::new();
        let target = match cfg {
            "origine" => origin,
            "snapshot" => {
                peers.push(p.snapshot(origin)?);
                origin
            }
            "trois" => {
                for n in 1..=3 {
                    p.write_file(origin, "/marque", 0, &[n])?;
                    peers.push(p.snapshot(origin)?);
                }
                origin
            }
            "clone" => {
                let snap = p.snapshot(origin)?;
                peers.extend([origin, snap]);
                p.clone_snapshot(snap, [22; 16])?
            }
            _ => return Err(Error::Invalid("topologie de mesure")),
        };
        if cfg != "origine" {
            assert!(p.usage(target)?.shared > 0);
        }
        Ok(Self {
            coord: novafs::shared::SharedCoordinator::mount(p)?,
            target,
            peers,
        })
    }
    fn read(&self, offset: usize, size: usize) -> Result<Vec<u8>> {
        self.coord
            .request_read(self.target)?
            .read("/f", offset as u64, size)
    }
    fn write(&self, offset: usize, bytes: &[u8]) -> Result<u64> {
        let mut t = self.coord.request_write(self.target)?;
        let now = Instant::now();
        t.prepare("/f", offset as u64, bytes)?;
        let prepare = now.elapsed().as_nanos() as u64;
        t.commit()?;
        Ok(prepare)
    }
    fn clear(&self) -> Result<()> {
        self.coord.request_admin()?.publish(|p| p.reclaim_cache())
    }
    fn verify_peers(&self, bytes: &[u8]) -> Result<()> {
        for &id in &self.peers {
            for (j, chunk) in bytes.chunks(65536).enumerate() {
                assert_eq!(
                    self.coord
                        .request_read(id)?
                        .read("/f", j as u64 * 65536, chunk.len())?,
                    chunk
                );
            }
        }
        Ok(())
    }
}

fn positions(size: usize, random: bool, seed: u64) -> Vec<usize> {
    let mut slots: Vec<_> = (0..FILE / size).collect();
    if random {
        let mut x = seed;
        for i in (1..slots.len()).rev() {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            slots.swap(i, (x % (i as u64 + 1)) as usize);
        }
    }
    slots.into_iter().take(PER_PASS).map(|i| i * size).collect()
}

struct Before {
    wall: Instant,
    cpu: u64,
    alloc: usize,
    io: Io,
}
impl Before {
    fn start(io: Io) -> Self {
        Self {
            io,
            alloc: heap_meter::ALLOCATIONS.load(Ordering::Relaxed),
            cpu: thread_clock::thread_cpu_ns(),
            wall: Instant::now(),
        }
    }
    fn finish(self, io: Io) -> [u64; 17] {
        let wall = self.wall.elapsed().as_nanos() as u64;
        let cpu = thread_clock::thread_cpu_ns() - self.cpu;
        let allocations = (heap_meter::ALLOCATIONS.load(Ordering::Relaxed) - self.alloc) as u64;
        let reads = std::array::from_fn::<_, 4, _>(|i| io.reads[i] - self.io.reads[i]);
        let writes = std::array::from_fn::<_, 4, _>(|i| io.writes[i] - self.io.writes[i]);
        let syncs = io.syncs - self.io.syncs;
        let barriers = if syncs == 2 { io.barriers } else { [0, 0] };
        [
            wall,
            cpu,
            allocations,
            reads[0],
            reads[1],
            reads[2],
            reads[3],
            writes[0],
            writes[1],
            writes[2],
            writes[3],
            io.read_ns - self.io.read_ns,
            io.write_ns - self.io.write_ns,
            syncs,
            io.sync_ns - self.io.sync_ns,
            barriers[0],
            barriers[1],
        ]
    }
}

fn precondition(
    c: &Client,
    cache: &str,
    offsets: &[usize],
    size: usize,
    oracle: &[u8],
) -> Result<()> {
    c.clear()?;
    // Installer le montage de contenu avant mesure. Une lecture de zéro octet
    // laisse les pages de données du fichier hors du cache du moteur.
    assert!(c.read(0, 0)?.is_empty());
    if cache == "chaud" {
        for &pos in offsets {
            assert_eq!(c.read(pos, size)?, oracle[pos..pos + size]);
        }
    }
    Ok(())
}

fn panel(
    sector: u32,
    cache: &str,
    config: &str,
    order: &str,
    size: usize,
    seed: u64,
) -> Result<()> {
    let io = Rc::new(Cell::new(Io::default()));
    let initial: Vec<_> = (0..FILE)
        .map(|i| ((i + i / 4096 * 17) % 251 + 1) as u8)
        .collect();
    let mut expected = initial.clone();
    let c = Client::setup(
        disk(sector, cache == "nocache", io.clone())?,
        config,
        &initial,
    )?;
    let offsets = positions(size, order == "alea", seed);
    let prefix = format!("{config},{order},{size}");
    let emit = |kind: &str, n: usize, pos: usize, prepare: u64, s: [u64; 17]| {
        assert_eq!(s[13], if kind == "lecture" { 0 } else { 2 });
        print!("{prefix},{kind},{n},{pos},{prepare}");
        for v in s {
            print!(",{v}");
        }
        println!();
    };
    for pass in 0..4 {
        precondition(&c, cache, &offsets, size, &expected)?;
        for (i, &pos) in offsets.iter().enumerate() {
            let time = Before::start(io.get());
            let bytes = c.read(pos, size)?;
            let s = time.finish(io.get());
            assert_eq!(bytes, expected[pos..pos + size]);
            emit("lecture", pass * offsets.len() + i, pos, 0, s);
        }
    }
    for pass in 0..2u8 {
        precondition(&c, cache, &offsets, size, &expected)?;
        for (j, &pos) in offsets.iter().enumerate() {
            let block = &mut expected[pos..pos + size];
            for b in block.iter_mut() {
                *b ^= 0x65 + pass;
            }
            let time = Before::start(io.get());
            let prepare = c.write(pos, block)?;
            let s = time.finish(io.get());
            let kind = if pass == 0 && config != "origine" {
                "divergence"
            } else if pass == 0 {
                "exclusive"
            } else {
                "reecriture"
            };
            emit(kind, j, pos, prepare, s);
        }
    }
    for (j, chunk) in expected.chunks(65536).enumerate() {
        assert_eq!(c.read(j * 65536, chunk.len())?, chunk);
    }
    c.verify_peers(&initial)?;
    Ok(())
}

fn main() -> Result<()> {
    let a: Vec<_> = std::env::args().skip(1).collect();
    if a == ["horloge"] {
        println!("{}", thread_clock::calibrate());
        return Ok(());
    }
    let sector: u32 = a[0].parse().unwrap();
    let cache = a[1].clone();
    let rep: u64 = a[2].parse().unwrap();
    let specific = a.get(3).cloned();
    let worker = std::thread::Builder::new()
        .stack_size(2 * 1024 * 1024)
        .spawn(move || -> Result<()> {
            #[cfg(shared)]
            let configs = &["origine", "snapshot", "trois", "clone"][..];
            #[cfg(not(shared))]
            let configs = &["origine"][..];
            for idx in 0..configs.len() {
                let cfg = configs[(idx + rep as usize) % configs.len()];
                if specific.as_ref().is_some_and(|s| s != cfg) {
                    continue;
                }
                for &size in &[4096, 16384, 65536] {
                    for order in ["seq", "alea"] {
                        panel(sector, &cache, cfg, order, size, 0x40b39033 ^ rep)?;
                    }
                }
            }
            eprintln!(
                "memory {} {}",
                heap_meter::PEAK.load(Ordering::Relaxed),
                heap_meter::current_stack_size()
            );
            Ok(())
        })?;
    worker.join().expect("terminaison du banc")
}
