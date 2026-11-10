//! Banc macOS sur fichiers temporaires anonymes. Les chronos incluent les
//! contrôles des résultats, les copies de données et les deux sync_all de commit.
use novafs::device::{FileImage, Persistence, SimDevice, SimFault};
use novafs::{Block, Device, Engine, Error, Ids, Mode, Result};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::atomic::Ordering,
    time::Instant,
};

mod heap_meter;
mod reservation_bench;
use heap_meter::{LIVE, PEAK, PHASE_PEAK};

const IDS: Ids = Ids {
    fs: [11; 16],
    lv: [29; 16],
};
const SECOND_STACK: usize = 512 * 1024;
const WORKER_STACK: usize = 2 * 1024 * 1024;
const TRAFFIC: usize = 32 * 1024 * 1024;
const CHUNK: usize = 64 * 1024;
const SMALL: usize = 4096;
const PAGE: usize = 4096;
const IMAGE_BLOCKS: u64 = 24 * 1024; // 96 Mio de fichier creux, hors tas Rust.

#[derive(Default)]
struct Counts {
    reads: Cell<u64>,
    writes: Cell<u64>,
    flushes: Cell<u64>,
    flush_ns: Cell<u128>,
}
impl Counts {
    fn readout(&self) -> (u64, u64, u64, u128) {
        (
            self.reads.get(),
            self.writes.get(),
            self.flushes.get(),
            self.flush_ns.get(),
        )
    }
}

struct Reuse {
    phase: u8,
    first: Vec<bool>,
    second: Vec<bool>,
}
struct Image {
    file: FileImage,
    counts: Rc<Counts>,
    reuse: Rc<RefCell<Reuse>>,
}

impl Device for Image {
    fn blocks(&self) -> u64 {
        self.file.blocks()
    }
    fn read(&self, block: u64) -> Result<Block> {
        self.counts.reads.set(self.counts.reads.get() + 1);
        self.file.read(block)
    }
    fn write(&mut self, block: u64, data: &Block) -> Result<()> {
        self.file.write(block, data)?;
        self.counts.writes.set(self.counts.writes.get() + 1);
        let mut tracker = self.reuse.borrow_mut();
        if tracker.phase == 1 && data[..16] == [0xa1; 16] {
            tracker.first[block as usize] = true;
        } else if tracker.phase == 2 && data[..16] == [0xb2; 16] {
            tracker.second[block as usize] = true;
        }
        Ok(())
    }
    fn flush(&mut self) -> Result<()> {
        let start = Instant::now();
        let result = self.file.flush();
        self.counts
            .flush_ns
            .set(self.counts.flush_ns.get() + start.elapsed().as_nanos());
        self.counts.flushes.set(self.counts.flushes.get() + 1);
        result
    }
}

type Fixture = (Engine<Image>, Rc<Counts>, Rc<RefCell<Reuse>>);
fn fixture(blocks: u64, sector: u32, trace_reuse: bool) -> Result<Fixture> {
    let tracked = if trace_reuse { blocks as usize } else { 0 };
    let counts = Rc::new(Counts::default());
    let reuse = Rc::new(RefCell::new(Reuse {
        phase: 0,
        first: vec![false; tracked],
        second: vec![false; tracked],
    }));
    let image = Image {
        file: FileImage::create(blocks, sector)?,
        counts: counts.clone(),
        reuse: reuse.clone(),
    };
    Ok((Engine::format(image, IDS)?, counts, reuse))
}

fn measure(
    label: &str,
    series: usize,
    operations: usize,
    useful: usize,
    counts: &Counts,
    mut operation: impl FnMut(usize) -> Result<()>,
) -> Result<()> {
    assert!(operations > 0 && operations <= SMALL * 2);
    let mut times = Vec::with_capacity(operations);
    PHASE_PEAK.store(LIVE.load(Ordering::Relaxed), Ordering::Relaxed);
    let before = counts.readout();
    let begin = Instant::now();
    for i in 0..operations {
        let op = Instant::now();
        operation(i)?;
        times.push(op.elapsed().as_nanos() as u64);
    }
    let elapsed = begin.elapsed().as_nanos().max(1);
    let after = counts.readout();
    let peak = PHASE_PEAK.load(Ordering::Relaxed);
    times.sort_unstable();
    let quantile = |p: usize| times[(operations * p).div_ceil(100) - 1];
    println!(
        "{label}\t{series}\t{operations}\t{useful}\t{elapsed}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{peak}",
        quantile(50usize),
        quantile(95),
        quantile(99),
        times[operations - 1],
        after.0 - before.0,
        after.1 - before.1,
        after.2 - before.2,
        after.3 - before.3,
    );
    Ok(())
}

fn discard(e: &mut Engine<Image>, file: &str) -> Result<()> {
    e.unlink(file)?;
    // Une collection peut publier plusieurs lots durables.
    while e.collect()? {}
    Ok(())
}

fn pressure(sector: u32) -> Result<()> {
    let (mut e, c, reuse) = fixture(480, sector, true)?;
    e.create("/a")?;
    e.create("/b")?;
    reuse.borrow_mut().phase = 1;
    let mut pages = 0;
    loop {
        let saved = e.receipt();
        match e.write("/a", pages * PAGE as u64, &[0xa1; PAGE]) {
            Ok(()) => {
                pages += 1;
            }
            Err(Error::NoSpace) => {
                assert_eq!(saved, e.receipt());
                break;
            }
            Err(e) => return Err(e),
        }
    }
    assert!(pages > 0);
    let full = e.space_stats();
    let extents = e.extent_count("/a")?;
    assert_eq!(e.read("/a", 0, PAGE)?, [0xa1; PAGE]);
    discard(&mut e, "/a")?;
    reuse.borrow_mut().phase = 2;
    let retry = (pages / 2).max(1) as usize;
    measure("reemploi", 1, retry, retry * PAGE, &c, |i| {
        e.write("/b", i as u64 * PAGE as u64, &[0xb2; PAGE])
    })?;
    let reused = {
        let r = reuse.borrow();
        r.first
            .iter()
            .zip(&r.second)
            .filter(|(before, after)| **before && **after)
            .count()
    };
    assert!(reused > 0, "aucun ancien bloc de données réutilisé");
    let after = e.space_stats();
    println!(
        "pression\tpages_avant_refus={pages}\textents={extents}\tlibres_au_refus={}\tdifferes={}\tplus_grand={}\tblocs_reutilises={reused}\tintervalles_apres={}",
        full.free, full.deferred, full.largest_free, after.intervals
    );
    e.verify()?;
    e.unmount()?;
    Ok(())
}

fn maximum_geometry(sector: u32) -> Result<()> {
    let (mut e, counts, _) = fixture(novafs::MAX_BLOCKS, sector, false)?;
    e.create("/petit")?; // Les deux générations doivent exister avant l'inspection.
    measure("verification_8gio_creux", 1, 1, 0, &counts, |_| e.verify())?;
    e.unmount()?;
    Ok(())
}

/// Le processus parent fournit le descripteur de son image creuse précréée.
/// Le formatage ordinaire exige tous les blocs vierges avant toute écriture.
fn prepare_check_image(sector: u32) -> Result<()> {
    use std::{fs::File, io, os::fd::AsFd};
    let file = File::from(io::stdin().as_fd().try_clone_to_owned()?);
    let disk = FileImage::from_file(file, novafs::MAX_BLOCKS, sector)?;
    let mut fs = Engine::format(disk, IDS)?;
    fs.mkdir("/catalogue")?;
    for i in 0..96 {
        fs.create(&format!("/catalogue/document-{i:03}"))?;
    }
    fs.create("/contenu")?;
    fs.write("/contenu", 0, &[0xaf; PAGE])?;
    fs.write("/contenu", 0, &[0x5c; PAGE])?;
    fs.verify()?;
    fs.unmount()?;
    Ok(())
}

fn check_restitution(sector: u32) -> Result<()> {
    use novafs::{CheckIssue, PairStatus, check_image, check_image_with_bitmap_budget};
    let mut fs = Engine::format(SimDevice::new(512, sector)?, IDS)?;
    fs.create("/t")?;
    fs.write("/t", 0, &[0x32; PAGE])?;
    let slot = fs.generation() as usize % 2;
    let healthy = fs.unmount()?;
    let corrupt = healthy.control().snapshot().device();
    let mut anchor = corrupt.read(slot as u64)?;
    anchor[64] ^= 1;
    corrupt.control().replace_stable(slot as u64, anchor)?;

    let check = |case| {
        let report = match case {
            0 => check_image(&healthy, IDS),
            1 => check_image(&corrupt, IDS),
            _ => check_image_with_bitmap_budget(&healthy, IDS, 0),
        };
        match case {
            0 => assert!(matches!(report.pair, PairStatus::Verified { slot: n } if n == slot)),
            1 => {
                assert!(matches!(report.pair, PairStatus::Unavailable));
                assert!(report.slots[1 - slot].issue.is_none());
                assert!(matches!(
                    report.slots[slot].issue,
                    Some(CheckIssue {
                        error: Error::Corrupt(_),
                        ..
                    })
                ));
            }
            _ => {
                assert!(matches!(report.pair, PairStatus::Unavailable));
                assert!(report.slots.iter().all(|s| matches!(
                    s.issue,
                    Some(CheckIssue {
                        error: Error::NoMemory,
                        ..
                    })
                )));
            }
        }
    };
    for (case, name) in ["sain", "corrompu", "penurie_bitmaps"]
        .into_iter()
        .enumerate()
    {
        check(case);
        let baseline = LIVE.load(Ordering::Relaxed);
        PHASE_PEAK.store(baseline, Ordering::Relaxed);
        for _ in 0..64 {
            check(case);
            assert_eq!(
                LIVE.load(Ordering::Relaxed),
                baseline,
                "allocation retenue : {name}"
            );
        }
        let peak = PHASE_PEAK.load(Ordering::Relaxed);
        let after = LIVE.load(Ordering::Relaxed);
        println!(
            "controle={name} cycles=64 vivant_avant={baseline} vivant_apres={after} pic_phase={peak}"
        );
    }
    Ok(())
}

fn recovery_cycle(sector: u32, byte: u8) -> Result<()> {
    let data = [byte; PAGE];
    {
        let (mut writer, counts, _) = fixture(256, sector, false)?;
        let formatted = counts.flushes.get();
        writer.create("/t")?;
        writer.write("/t", 0, &data)?;
        let published = counts.flushes.get();
        assert_eq!(published - formatted, 4);
        let reader = Engine::mount(writer.unmount()?, IDS, Mode::ReadOnly)?;
        reader.verify()?;
        assert_eq!(reader.read("/t", 0, PAGE)?, data);
        assert_eq!(counts.flushes.get(), published);
        let writer = Engine::mount(reader.unmount()?, IDS, Mode::ReadWrite)?;
        assert_eq!(counts.flushes.get() - published, 1);
        drop(writer.unmount()?);
    }

    let sim = SimDevice::new(256, sector)?;
    let control = sim.control();
    let mut engine = Engine::format(sim, IDS)?;
    engine.create("/t")?;
    engine.write("/t", 0, &data)?;
    let receipt = engine.receipt();
    control.inject(SimFault::Flush {
        ordinal: 2,
        persist_newest: 0,
    })?;
    assert!(matches!(
        engine.write("/t", 0, &[!byte; PAGE]),
        Err(Error::Publication { phase: "F2", .. })
    ));
    let sim = engine.into_device();
    control.power_cut(Persistence::None)?;
    let recovered = Engine::mount(sim, IDS, Mode::ReadWrite)?;
    assert_eq!(recovered.receipt(), receipt);
    assert_eq!(recovered.read("/t", 0, PAGE)?, data);
    recovered.verify()?;
    drop(recovered.unmount()?);
    Ok(())
}

fn recovery_memory(sector: u32) -> Result<()> {
    // Amorcer le runtime de ce thread avant de mesurer les allocations retenues.
    recovery_cycle(sector, 0xa5)?;
    let baseline = LIVE.load(Ordering::Relaxed);
    PHASE_PEAK.store(baseline, Ordering::Relaxed);
    for cycle in 0..64 {
        recovery_cycle(sector, cycle)?;
        assert_eq!(
            LIVE.load(Ordering::Relaxed),
            baseline,
            "allocations résiduelles au cycle {cycle}"
        );
    }
    let after = LIVE.load(Ordering::Relaxed);
    let peak = PHASE_PEAK.load(Ordering::Relaxed);
    println!(
        "reprise secteur={sector} cycles=64 vivant_avant={baseline} vivant_apres={after} pic_phase={peak} flushs_par_commit=2 flushs_montage_ro=0 flushs_montage_rw=1"
    );
    Ok(())
}

/// Deux acteurs macOS, chemins distincts dans le même moteur à écrivain
/// unique. Un client garde l’exclusion le temps d’une opération du moteur.
fn contention(sector: u32) -> Result<()> {
    let file = FileImage::create(2048, sector)?;
    let mut e = Engine::format(file, IDS)?;
    e.create("/a")?;
    e.create("/b")?;
    let shared = std::sync::Mutex::new(e);
    for series in 0..=5 {
        let barrier = std::sync::Barrier::new(2);
        let ops = if series == 0 { 8 } else { 64 };
        let client = |path: &str, seed: u8| -> Result<(Vec<u64>, u128)> {
            let mut latency = Vec::with_capacity(ops);
            barrier.wait();
            let start = Instant::now();
            for i in 0..ops {
                let begin = Instant::now();
                let offset = ((i % 32) * PAGE) as u64;
                let page = [seed.wrapping_add(i as u8); PAGE];
                shared.lock().unwrap().write(path, offset, &page)?;
                assert_eq!(shared.lock().unwrap().read(path, offset, PAGE)?, page);
                latency.push(begin.elapsed().as_nanos() as u64);
            }
            Ok((latency, start.elapsed().as_nanos()))
        };
        std::thread::scope(|scope| -> Result<()> {
            let start = Instant::now();
            let b = std::thread::Builder::new()
                .stack_size(SECOND_STACK)
                .spawn_scoped(scope, || client("/b", 92))?;
            let first = client("/a", 11);
            let second = b.join().expect("panique du client B");
            let wall = start.elapsed().as_nanos();
            for (actor, (mut samples, ns)) in [("A", first?), ("B", second?)] {
                samples.sort_unstable();
                let q = |p: usize| samples[(samples.len() * p).div_ceil(100) - 1];
                println!(
                    "contention\tserie={series}\tacteur={actor}\toperations={ops}\tutile={}\tacteur_ns={ns}\tsimultane_ns={wall}\tp50_ns={}\tp95_ns={}\tp99_ns={}\tmax_ns={}",
                    ops * 2 * PAGE,
                    q(50),
                    q(95),
                    q(99),
                    q(100)
                );
            }
            Ok(())
        })?;
    }
    let e = shared.into_inner().unwrap();
    e.verify()?;
    drop(e.unmount()?);
    Ok(())
}

fn run(sector: u32) -> Result<()> {
    println!(
        "architecture={} secteur={sector} image_mio=96 arene_stack_max={} allocations=System",
        std::env::consts::ARCH,
        WORKER_STACK
    );
    println!(
        "phase\tserie\toperations\toctets_utiles\tduree_ns\tp50_ns\tp95_ns\tp99_ns\tmax_ns\tblocs_lus\tblocs_ecrits\tflushs\tflush_ns\tpic_malloc_octets"
    );
    let (mut e, counts, _) = fixture(IMAGE_BLOCKS, sector, false)?;
    e.create("/volume")?;
    let mut transfer = [0; CHUNK];
    for i in 0..32 {
        transfer.fill(i as u8);
        e.write("/volume", (i * CHUNK) as u64, &transfer)?;
        assert_eq!(e.read("/volume", (i * CHUNK) as u64, CHUNK)?, transfer);
    }
    for series in 1..=5 {
        measure(
            "ecriture_seq64k",
            series,
            TRAFFIC / CHUNK,
            TRAFFIC,
            &counts,
            |i| {
                transfer.fill((i ^ series) as u8);
                e.write("/volume", (i * CHUNK) as u64, &transfer)
            },
        )?;
        measure(
            "lecture_seq64k",
            series,
            TRAFFIC / CHUNK,
            TRAFFIC,
            &counts,
            |i| {
                let read = e.read("/volume", (i * CHUNK) as u64, CHUNK)?;
                assert_eq!(read.len(), CHUNK);
                assert!(read.iter().all(|&b| b == (i ^ series) as u8));
                Ok(())
            },
        )?;
        // Permutation déterministe ; un accès sur deux croise la frontière du bloc.
        let address = |i: usize| {
            (((i * 71 + series * 311) % (TRAFFIC / PAGE - 1)) * PAGE
                + if i % 2 == 1 { 23 } else { 0 }) as u64
        };
        measure(
            "lecture_aleatoire4k",
            series,
            256,
            256 * PAGE,
            &counts,
            |i| {
                let addr = address(i);
                let bytes = e.read("/volume", addr, PAGE)?;
                assert_eq!(bytes.len(), PAGE);
                for (j, b) in bytes.into_iter().enumerate() {
                    assert_eq!(b, (((addr as usize + j) / CHUNK) ^ series) as u8);
                }
                Ok(())
            },
        )?;
        measure(
            "ecriture_aleatoire4k",
            series,
            256,
            256 * PAGE,
            &counts,
            |i| e.write("/volume", address(i), &[(i ^ 0x55) as u8; PAGE]),
        )?;
        for i in 0..256 {
            assert_eq!(
                e.read("/volume", address(i), PAGE)?,
                [(i ^ 0x55) as u8; PAGE]
            );
        }
        println!(
            "fragmentation\tserie={series}\textents={}\tintervalles={}\tplus_grand={}",
            e.extent_count("/volume")?,
            e.space_stats().intervals,
            e.space_stats().largest_free
        );
    }
    e.mkdir("/petits")?;
    measure("creer_et_ecrire16", 1, SMALL, 16 * SMALL, &counts, |i| {
        let path = format!("/petits/f{i:04}");
        e.create(&path)?;
        e.write(&path, 0, &[i as u8; 16])
    })?;
    for series in 1..=5 {
        measure(
            "petite_lecture16",
            series,
            SMALL,
            16 * SMALL,
            &counts,
            |i| {
                let key = (i * 177 + series * 223) % SMALL;
                let bytes = e.read(&format!("/petits/f{key:04}"), 0, 16)?;
                assert_eq!(bytes, [key as u8; 16]);
                Ok(())
            },
        )?;
    }
    measure("verification_integrale", 1, 1, 0, &counts, |_| e.verify())?;
    for series in 1..=5 {
        let d = e.unmount()?;
        let start = Instant::now();
        e = Engine::mount(d, IDS, Mode::ReadOnly)?;
        println!(
            "remontage\tserie={series}\tduree_ns={}",
            start.elapsed().as_nanos()
        );
        for (bytes, labels) in [
            (TRAFFIC, ["lecture_apres_remontage", "lecture_repetee"]),
            (512 * 1024, ["lecture_512k_froide", "lecture_512k_chaude"]),
        ] {
            let previous = e.cache_stats();
            let released = e.reclaim_cache();
            assert_eq!(released, previous.pages);
            assert_eq!(e.cache_stats().pages, 0);
            let mut sums = [0u64; 2];
            for (j, name) in labels.into_iter().enumerate() {
                let before = e.cache_stats();
                measure(name, series, bytes / CHUNK, bytes, &counts, |i| {
                    let block = e.read("/volume", (i * CHUNK) as u64, CHUNK)?;
                    assert_eq!(block.len(), CHUNK);
                    sums[j] += block.iter().map(|&b| u64::from(b)).sum::<u64>();
                    Ok(())
                })?;
                let after = e.cache_stats();
                assert!(after.pages > 0);
                assert!(after.hits > before.hits);
                println!(
                    "cache\tserie={series}\tphase={name}\tpages={}\ttouches={}\tabsences={}\tevictions={}\tpages_rendues_avant_duo={released}",
                    after.pages,
                    after.hits - before.hits,
                    after.misses - before.misses,
                    after.evictions - before.evictions
                );
            }
            assert_eq!(sums[0], sums[1]);
        }
        assert_eq!(e.read("/petits/f4095", 0, 16)?, [255; 16]);
    }
    drop(e.unmount()?);
    contention(sector)?;
    pressure(sector)?;
    maximum_geometry(sector)?;
    Ok(())
}

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let sector = match args.next().as_deref() {
        Some("512") => 512,
        None | Some("4096") => 4096,
        _ => {
            return Err(Error::Invalid(
                "novafs-bench [512|4096] [complet|pression|memoire|reprise|prepare-controle|restitution-controle|reservations]",
            ));
        }
    };
    let scenario = args.next().unwrap_or_else(|| "complet".to_owned());
    if args.next().is_some() {
        return Err(Error::Invalid("trop d’arguments"));
    }
    let stacks = if scenario == "complet" {
        WORKER_STACK + SECOND_STACK
    } else {
        WORKER_STACK
    };
    std::thread::Builder::new()
        .stack_size(WORKER_STACK)
        .spawn(move || match scenario.as_str() {
            "complet" => run(sector),
            "pression" => pressure(sector),
            "memoire" => maximum_geometry(sector),
            "reprise" => recovery_memory(sector),
            "prepare-controle" => prepare_check_image(sector),
            "restitution-controle" => check_restitution(sector),
            "reservations" => reservation_bench::run(sector),
            _ => Err(Error::Invalid("scénario inconnu")),
        })?
        .join()
        .expect("panique du benchmark")?;
    let peak = PEAK.load(Ordering::Relaxed);
    #[cfg(target_os = "macos")]
    {
        let main_stack = heap_meter::current_stack_size();
        println!(
            "pile_principale={main_stack} tas_et_toutes_piles={}",
            peak + stacks + main_stack
        );
    }
    println!(
        "pic_malloc_octets={peak} piles_travail_reservees_max={stacks} majorant_tas_rust_et_piles={} plafond_7mio={}",
        peak + stacks,
        7 * 1024 * 1024
    );
    Ok(())
}
