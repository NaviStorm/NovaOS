//! Comptage des E/S de l'image, des allocations et des phases imbriquées.
//! Les données de la fixture sont des pages remplies d'une valeur entre 1 et 219.
//! Une signature de métadonnées ne peut donc pas être confondue avec ces données.
#[path = "heap_meter.rs"]
mod heap_meter;
use novafs::device::FileImage;
use novafs::latency_probes as probes;
#[cfg(feature = "p5-profil")]
use novafs::shared::SharedPool;
use novafs::{Block, Device, Ids, Result};
use serde_json::json;
use std::cell::Cell;
use std::rc::Rc;
use std::sync::atomic::Ordering;
use std::time::Instant;

const N: usize = 16;
const TYPES: [&str; 8] = [
    "donnees",
    "ancre_fs",
    "manifeste_fs",
    "arbre_fs",
    "racine_traduction",
    "feuille_traduction",
    "catalogue",
    "selecteur",
];
fn role(block: u64, data: &Block) -> usize {
    match &data[..8] {
        b"NOVAP0FS" => match u16::from_le_bytes(data[12..14].try_into().unwrap()) {
            n @ 1..=3 => usize::from(n),
            n => panic!("type FS inconnu {n}"),
        },
        b"NP5FSROT" => 4,
        b"NP5FSLEF" => 5,
        b"NP5POOLC" => 6,
        b"NP5POOLS" => 7,
        b"NP5POOLH" => panic!("en-tête sur le chemin mesuré, bloc {block}"),
        _ => {
            assert!(data[..8].iter().all(|&byte| byte == data[0]));
            0
        }
    }
}
#[derive(Default)]
struct Counter {
    on: Cell<bool>,
    reads: Cell<[u64; 8]>,
    writes: Cell<[u64; 8]>,
    syncs: Cell<u64>,
    wall_flush: Cell<[u64; 2]>,
    cpu_flush: Cell<[u64; 2]>,
}
impl Counter {
    fn reset(&self) {
        self.reads.set([0; 8]);
        self.writes.set([0; 8]);
        self.syncs.set(0);
        self.wall_flush.set([0; 2]);
        self.cpu_flush.set([0; 2]);
    }
}
struct Observed(FileImage, Rc<Counter>);
impl Device for Observed {
    fn blocks(&self) -> u64 {
        self.0.blocks()
    }
    fn read(&self, b: u64) -> Result<Block> {
        let data = self.0.read(b)?;
        if self.1.on.get() {
            let mut groups = self.1.reads.get();
            groups[role(b, &data)] += 1;
            self.1.reads.set(groups);
        }
        Ok(data)
    }
    fn write(&mut self, b: u64, data: &Block) -> Result<()> {
        self.0.write(b, data)?;
        if self.1.on.get() {
            let mut groups = self.1.writes.get();
            groups[role(b, data)] += 1;
            self.1.writes.set(groups);
        }
        Ok(())
    }
    fn flush(&mut self) -> Result<()> {
        if !self.1.on.get() {
            return self.0.flush();
        }
        let mark = Instant::now();
        let cpu = probes::thread_cpu_ns();
        self.0.flush()?;
        let cpu = probes::thread_cpu_ns() - cpu;
        let wall = mark.elapsed().as_nanos() as u64;
        let n = self.1.syncs.get();
        self.1.syncs.set(n + 1);
        let mut wall_totals = self.1.wall_flush.get();
        wall_totals[n as usize % 2] += wall;
        self.1.wall_flush.set(wall_totals);
        let mut cpu_totals = self.1.cpu_flush.get();
        cpu_totals[n as usize % 2] += cpu;
        self.1.cpu_flush.set(cpu_totals);
        Ok(())
    }
}

fn group(
    cnt: &Counter,
    phase: &str,
    config: &str,
    sector: u32,
    mut f: impl FnMut(usize) -> Result<()>,
) -> Result<()> {
    cnt.reset();
    probes::begin();
    cnt.on.set(true);
    let allocations = heap_meter::ALLOCATIONS.load(Ordering::Relaxed);
    let begin = Instant::now();
    let cpu = probes::thread_cpu_ns();
    for i in 0..N {
        f(i)?;
    }
    let cpu = probes::thread_cpu_ns() - cpu;
    let wall = begin.elapsed().as_nanos() as u64;
    let allocations = heap_meter::ALLOCATIONS.load(Ordering::Relaxed) - allocations;
    cnt.on.set(false);
    let mut counters = probes::finish();
    counters
        .as_object_mut()
        .unwrap()
        .retain(|_, v| v["mur"]["n"].as_u64().unwrap() > 0);
    let map = |values: [u64; 8]| {
        TYPES
            .iter()
            .zip(values)
            .map(|(&k, n)| (k.to_string(), json!(n)))
            .collect::<serde_json::Map<_, _>>()
    };
    println!(
        "{}",
        json!({"operation":phase,"volume":config,"secteur":sector,"architecture":std::env::consts::ARCH,
        "operations":N,"cpu_ns":cpu,"mur_ns":wall,"allocations":allocations,
        "lectures":map(cnt.reads.get()),"ecritures":map(cnt.writes.get()),"flushs":cnt.syncs.get(),
        "mur_flush_ns":cnt.wall_flush.get(),"cpu_flush_ns":cnt.cpu_flush.get(),"sondes":counters})
    );
    Ok(())
}
#[cfg(feature = "p5-profil")]
fn fixture(config: &str, sector: u32) -> Result<()> {
    let cnt = Rc::new(Counter::default());
    let mut pool = SharedPool::format(
        Observed(FileImage::create(8192, sector)?, Rc::clone(&cnt)),
        [9; 16],
    )?;
    let origin = pool.create(
        [21; 16],
        Ids {
            fs: [8; 16],
            lv: [5; 16],
        },
        512,
    )?;
    pool.create_file(origin, "/f")?;
    for i in 0..N {
        pool.write_file(origin, "/f", i as u64 * 16384, &[1 + i as u8; 16384])?;
    }
    let mut id = origin;
    let mut guards = Vec::new();
    match config {
        "exclusif" => {}
        "un_snapshot" | "plusieurs_snapshots" | "clone" => {
            let s = pool.snapshot(origin)?;
            guards.push((s, 1));
            if config == "clone" {
                id = pool.clone_snapshot(s, [29; 16])?;
            }
            if config == "plusieurs_snapshots" {
                for n in 0..2 {
                    pool.write_file(origin, "/f", 0, &[60 + n; 16384])?;
                    guards.push((pool.snapshot(origin)?, 60 + n));
                }
            }
        }
        _ => unreachable!(),
    }
    group(&cnt, "ecriture16k", config, sector, |i| {
        pool.write_file(id, "/f", i as u64 * 16384, &[101 + i as u8; 16384])
    })?;
    group(&cnt, "lecture16k_apres_ecritures", config, sector, |i| {
        assert_eq!(
            pool.read_file(id, "/f", i as u64 * 16384, 16384)?,
            [101 + i as u8; 16384]
        );
        Ok(())
    })?;
    group(&cnt, "lecture16k_rechauffee", config, sector, |i| {
        assert_eq!(
            pool.read_file(id, "/f", i as u64 * 16384, 16384)?,
            [101 + i as u8; 16384]
        );
        Ok(())
    })?;
    for (s, byte) in guards {
        assert_eq!(pool.read_file(s, "/f", 0, 16384)?, [byte; 16384]);
    }
    Ok(())
}
/// Rejeu exact du cycle de 128 écritures/lectures à 16 Kio du rapport P5,
/// sans temporisation des phases internes. La ligne compte les écritures seules.
fn historique(sector: u32) -> Result<()> {
    #[cfg(not(feature = "p5-profil"))]
    use novafs::{
        Engine,
        pool::{Pool, PoolCoordinator, Type},
    };
    let cnt = Rc::new(Counter::default());
    let ids = Ids {
        fs: [8; 16],
        lv: [9; 16],
    };
    let disk = Observed(FileImage::create(8192, sector)?, Rc::clone(&cnt));
    #[cfg(feature = "p5-profil")]
    let (coordinator, view) = {
        use novafs::shared::SharedCoordinator;
        let mut pool = SharedPool::format(disk, [17; 16])?;
        let id = pool.create([19; 16], ids, 256)?;
        pool.create_file(id, "/f")?;
        for j in 0..4 {
            pool.write_file(id, "/f", j * 16384, &[0x36; 16384])?;
        }
        (SharedCoordinator::mount(pool)?, id)
    };
    #[cfg(not(feature = "p5-profil"))]
    let (coordinator, view) = {
        let mut pool = Pool::format(disk)?;
        pool.create(ids.lv, Some(ids.fs), 1)?;
        {
            let mut fs = Engine::format(pool.view(ids.lv, Type::Preparing, ids.fs)?, ids)?;
            fs.create("/f")?;
            for j in 0..4 {
                fs.write("/f", j * 16384, &[0x36; 16384])?;
            }
        }
        pool.mark_ready(ids.lv, ids.fs)?;
        (PoolCoordinator::mount(pool)?, ids.lv)
    };
    let mut cpu = 0;
    let mut wall = 0u128;
    let mut allocs = 0;
    let io = 16384;
    for j in 0..128 {
        let offset = (j % 4) * io as u64;
        let data = [j as u8; 16384];
        cnt.on.set(true);
        let old_alloc = heap_meter::ALLOCATIONS.load(Ordering::Relaxed);
        let began = Instant::now();
        let start = probes::thread_cpu_ns();
        let mut ticket = coordinator.request_write(view)?;
        ticket.prepare("/f", offset, &data)?;
        ticket.commit()?;
        cpu += probes::thread_cpu_ns() - start;
        wall += began.elapsed().as_nanos();
        allocs += heap_meter::ALLOCATIONS.load(Ordering::Relaxed) - old_alloc;
        cnt.on.set(false);
        assert_eq!(
            coordinator.request_read(view)?.read("/f", offset, io)?,
            data
        );
    }
    let map = |vals: [u64; 8]| {
        TYPES
            .iter()
            .zip(vals)
            .map(|(&k, n)| (k.to_string(), json!(n)))
            .collect::<serde_json::Map<_, _>>()
    };
    println!(
        "{}",
        json!({"operation":"ecriture_historique_16k","volume":"exclusif","secteur":sector,
        "architecture":std::env::consts::ARCH,"operations":128,"cpu_ns":cpu,"mur_ns":wall,
        "allocations":allocs,"lectures":map(cnt.reads.get()),"ecritures":map(cnt.writes.get()),"flushs":cnt.syncs.get(),
        "mur_flush_ns":cnt.wall_flush.get(),"cpu_flush_ns":cnt.cpu_flush.get()})
    );
    Ok(())
}
fn main() -> Result<()> {
    println!(
        "{}",
        json!({"etalonnage":probes::calibrate(),"tls_octets":probes::TLS_BYTES,"pile_hote":heap_meter::current_stack_size()})
    );
    group(&Counter::default(), "temoin_sonde", "horloge", 4096, |_| {
        let _sample = probes::enter(probes::Phase::Empty);
        Ok(())
    })?;
    for sector in [4096, 512] {
        historique(sector)?;
        #[cfg(feature = "p5-profil")]
        for config in ["exclusif", "un_snapshot", "plusieurs_snapshots", "clone"] {
            fixture(config, sector)?;
        }
    }
    Ok(())
}
