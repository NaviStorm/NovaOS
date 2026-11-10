//! Lecture comparative avec l'allocateur System ordinaire du processus.
use novafs::{Engine, Error, Ids, Mode, Result, device::FileImage};
use std::{fs::File, io::Read, path::Path, time::Instant};

#[global_allocator]
static ALLOCATOR: std::alloc::System = std::alloc::System;

const CHUNK: usize = 64 * 1024;
const COUNT: usize = 512;
const REGION: usize = 8;
const PAGES: u64 = 24 * 1024;
const IDS: Ids = Ids {
    fs: [11; 16],
    lv: [29; 16],
};

fn stack_bytes() -> usize {
    use std::ffi::c_void;
    unsafe extern "C" {
        fn pthread_self() -> *mut c_void;
        fn pthread_get_stacksize_np(thread: *mut c_void) -> usize;
    }
    // Sûreté : pthread_self désigne le thread appelant pendant la requête.
    unsafe { pthread_get_stacksize_np(pthread_self()) }
}

fn read(fs: &Engine<FileImage>, chunk: usize, series: usize) -> Result<()> {
    let data = fs.read("/volume", (chunk * CHUNK) as u64, CHUNK)?;
    assert_eq!(data.len(), CHUNK);
    assert!(data.iter().all(|&b| b == (chunk ^ series) as u8));
    Ok(()) // La libération du Vec appartient à la requête chronométrée.
}

fn phase(fs: &Engine<FileImage>, series: usize, hot: bool) -> Result<()> {
    let before = fs.cache_stats();
    let limit = if hot { REGION } else { COUNT };
    let mut elapsed = [0u64; COUNT];
    let begin = Instant::now();
    for (i, ns) in elapsed.iter_mut().enumerate() {
        let k = i % limit;
        let start = Instant::now();
        read(fs, k, series)?;
        *ns = start.elapsed().as_nanos() as u64;
    }
    let loop_ns = begin.elapsed().as_nanos();
    let after = fs.cache_stats();
    if hot {
        assert_eq!(after.misses, before.misses);
        assert_eq!(after.evictions, before.evictions);
    }
    let name = if hot { "chaud_512k" } else { "froid_32m" };
    print!("phase {series} {name} {loop_ns}");
    for ns in elapsed {
        print!(" {ns}");
    }
    println!();
    Ok(())
}

fn run(images: &Path, sector: u32) -> Result<()> {
    println!("pile_travail {}", stack_bytes());
    for series in 1..=5 {
        let path = images.join(format!("serie-{series}.img"));
        let mut source = File::open(&path)?;
        let mut total = 0;
        let mut buf = [0; 128 * 1024];
        loop {
            let n = source.read(&mut buf)?;
            if n == 0 {
                break;
            }
            std::hint::black_box(&buf[..n]);
            total += n as u64;
        }
        assert_eq!(total, PAGES * 4096);
        let fs = Engine::mount(
            FileImage::from_file(File::open(path)?, PAGES, sector)?,
            IDS,
            Mode::ReadOnly,
        )?;
        fs.reclaim_cache();
        assert_eq!(fs.cache_stats().pages, 0);
        phase(&fs, series, false)?;
        fs.reclaim_cache();
        for k in 0..REGION {
            read(&fs, k, series)?;
        }
        phase(&fs, series, true)?;
        fs.unmount()?;
    }
    Ok(())
}

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let [images, sector] = &args[..] else {
        return Err(Error::Invalid("dossier 512|4096"));
    };
    let sector = match sector.as_str() {
        "4096" => 4096,
        "512" => 512,
        _ => return Err(Error::Invalid("secteur")),
    };
    println!("pile_principale {}", stack_bytes());
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .stack_size(2 * 1024 * 1024)
            .spawn_scoped(scope, || run(Path::new(images), sector))?
            .join()
            .expect("panique du lecteur témoin")
    })
}
