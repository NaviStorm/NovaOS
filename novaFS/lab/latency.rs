//! Relecture des cinq états du fichier /volume avec la même charge utile que P4.
//! Binaire du laboratoire greffé sur chaque extraction par measure-latency.py.
use novafs::device::FileImage;
use novafs::{Block, Device, Engine, Error, Ids, Mode, Result};
use std::cell::Cell;
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, Write};
use std::os::fd::AsRawFd;
use std::path::Path;
use std::sync::atomic::Ordering;
use std::time::Instant;

mod heap_meter;
mod thread_clock;
use thread_clock::thread_cpu_ns as cpu_ns;

const PAGES: u64 = 24 * 1024;
const CHUNK: usize = 64 * 1024;
const COUNT: usize = 512;
const REGION: usize = 8; // 512 Kio tiennent dans le cache NovaFS.
const STACK: usize = 2 * 1024 * 1024;
const IDS: Ids = Ids {
    fs: [11; 16],
    lv: [29; 16],
};

unsafe extern "C" {
    fn fcntl(fd: i32, operation: i32, ...) -> i32;
}

fn bypass(file: &File) -> io::Result<()> {
    // Sûreté : le descripteur emprunté reste ouvert, F_NOCACHE (48, SDK
    // Darwin) reçoit un int à 1 et ne conserve aucune référence au processus.
    if unsafe { fcntl(file.as_raw_fd(), 48, 1i32) } == -1 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

// Les cinq images contiennent exactement l'état précédant la passe de lecture
// séquentielle correspondante du banc P4, y compris les CoW des séries passées.
fn prepare(directory: &Path) -> Result<()> {
    let live = tempfile::NamedTempFile::new_in(directory)?;
    live.as_file().set_len(PAGES * 4096)?;
    let dev = FileImage::from_file(live.as_file().try_clone()?, PAGES, 4096)?;
    let mut e = Engine::format(dev, IDS)?;
    e.create("/volume")?;
    let mut transfer = [0; CHUNK];
    for i in 0..32 {
        transfer.fill(i as u8);
        e.write("/volume", (i * CHUNK) as u64, &transfer)?;
        assert_eq!(e.read("/volume", (i * CHUNK) as u64, CHUNK)?, transfer);
    }
    for series in 1..=5 {
        for i in 0..COUNT {
            transfer.fill((i ^ series) as u8);
            e.write("/volume", (i * CHUNK) as u64, &transfer)?;
        }
        let path = directory.join(format!("serie-{series}.img"));
        let mut frozen = OpenOptions::new().write(true).create_new(true).open(path)?;
        let copied = io::copy(&mut File::open(live.path())?, &mut frozen)?;
        assert_eq!(copied, PAGES * 4096);
        frozen.sync_all()?;
        for i in 0..COUNT {
            let bytes = e.read("/volume", (i * CHUNK) as u64, CHUNK)?;
            assert_eq!(bytes.len(), CHUNK);
            assert!(bytes.iter().all(|&b| b == (i ^ series) as u8));
        }
        let addr = |i: usize| {
            (((i * 71 + series * 311) % (COUNT * CHUNK / 4096 - 1)) * 4096
                + if i % 2 == 1 { 23 } else { 0 }) as u64
        };
        for i in 0..256 {
            let address = addr(i);
            let bytes = e.read("/volume", address, 4096)?;
            assert_eq!(bytes.len(), 4096);
            for (j, b) in bytes.into_iter().enumerate() {
                assert_eq!(b, (((address as usize + j) / CHUNK) ^ series) as u8);
            }
        }
        for i in 0..256 {
            e.write("/volume", addr(i), &[(i ^ 0x55) as u8; 4096])?;
        }
        for i in 0..256 {
            assert_eq!(e.read("/volume", addr(i), 4096)?, [(i ^ 0x55) as u8; 4096]);
        }
    }
    e.verify()?;
    e.unmount()?;
    Ok(())
}

#[derive(Default)]
struct IoStats {
    n: Cell<u64>,
    ns: Cell<u64>,
    active: Cell<bool>,
    mounted: Cell<bool>,
}

struct ReadDevice<'a> {
    file: FileImage,
    stats: &'a IoStats,
    source_for_mount: Option<FileImage>,
}

impl Device for ReadDevice<'_> {
    fn blocks(&self) -> u64 {
        self.file.blocks()
    }
    fn read(&self, block: u64) -> Result<Block> {
        if !self.stats.mounted.get()
            && let Some(source) = &self.source_for_mount
        {
            return source.read(block);
        }
        if !self.stats.active.get() {
            return self.file.read(block);
        }
        let start = Instant::now();
        let data = self.file.read(block);
        self.stats
            .ns
            .set(self.stats.ns.get() + start.elapsed().as_nanos() as u64);
        self.stats.n.set(self.stats.n.get() + 1);
        data
    }
    fn write(&mut self, _: u64, _: &Block) -> Result<()> {
        Err(Error::ReadOnly)
    }
    fn flush(&mut self) -> Result<()> {
        Err(Error::ReadOnly)
    }
}

struct Sample {
    full: u64,
    cpu: u64,
    read: u64,
    check_drop: u64,
    io_ns: u64,
    reads: u64,
    hits: u64,
    misses: u64,
    evictions: u64,
    allocs: usize,
}

fn sample_phase(
    fs: &Engine<ReadDevice<'_>>,
    stats: &IoStats,
    series: usize,
    phase: &str,
) -> Result<()> {
    let mut samples = Vec::with_capacity(COUNT);
    heap_meter::PHASE_PEAK.store(heap_meter::LIVE.load(Ordering::Relaxed), Ordering::Relaxed);
    stats.active.set(true);
    let begin = Instant::now();
    for i in 0..COUNT {
        let k = if phase == "chaud_512k" { i % REGION } else { i };
        let allocs = heap_meter::ALLOCATIONS.load(Ordering::Relaxed);
        let before = fs.cache_stats();
        let (io_before, n_before) = (stats.ns.get(), stats.n.get());
        let cpu_before = cpu_ns();
        let start = Instant::now();
        let data = fs.read("/volume", (k * CHUNK) as u64, CHUNK)?;
        let read = start.elapsed().as_nanos() as u64;
        assert_eq!(data.len(), CHUNK);
        assert!(data.iter().all(|&b| b == (k ^ series) as u8));
        drop(data);
        let full = start.elapsed().as_nanos() as u64;
        let cpu = cpu_ns() - cpu_before;
        let after = fs.cache_stats();
        samples.push(Sample {
            full,
            cpu,
            read,
            check_drop: full - read,
            io_ns: stats.ns.get() - io_before,
            reads: stats.n.get() - n_before,
            hits: after.hits - before.hits,
            misses: after.misses - before.misses,
            evictions: after.evictions - before.evictions,
            allocs: heap_meter::ALLOCATIONS.load(Ordering::Relaxed) - allocs,
        });
    }
    let loop_ns = begin.elapsed().as_nanos();
    stats.active.set(false);
    let peak = heap_meter::PHASE_PEAK.load(Ordering::Relaxed);
    println!("phase {series} {phase} {loop_ns} {peak}");
    let stdout = io::stdout();
    let mut out = io::BufWriter::new(stdout.lock());
    for s in samples {
        if phase == "chaud_512k" {
            assert_eq!((s.reads, s.misses, s.evictions), (0, 0, 0));
        }
        writeln!(
            out,
            "{} {} {} {} {} {} {} {} {} {}",
            s.full,
            s.cpu,
            s.read,
            s.check_drop,
            s.io_ns,
            s.reads,
            s.hits,
            s.misses,
            s.evictions,
            s.allocs
        )?;
    }
    out.flush()?;
    Ok(())
}

fn run(directory: &Path, sector: u32, policy: &str) -> Result<()> {
    assert!(cpu_ns() > 0);
    for series in 1..=5 {
        let path = directory.join(format!("serie-{series}.img"));
        let mut src = File::open(&path)?;
        let (file, source_for_mount) = if policy == "nocache" {
            // Inode indépendant, créé et alimenté avec F_NOCACHE actif dès
            // la première écriture. Vérification sur le même descripteur avant
            // les mesures. L'image immuable d'origine sert au montage strict.
            let mut copy = tempfile::tempfile()?;
            bypass(&copy)?;
            let mut buf = [0; 128 * 1024];
            loop {
                let n = src.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                copy.write_all(&buf[..n])?;
            }
            copy.sync_all()?;
            src.rewind()?;
            copy.rewind()?;
            let mut replica = [0; 128 * 1024];
            let mut checked = 0u64;
            loop {
                let n = src.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                copy.read_exact(&mut replica[..n])?;
                if buf[..n] != replica[..n] {
                    return Err(Error::Corrupt("copie temporaire différente de la source"));
                }
                checked += n as u64;
            }
            if checked != PAGES * 4096 || copy.read(&mut replica[..1])? != 0 {
                return Err(Error::Invalid("longueur de copie incorrecte"));
            }
            (copy, Some(FileImage::from_file(src, PAGES, sector)?))
        } else {
            // Lecture explicite de toute l'image via le cache de fichiers macOS.
            // Aucune obligation de résidence après cette passe de préchauffage.
            let mut buf = [0; 128 * 1024];
            let mut total = 0;
            loop {
                let n = src.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                std::hint::black_box(&buf[..n]);
                total += n;
            }
            assert_eq!(total as u64, PAGES * 4096);
            (File::open(path)?, None)
        };
        let stats = IoStats::default();
        let image = ReadDevice {
            file: FileImage::from_file(file, PAGES, sector)?,
            stats: &stats,
            source_for_mount,
        };
        let fs = Engine::mount(image, IDS, Mode::ReadOnly)?;
        stats.mounted.set(true);
        fs.reclaim_cache();
        assert_eq!(fs.cache_stats().pages, 0);
        sample_phase(&fs, &stats, series, "froid_32m")?;
        fs.reclaim_cache();
        for i in 0..REGION {
            let bytes = fs.read("/volume", (i * CHUNK) as u64, CHUNK)?;
            assert_eq!(bytes.len(), CHUNK);
            assert!(bytes.iter().all(|&b| b == (i ^ series) as u8));
        }
        sample_phase(&fs, &stats, series, "chaud_512k")?;
        fs.unmount()?;
    }
    Ok(())
}

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let main_stack = heap_meter::current_stack_size();
    if let [cmd, path] = &args[..]
        && cmd == "prepare"
    {
        return prepare(Path::new(path));
    }
    let [path, sector, policy] = &args[..] else {
        return Err(Error::Invalid("dossier 512|4096 buffered|nocache"));
    };
    let sector = match sector.as_str() {
        "512" => 512,
        "4096" => 4096,
        _ => return Err(Error::Invalid("secteur")),
    };
    if !matches!(policy.as_str(), "buffered" | "nocache") {
        return Err(Error::Invalid("cache hôte"));
    }
    eprintln!("calibration_cpu={}", thread_clock::calibrate());
    std::thread::scope(|scope| -> Result<()> {
        std::thread::Builder::new()
            .stack_size(STACK)
            .spawn_scoped(scope, || run(Path::new(path), sector, policy))?
            .join()
            .expect("panique du banc")
    })?;
    let heap = heap_meter::PEAK.load(Ordering::Relaxed);
    println!("memoire {heap} {main_stack} {STACK}");
    Ok(())
}
