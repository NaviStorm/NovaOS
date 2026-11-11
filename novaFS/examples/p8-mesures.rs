//! Mesures sur une copie privée du corpus public de p7-lab. Les clés fixes
//! sont réservées aux images synthétiques créées par ce programme de laboratoire.
#[path = "../lab/heap_meter.rs"]
mod heap_meter;
#[allow(dead_code)]
#[path = "../lab/thread_clock.rs"]
mod thread_clock;
use novafs::crypto::{Credential, Secret};
use novafs::device::FileImage;
use novafs::secure_pool::scrub::{Authority, Mode, Status, scrub};
use novafs::secure_pool::{SecurePool, verify_commit_replicas};
use novafs::{Block, Device, Error, Result};
use serde_json::json;
use std::cell::Cell;
use std::fs::File;
use std::sync::atomic::Ordering::Relaxed;
use std::sync::{Arc, Mutex};
use std::time::Instant;

struct Disk {
    image: FileImage,
    reads: Cell<u64>,
    writes: u64,
    flushes: u64,
}
impl Device for Disk {
    fn mutation_lock(&self) -> Option<Arc<Mutex<()>>> {
        self.image.mutation_lock()
    }
    fn blocks(&self) -> u64 {
        self.image.blocks()
    }
    fn read(&self, b: u64) -> Result<Block> {
        self.reads.set(self.reads.get() + 1);
        self.image.read(b)
    }
    fn write(&mut self, b: u64, page: &Block) -> Result<()> {
        self.writes += 1;
        self.image.write(b, page)
    }
    fn flush(&mut self) -> Result<()> {
        self.flushes += 1;
        self.image.flush()
    }
}
fn samples(args: &[String]) -> Result<()> {
    let [path, sector, count, op] = args else {
        return Err(Error::Invalid("image secteur répétitions opération"));
    };
    let sector = sector.parse().map_err(|_| Error::Invalid("secteur"))?;
    let count: usize = count.parse().map_err(|_| Error::Invalid("répétitions"))?;
    let source = FileImage::from_file(File::open(path)?, 4096, sector)?;
    let mut image = FileImage::create(source.blocks(), sector)?;
    for lba in 0..source.blocks() {
        image.write(lba, &source.read(lba)?)?;
    }
    image.flush()?;
    let mut d = Disk {
        image,
        reads: Cell::new(0),
        writes: 0,
        flushes: 0,
    };
    let pool = [38; 16];
    let admin = Secret::from_bytes([11; 32]);
    let recovery = Secret::from_bytes([23; 32]);
    let (generation, domain) = {
        let mut p = SecurePool::open(&mut d, pool, &admin)?;
        let id = p.list()?[0].id;
        (p.generation(), p.domain(id)?)
    };
    let domains = [(domain, Credential::Recovery(&recovery))];
    let auth = Authority {
        pool,
        admin: &admin,
        domains: &domains,
    };
    let cert = 2 + 2 * (generation % 4);
    let good = d.read(cert)?;
    let mut bad = good;
    bad[4095] ^= 1;
    for _ in 0..count {
        if op == "reparation" {
            d.write(cert, &bad)?;
            d.flush()?;
        }
        d.reads.set(0);
        d.writes = 0;
        d.flushes = 0;
        let before = heap_meter::LIVE.load(Relaxed);
        heap_meter::PHASE_PEAK.store(before, Relaxed);
        let alloc = heap_meter::ALLOCATIONS.load(Relaxed);
        let cpu = thread_clock::thread_cpu_ns();
        let started = Instant::now();
        let mut distinct = None;
        match op.as_str() {
            "controle_p72" => {
                verify_commit_replicas(&d, pool, &admin)?;
                let mut p = SecurePool::open(&mut d, pool, &admin)?;
                p.unlock(domain, Credential::Recovery(&recovery))?;
                p.check()?;
            }
            "diagnostic" | "complet" | "reparation" => {
                let (mode, authority, expected) = match op.as_str() {
                    "diagnostic" => (Mode::Diagnostic, None, Status::Incomplete),
                    "complet" => (Mode::Full, Some(&auth), Status::Healthy),
                    _ => (Mode::Repair, Some(&auth), Status::Repaired),
                };
                let r = scrub(&mut d, mode, authority, |_| true)?;
                assert_eq!(r.status, expected);
                assert_eq!(
                    (r.reads, r.writes, r.flushes),
                    (d.reads.get(), d.writes, d.flushes)
                );
                distinct = Some(r.distinct_blocks);
            }
            _ => return Err(Error::Invalid("opération")),
        }
        let wall = started.elapsed().as_nanos() as u64;
        let cpu = thread_clock::thread_cpu_ns() - cpu;
        let peak = heap_meter::PHASE_PEAK.load(Relaxed);
        let allocations = heap_meter::ALLOCATIONS.load(Relaxed) - alloc;
        let (reads, writes, flushes) = (d.reads.get(), d.writes, d.flushes);
        assert_eq!(before, heap_meter::LIVE.load(Relaxed));
        if op == "reparation" {
            assert_eq!(d.read(cert)?, good);
            assert_eq!((writes, flushes), (1, 2));
        }
        println!(
            "{}",
            json!({"mur_ns":wall,"cpu_ns":cpu,"lectures":reads,"distincts":distinct,"ecritures":writes,"flushs":flushes,"allocations":allocations,"tas_pic":peak,"pile":heap_meter::current_stack_size()})
        );
    }
    Ok(())
}
fn main() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    std::thread::Builder::new()
        .stack_size(2 * 1024 * 1024)
        .spawn(move || {
            println!("{{\"repere\":\"debut\"}}");
            let before = heap_meter::LIVE.load(Relaxed);
            samples(&args)?;
            let after = heap_meter::LIVE.load(Relaxed);
            assert_eq!(before, after);
            println!(
                "{}",
                json!({"repere":"fin","tas_avant":before,"tas_apres":after})
            );
            Ok(())
        })?
        .join()
        .expect("thread du banc")
}
