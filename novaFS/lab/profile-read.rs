//! Profil inclusif de lecture sur image temporaire, séparé du banc de référence.
use novafs::{
    Engine, Error, Ids, Result,
    device::FileImage,
    latency_probes::{self as probes, Phase},
};
use std::sync::atomic::Ordering;

mod heap_meter;

const CHUNK: usize = 64 * 1024;
const CHUNKS: usize = 64;
const WINDOW: usize = 8;

fn read(engine: &Engine<FileImage>, i: usize) -> Result<()> {
    let bytes = {
        let _sample = probes::enter(Phase::ReadRequest);
        engine.read("/volume", (i * CHUNK) as u64, CHUNK)?
    };
    assert_eq!(bytes.len(), CHUNK);
    assert!(bytes.iter().all(|&b| b == (i + 1) as u8));
    Ok(())
}

fn profile(engine: &Engine<FileImage>, name: &str, chunks: usize, passes: usize) -> Result<()> {
    let before = engine.cache_stats();
    heap_meter::PHASE_PEAK.store(heap_meter::LIVE.load(Ordering::Relaxed), Ordering::Relaxed);
    let allocations = heap_meter::ALLOCATIONS.load(Ordering::Relaxed);
    probes::begin();
    for _ in 0..passes {
        for i in 0..chunks {
            read(engine, i)?;
        }
    }
    let allocs = heap_meter::ALLOCATIONS.load(Ordering::Relaxed) - allocations;
    let peak = heap_meter::PHASE_PEAK.load(Ordering::Relaxed);
    let histograms = probes::finish();
    let after = engine.cache_stats();
    let hits = after.hits - before.hits;
    let misses = after.misses - before.misses;
    let evictions = after.evictions - before.evictions;
    let count = |k: &str| histograms[k]["mur"]["n"].as_u64().unwrap();
    assert_eq!(count("lecture_moteur_64k"), (chunks * passes) as u64);
    assert_eq!(count("recherche_cache"), hits + misses);
    assert_eq!(
        count("defaut_metadonnees") + count("defaut_donnees"),
        misses
    );
    assert_eq!(count("lecture_image"), misses);
    assert_eq!(count("eviction_clock"), evictions);
    assert_eq!(count("allocation_system"), allocs as u64);
    if name == "ensemble_chaud_512k" {
        assert_eq!((misses, evictions), (0, 0));
    } else {
        assert!(misses > 0 && evictions > 0);
    }
    println!(
        "{}",
        serde_json::json!({
            "phase": name, "architecture": std::env::consts::ARCH, "lectures_64k": chunks * passes,
            "succes": hits, "defauts": misses, "evictions": evictions, "pas_clock": after.clock_steps - before.clock_steps,
            "pic_tas_processus_octets": peak, "pile_thread_octets": heap_meter::current_stack_size(),
        "etat_sondes_tls_octets": probes::TLS_BYTES,
            "sondes": histograms,
        })
    );
    Ok(())
}

fn run(sector: u32) -> Result<()> {
    // Mesure l'intervalle vide avec les mêmes lectures d'horloges et la même portée.
    probes::begin();
    for _ in 0..8_192 {
        drop(probes::enter(Phase::Empty));
    }
    let calibration = probes::finish();
    println!(
        "{}",
        serde_json::json!({
            "phase": "calibration", "architecture": std::env::consts::ARCH, "secteur": sector,
            "sonde_vide": calibration["sonde_vide"],
        })
    );
    drop(calibration);

    let mut e = Engine::format(
        FileImage::create(4096, sector)?,
        Ids {
            fs: [11; 16],
            lv: [29; 16],
        },
    )?;
    e.create("/volume")?;
    for i in 0..CHUNKS {
        e.write("/volume", (i * CHUNK) as u64, &[(i + 1) as u8; CHUNK])?;
    }
    e.reclaim_cache();
    profile(&e, "premier_parcours_4m", CHUNKS, 1)?;
    profile(&e, "parcours_repete_4m", CHUNKS, 8)?;
    e.reclaim_cache();
    for i in 0..WINDOW {
        read(&e, i)?;
    }
    profile(&e, "ensemble_chaud_512k", WINDOW, 64)?;
    e.verify()?;
    e.unmount()?;
    Ok(())
}

fn main() -> Result<()> {
    println!(
        "{}",
        serde_json::json!({"phase": "horloge_cpu", "etalonnage": probes::calibrate()})
    );
    let sector = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "4096".into())
        .parse()
        .map_err(|_| Error::Invalid("secteur attendu : 512 ou 4096"))?;
    std::thread::Builder::new()
        .stack_size(2 * 1024 * 1024)
        .spawn(move || run(sector))?
        .join()
        .expect("panique de la sonde de lecture")
}
