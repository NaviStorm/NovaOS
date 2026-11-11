//! Comparaison appariée sur copies privées d'une même image synthétique.
//! Les deux clés publiques ci-dessous sont exclusivement des vecteurs du banc.
#[path = "../lab/p6_donnees.rs"]
mod corpus;
#[path = "../lab/heap_meter.rs"]
#[allow(dead_code)]
mod heap_meter;
#[path = "../lab/thread_clock.rs"]
#[allow(dead_code)]
mod thread_clock;

use novafs::crypto::{Credential, Secret};
use novafs::device::FileImage;
use novafs::secure_pool::SecurePool;
use novafs::{Block, Compression, Device, Ids, Result};
use serde_json::json;
use std::cell::Cell;
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
use std::rc::Rc;
use std::sync::{Arc, Mutex, atomic::Ordering::Relaxed};
use std::time::Instant;

const BLOCKS: u64 = 4096;
const SIZE: usize = 16384;
const POOL: [u8; 16] = [38; 16];
const PHRASE: &[u8] = b"synthetic public P72 benchmark";

#[derive(Default, Clone, Copy)]
struct Counts {
    reads: [u64; 4],
    read_ns: [u64; 4],
    writes: u64,
    write_ns: u64,
    flush: [u64; 2],
    syncs: usize,
}
struct Disk(FileImage, Rc<Cell<Counts>>);

impl Device for Disk {
    fn mutation_lock(&self) -> Option<Arc<Mutex<()>>> {
        self.0.mutation_lock()
    }
    fn blocks(&self) -> u64 {
        self.0.blocks()
    }
    fn read(&self, block: u64) -> Result<Block> {
        let class = match block {
            0..=1 => 0,
            2..=9 => 1,
            10..=45 => 2,
            _ => 3,
        };
        let start = Instant::now();
        let result = self.0.read(block);
        let mut c = self.1.get();
        c.read_ns[class] += start.elapsed().as_nanos() as u64;
        c.reads[class] += 1;
        self.1.set(c);
        result
    }
    fn write(&mut self, block: u64, page: &Block) -> Result<()> {
        let start = Instant::now();
        let result = self.0.write(block, page);
        let mut c = self.1.get();
        c.write_ns += start.elapsed().as_nanos() as u64;
        c.writes += 1;
        self.1.set(c);
        result
    }
    fn flush(&mut self) -> Result<()> {
        let start = Instant::now();
        let result = self.0.flush();
        let mut c = self.1.get();
        if c.syncs < 2 {
            c.flush[c.syncs] = start.elapsed().as_nanos() as u64;
        }
        c.syncs += 1;
        self.1.set(c);
        result
    }
}

fn run(op: &str, sector: u32, profile: &str, seed: u64, path: &Path, calls: usize) -> Result<()> {
    let admin = Secret::from_bytes([11; 32]);
    let recovery = Secret::from_bytes([23; 32]);
    let crypt = profile != "clair";
    let codec = if matches!(profile, "lz4" | "snapshot" | "clone") {
        Compression::Lz4
    } else {
        Compression::None
    };
    let mut expected = corpus::donnees(SIZE, "repetitif", seed);
    let mut disk = FileImage::create(BLOCKS, sector)?;
    if op == "creer" {
        let mut p = SecurePool::format(disk, POOL, &admin)?;
        let id = p.create(
            [7; 16],
            Ids {
                fs: [9; 16],
                lv: [12; 16],
            },
            256,
            codec,
            crypt.then_some((PHRASE, &recovery)),
        )?;
        p.create_file(id, "/f")?;
        p.write_file(id, "/f", 0, &expected)?;
        if matches!(profile, "snapshot" | "clone") {
            let snap = p.snapshot(id)?;
            if profile == "clone" {
                p.clone_snapshot(snap, [31; 16])?;
            }
        }
        let mut out = File::create_new(path)?;
        std::io::copy(&mut p.into_device().into_file(), &mut out)?;
        out.flush()?;
        out.sync_all()?;
        return Ok(());
    }
    let mut source = File::open(path)?;
    assert_eq!(source.metadata()?.len(), BLOCKS * 4096);
    let mut block = [0; 4096];
    for i in 0..BLOCKS {
        source.read_exact(&mut block)?;
        disk.write(i, &block)?;
    }
    disk.flush()?;
    let counts: Rc<Cell<Counts>> = Rc::default();
    let mut p = SecurePool::open(Disk(disk, counts.clone()), POOL, &admin)?;
    let list = p.list()?;
    let id = if profile == "clone" {
        list.last().unwrap().id
    } else {
        list[0].id
    };
    let stack = heap_meter::current_stack_size();
    if crypt {
        let domain = p.domain(id)?;
        heap_meter::PHASE_PEAK.store(heap_meter::LIVE.load(Relaxed), Relaxed);
        p.unlock(domain, Credential::Passphrase(PHRASE))?;
        println!(
            "{}",
            json!({"kdf_octets":stack+heap_meter::PHASE_PEAK.load(Relaxed)})
        );
    }
    p.advance()?;
    assert_eq!(p.read_file(id, "/f", 0, SIZE)?, expected);
    for i in 0..calls {
        let pos = (i % 2) * SIZE / 2;
        let delta = corpus::donnees(SIZE / 2, "repetitif", seed + 327 + i as u64);
        counts.set(Counts::default());
        #[cfg(feature = "latency-probes")]
        novafs::latency_probes::begin();
        let live = heap_meter::LIVE.load(Relaxed);
        heap_meter::PHASE_PEAK.store(live, Relaxed);
        let alloc = heap_meter::ALLOCATIONS.load(Relaxed);
        let cpu = thread_clock::thread_cpu_ns();
        let start = Instant::now();
        p.write_file(id, "/f", pos as u64, &delta)?;
        let ns = start.elapsed().as_nanos() as u64;
        let cpu = thread_clock::thread_cpu_ns() - cpu;
        let heap = heap_meter::PHASE_PEAK.load(Relaxed);
        let alloc = heap_meter::ALLOCATIONS.load(Relaxed) - alloc;
        let c = counts.get();
        assert_eq!(c.syncs, 2);
        #[cfg(feature = "latency-probes")]
        let phases = novafs::latency_probes::finish();
        #[cfg(not(feature = "latency-probes"))]
        let phases = serde_json::Value::Null;
        println!(
            "{}",
            json!({"i":i,"octets":delta.len(),"mur_ns":ns,"cpu_ns":cpu,"lectures":c.reads,
            "lectures_ns":c.read_ns,"ecritures_blocs":c.writes,"ecritures_ns":c.write_ns,"f_ns":c.flush,
            "flushs":c.syncs,"flush_ns":c.flush.iter().sum::<u64>(),"tas_octets":heap,
            "pile_reservee_octets":stack,"allocations":alloc,"phases":phases})
        );
        expected[pos..pos + delta.len()].copy_from_slice(&delta);
        assert_eq!(p.read_file(id, "/f", 0, SIZE)?, expected);
    }
    p.check()
}

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    std::thread::Builder::new()
        .stack_size(2 * 1024 * 1024)
        .spawn(move || {
            println!("{{\"repere\":\"debut\"}}");
            let before = heap_meter::LIVE.load(Relaxed);
            run(
                &args[0],
                args[1].parse().unwrap(),
                &args[2],
                args[3].parse().unwrap(),
                Path::new(&args[4]),
                args[5].parse().unwrap(),
            )?;
            let after = heap_meter::LIVE.load(Relaxed);
            assert_eq!(before, after);
            println!(
                "{}",
                json!({"repere":"fin","tas_avant":before,"tas_apres":after})
            );
            Ok(())
        })?
        .join()
        .unwrap()
}
