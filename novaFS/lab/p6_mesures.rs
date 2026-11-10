//! Même client compilé contre 3ca74ee et le pool P6, images Darwin temporaires.
#[path = "p6_donnees.rs"]
mod corpus;
#[path = "heap_meter.rs"]
#[allow(dead_code)]
mod heap_meter;
#[path = "thread_clock.rs"]
mod thread_clock;
use corpus::{donnees, suivant};
use novafs::device::FileImage;
use novafs::shared::SharedPool;
use novafs::{Block, Device, Ids, Result};
use serde_json::json;
use std::cell::Cell;
use std::rc::Rc;
use std::sync::atomic::Ordering;
use std::time::Instant;

const IDS: Ids = Ids {
    fs: [2; 16],
    lv: [3; 16],
};
const B: usize = 4096;

#[derive(Clone, Copy, Default)]
struct Compteurs {
    lus: u64,
    ecrits: u64,
    lectures_ns: u64,
    ecritures_ns: u64,
    sync: u64,
    sync_ns: u64,
    derniere: u64,
}
struct Disque(FileImage, Rc<Cell<Compteurs>>);
impl Device for Disque {
    fn blocks(&self) -> u64 {
        self.0.blocks()
    }
    fn read(&self, b: u64) -> Result<Block> {
        let now = Instant::now();
        let page = self.0.read(b)?;
        let mut i = self.1.get();
        i.lectures_ns += now.elapsed().as_nanos() as u64;
        i.lus += 1;
        self.1.set(i);
        Ok(page)
    }
    fn write(&mut self, b: u64, bytes: &Block) -> Result<()> {
        let now = Instant::now();
        self.0.write(b, bytes)?;
        let mut i = self.1.get();
        i.ecritures_ns += now.elapsed().as_nanos() as u64;
        i.ecrits += 1;
        self.1.set(i);
        Ok(())
    }
    fn flush(&mut self) -> Result<()> {
        let now = Instant::now();
        self.0.flush()?;
        let mut i = self.1.get();
        i.derniere = now.elapsed().as_nanos() as u64;
        i.sync_ns += i.derniere;
        i.sync += 1;
        self.1.set(i);
        Ok(())
    }
}

fn codec() -> (u128, u128) {
    #[cfg(sondes)]
    {
        novafs::latency_probes::codec_cpu_ns()
    }
    #[cfg(not(sondes))]
    {
        (0, 0)
    }
}

struct Panneau<'a> {
    io: Rc<Cell<Compteurs>>,
    profil: &'a str,
    taille: usize,
}

impl Panneau<'_> {
    fn mesure<T>(
        &self,
        op: &str,
        n: usize,
        pos: usize,
        len: usize,
        f: impl FnOnce() -> Result<T>,
    ) -> Result<T> {
        let avant = self.io.get();
        let c = codec();
        let cpu = thread_clock::thread_cpu_ns();
        let debut = Instant::now();
        let v = f()?;
        let mur = debut.elapsed().as_nanos() as u64;
        let cpu = thread_clock::thread_cpu_ns() - cpu;
        let fin = self.io.get();
        let delta = codec();
        let sync = fin.sync - avant.sync;
        if op.starts_with("lecture") || op == "montage" || op == "check" {
            assert_eq!(sync, 0)
        } else {
            assert_eq!(sync, 2, "{op}")
        };
        let sync_ns = fin.sync_ns - avant.sync_ns;
        println!(
            "{}",
            json!({
                "type":"appel","profil":self.profil,"fichier":self.taille,"operation":op,"n":n,"offset":pos,"octets":len,
                "mur_ns":mur,"cpu_ns":cpu,"compression_ns":(delta.0-c.0) as u64,"decompression_ns":(delta.1-c.1) as u64,
                "lectures":fin.lus-avant.lus,"ecritures":fin.ecrits-avant.ecrits,"lectures_ns":fin.lectures_ns-avant.lectures_ns,
                "ecritures_ns":fin.ecritures_ns-avant.ecritures_ns,"flushs":sync,"flush_ns":sync_ns,
                "F1_ns":if sync==2{sync_ns-fin.derniere}else{0},"F2_ns":if sync==2{fin.derniere}else{0}
            })
        );
        Ok(v)
    }
    fn espace(&self, p: &SharedPool<Disque>, id: u64, etat: &str) -> Result<()> {
        let usage = p.usage(id)?;
        #[cfg(p6)]
        let (unites, pages, charge, physiques, economie) = {
            let c = p.compression_usage(id)?;
            (
                c.compressed_units,
                c.mapped_compressed_pages,
                c.encoded_bytes,
                c.physical_bytes,
                c.saved_bytes,
            )
        };
        #[cfg(not(p6))]
        let (unites, pages, charge, physiques, economie) = (0, 0, 0, 0, 0);
        println!(
            "{}",
            json!({"type":"espace","profil":self.profil,"fichier":self.taille,"etat":etat,
          "occupe":usage.occupied,"reserve":usage.reserved,"exclusif":usage.exclusive,"partage":usage.shared,
          "unites":unites,"pages_compressees":pages,"octets_encodes":charge,"octets_physiques_unites":physiques,"octets_gagnes_unites":economie})
        );
        Ok(())
    }
}

fn panneau(secteur: u32, politique: &str, profil: &str, len: usize, seed: u64) -> Result<()> {
    let io = Rc::new(Cell::new(Compteurs::default()));
    let m = Panneau {
        io: io.clone(),
        profil,
        taille: len,
    };
    let dev = Disque(FileImage::create(8192, secteur)?, io);
    #[cfg(p6)]
    let mut p = SharedPool::format_p6(dev, [6; 16])?;
    #[cfg(not(p6))]
    let mut p = SharedPool::format(dev, [6; 16])?;
    let o = p.create([1; 16], IDS, 1024)?;
    #[cfg(p6)]
    if politique == "lz4" {
        p.set_compression(o, novafs::Compression::Lz4)?
    }
    #[cfg(not(p6))]
    assert_eq!(politique, "none");
    p.create_file(o, "/f")?;
    let mut oracle = donnees(len, profil, seed);
    let width = len.min(16 * B);
    for (i, bytes) in oracle.chunks(width).enumerate() {
        m.mesure("ecriture_seq", i, i * width, bytes.len(), || {
            p.write_file(o, "/f", (i * width) as u64, bytes)
        })?;
    }
    m.espace(&p, o, "apres_ecriture")?;
    let read_width = len.min(4 * B);
    let mut positions: Vec<_> = (0..len / read_width).map(|i| i * read_width).collect();
    let mut rng = seed;
    for order in ["seq", "alea"] {
        if order == "alea" {
            for i in (1..positions.len()).rev() {
                let j = suivant(&mut rng) as usize % (i + 1);
                positions.swap(i, j);
            }
        }
        p.reclaim_cache()?;
        // Ouvrir le chemin avant la passe. Le premier passage chauffe les pages lues du FS.
        p.read_file(o, "/f", 0, 0)?;
        for cache in ["froid", "chaud"] {
            let op = format!("lecture_{order}_{cache}");
            for (i, &pos) in positions.iter().enumerate() {
                let bytes = m.mesure(&op, i, pos, read_width, || {
                    p.read_file(o, "/f", pos as u64, read_width)
                })?;
                assert_eq!(bytes, &oracle[pos..pos + read_width]);
            }
        }
    }
    for (i, &pos) in positions.iter().take(8).enumerate() {
        let bytes = donnees(read_width, profil, suivant(&mut rng));
        m.mesure("ecriture_alea", i, pos, read_width, || {
            p.write_file(o, "/f", pos as u64, &bytes)
        })?;
        oracle[pos..pos + read_width].copy_from_slice(&bytes);
    }
    let s = m.mesure("snapshot", 0, 0, 0, || p.snapshot(o))?;
    let c = m.mesure("clone", 0, 0, 0, || p.clone_snapshot(s, [17; 16]))?;
    let mut expected = oracle.clone();
    for (n, size) in [1, 4096, 16384].into_iter().enumerate() {
        let size = size.min(len);
        let pos = (len / 2).saturating_sub(7).min(len - size);
        let bytes = donnees(size, profil, suivant(&mut rng));
        m.mesure("partielle_clone", n, pos, size, || {
            p.write_file(c, "/f", pos as u64, &bytes)
        })?;
        expected[pos..pos + size].copy_from_slice(&bytes);
    }
    m.espace(&p, c, "apres_divergence")?;
    let dev = p.into_device();
    let p = m.mesure("montage", 0, 0, 0, || SharedPool::open(dev))?;
    let recu = p.receipt(s)?;
    let dev = p.into_device();
    let p = m.mesure("check", 0, 0, 0, || {
        // Même ouverture exhaustive et même vérification du reçu que la commande hors ligne.
        let pool = SharedPool::open(dev)?;
        pool.verify_receipt(&recu)?;
        Ok(pool)
    })?;
    for (id, file) in [(s, &oracle), (o, &oracle), (c, &expected)] {
        for (i, bytes) in file.chunks(width).enumerate() {
            assert_eq!(
                p.read_file(id, "/f", (i * width) as u64, bytes.len())?,
                bytes
            );
        }
    }
    Ok(())
}

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("horloge") {
        println!("{}", thread_clock::calibrate());
        return Ok(());
    }
    let secteur = args[1].parse().unwrap();
    let politique = args[2].clone();
    let rep: u64 = args[3].parse().unwrap();
    std::thread::Builder::new()
        .stack_size(2 * 1024 * 1024)
        .spawn(move || -> Result<()> {
            #[cfg(sondes)]
            novafs::latency_probes::begin();
            for mode in ["repetitif", "mixte", "aleatoire"] {
                for len in [B, 512 * 1024] {
                    panneau(secteur, &politique, mode, len, 0xad30813 ^ rep)?;
                }
            }
            #[cfg(sondes)]
            drop(novafs::latency_probes::finish());
            eprintln!(
                "memoire {} {}",
                heap_meter::PEAK.load(Ordering::Relaxed),
                heap_meter::current_stack_size()
            );
            Ok(())
        })?
        .join()
        .expect("thread du banc")
}
