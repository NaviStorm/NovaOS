//! Banc sur images jetables Darwin. Les constantes d'interopérabilité sont des
//! vecteurs publics réservés au test. L'entropie de production reste celle de l'hôte.
#[path = "../lab/p6_donnees.rs"]
mod corpus;
#[path = "../lab/heap_meter.rs"]
#[allow(dead_code)]
mod heap_meter;
#[path = "../lab/thread_clock.rs"]
mod thread_clock;
use novafs::crypto::{Credential, HostEntropy, Secret};
use novafs::device::FileImage;
use novafs::secure_pool::SecurePool;
use novafs::shared::SharedPool;
use novafs::{Block, Compression, Device, Error, Ids, Result};
use serde_json::json;
use std::cell::Cell;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::rc::Rc;
use std::sync::atomic::Ordering::Relaxed;
use std::time::Instant;

const BLOCKS: u64 = 4096;
const BYTES: usize = 16384;
const OWNER: [u8; 16] = [7; 16];
const IDS: Ids = Ids {
    fs: [9; 16],
    lv: [12; 16],
};
const POOL: [u8; 16] = [38; 16];
const DEST: [u8; 16] = [42; 16];
const PHRASE: &[u8] = b"vecteur synthetique du laboratoire P7";

#[derive(Clone, Copy, Default)]
struct Counters {
    writes: u64,
    syncs: u64,
    sync_ns: u64,
}
struct Measured(FileImage, Rc<Cell<Counters>>);
impl Device for Measured {
    fn blocks(&self) -> u64 {
        self.0.blocks()
    }
    fn read(&self, b: u64) -> Result<Block> {
        self.0.read(b)
    }
    fn write(&mut self, b: u64, page: &Block) -> Result<()> {
        self.0.write(b, page)?;
        let mut c = self.1.get();
        c.writes += 1;
        self.1.set(c);
        Ok(())
    }
    fn flush(&mut self) -> Result<()> {
        let t = Instant::now();
        self.0.flush()?;
        let mut c = self.1.get();
        c.syncs += 1;
        c.sync_ns += t.elapsed().as_nanos() as u64;
        self.1.set(c);
        Ok(())
    }
}

struct Panel {
    io: Rc<Cell<Counters>>,
    stack: usize,
}
impl Panel {
    fn timed<T>(&self, op: &str, bytes: usize, run: impl FnOnce() -> Result<T>) -> Result<T> {
        let old = self.io.get();
        heap_meter::PHASE_PEAK.store(heap_meter::LIVE.load(Relaxed), Relaxed);
        let cpu = thread_clock::thread_cpu_ns();
        let t = Instant::now();
        let v = run()?;
        let ns = t.elapsed().as_nanos() as u64;
        let cpu_ns = thread_clock::thread_cpu_ns() - cpu;
        let heap = heap_meter::PHASE_PEAK.load(Relaxed);
        let after = self.io.get();
        println!(
            "{}",
            json!({"op":op,"octets":bytes,"mur_ns":ns,"cpu_ns":cpu_ns,"tas_octets":heap,"pile_reservee_octets":self.stack,"ecritures_blocs":after.writes-old.writes,"flushs":after.syncs-old.syncs,"flush_ns":after.sync_ns-old.sync_ns})
        );
        Ok(v)
    }
}

enum Pool {
    P6(SharedPool<Measured>),
    P7(SecurePool<Measured>),
}
impl Pool {
    fn medium(self) -> Measured {
        match self {
            Self::P6(p) => p.into_device(),
            Self::P7(p) => p.into_device(),
        }
    }
}
macro_rules! call {
    ($pool:expr,$op:ident($($arg:expr),*))=>{match &mut $pool {
        Pool::P6(p)=>p.$op($($arg),*), Pool::P7(p)=>p.$op($($arg),*)
    }};
}

fn bench(sector: u32, version: &str, policy: Compression, pattern: &str, seed: u64) -> Result<()> {
    let encrypted = version == "p7-chiffre";
    let p7 = version != "p6";
    let io = Rc::new(Cell::new(Counters::default()));
    let timed = Panel {
        io: io.clone(),
        stack: heap_meter::current_stack_size(),
    };
    let fresh =
        || -> Result<Measured> { Ok(Measured(FileImage::create(BLOCKS, sector)?, io.clone())) };
    let admin = Secret::generate(&mut HostEntropy)?;
    let recovery = Secret::generate(&mut HostEntropy)?;
    let backup = Secret::generate(&mut HostEntropy)?;
    let mut p = if p7 {
        Pool::P7(SecurePool::format(fresh()?, POOL, &admin)?)
    } else {
        Pool::P6(SharedPool::format_p6(fresh()?, POOL)?)
    };
    let id = timed.timed("creation_volume", 0, || match &mut p {
        Pool::P6(p) => p.create_with_compression(OWNER, IDS, 256, policy),
        Pool::P7(p) => p.create(
            OWNER,
            IDS,
            256,
            policy,
            encrypted.then_some((PHRASE, &recovery)),
        ),
    })?;
    call!(p, create_file(id, "/f"))?;
    let mut data = corpus::donnees(BYTES, pattern, seed);
    timed.timed("ecriture_initiale", BYTES, || {
        call!(p, write_file(id, "/f", 0, &data))
    })?;
    let snapshot = timed.timed("snapshot", 0, || call!(p, snapshot(id)))?;
    let clone = timed.timed("clone", 0, || call!(p, clone_snapshot(snapshot, [31; 16])))?;
    for i in 0..32u64 {
        let pos = (i as usize % 2) * BYTES / 2;
        let delta = corpus::donnees(BYTES / 2, pattern, seed + 327 + i);
        timed.timed("ecriture_clone", delta.len(), || {
            call!(p, write_file(clone, "/f", pos as u64, &delta))
        })?;
        data[pos..pos + delta.len()].copy_from_slice(&delta);
        let got = timed.timed("lecture_clone", BYTES, || {
            call!(p, read_file(clone, "/f", 0, BYTES))
        })?;
        assert_eq!(got, data);
    }
    let disk = p.medium();
    let mut p = timed.timed("montage", 0, || {
        Ok(if p7 {
            Pool::P7(SecurePool::open(disk, POOL, &admin)?)
        } else {
            Pool::P6(SharedPool::open(disk)?)
        })
    })?;
    if let Pool::P7(p) = &mut p
        && encrypted
    {
        let domain = p.domain(clone)?;
        timed.timed("argon2id_deverrouillage", 0, || {
            p.unlock(domain, Credential::Passphrase(PHRASE))
        })?;
        p.lock(domain);
        timed.timed("recuperation_deverrouillage", 0, || {
            p.unlock(domain, Credential::Recovery(&recovery))
        })?;
    }
    let receipt = if let Pool::P6(p) = &p {
        Some(p.receipt(clone)?)
    } else {
        None
    };
    let mut p = timed.timed("verification", 0, || match p {
        Pool::P6(p) => {
            let p = SharedPool::open(p.into_device())?;
            p.verify_receipt(&receipt.unwrap())?;
            Ok(Pool::P6(p))
        }
        Pool::P7(mut p) => {
            p.check()?;
            Ok(Pool::P7(p))
        }
    })?;
    let mut archive = Vec::new();
    timed.timed("export", BYTES, || match &mut p {
        Pool::P6(p) => novafs::shared_export::export_view(p, clone, sector, &mut archive),
        Pool::P7(p) => p.export(clone, sector, &backup, &mut archive),
    })?;
    drop(p);
    let mut dest = if p7 {
        Pool::P7(SecurePool::format(fresh()?, DEST, &admin)?)
    } else {
        Pool::P6(SharedPool::format_p6(fresh()?, DEST)?)
    };
    let rid = timed.timed("restauration", BYTES, || match &mut dest {
        Pool::P6(p) => {
            novafs::shared_export::restore_with_compression(p, &archive[..], OWNER, policy)
        }
        Pool::P7(p) => p.restore(&archive[..], &backup, OWNER, PHRASE, &recovery, policy),
    })?;
    assert_eq!(call!(dest, read_file(rid, "/f", 0, BYTES))?, data);
    Ok(())
}

fn crypto() -> Result<()> {
    let timed = Panel {
        io: Rc::default(),
        stack: heap_meter::current_stack_size(),
    };
    let k = Secret::generate(&mut HostEntropy)?;
    let mut b = [71; 4096];
    for _ in 0..128 {
        let (n, t) = timed.timed("aead_scellement_csprng", b.len(), || {
            k.seal(&mut HostEntropy, b"banc", &mut b)
        })?;
        timed.timed("aead_ouverture", b.len(), || {
            k.open(&n, &t, b"banc", &mut b)
        })?;
        assert_eq!(b, [71; 4096]);
    }
    Ok(())
}

fn exchange(action: &str, sector: u32, dir: &Path) -> Result<()> {
    // Vecteurs publiquement connus : seules les images et archives de synthèse les utilisent.
    let admin = Secret::from_bytes([11; 32]);
    let recovery = Secret::from_bytes([23; 32]);
    let backup = Secret::from_bytes([37; 32]);
    let image = dir.join("pool");
    let archive = dir.join("archive");
    let data = corpus::donnees(BYTES, "repetitif", 0xaf);
    match action {
        "produire" => {
            let file = OpenOptions::new()
                .create_new(true)
                .read(true)
                .write(true)
                .open(image)?;
            file.set_len(BLOCKS * 4096)?;
            let mut p =
                SecurePool::format(FileImage::from_file(file, BLOCKS, sector)?, POOL, &admin)?;
            let o = p.create(OWNER, IDS, 256, Compression::Lz4, Some((PHRASE, &recovery)))?;
            p.create_file(o, "/f")?;
            p.write_file(o, "/f", 0, &data)?;
            let s = p.snapshot(o)?;
            let c = p.clone_snapshot(s, [15; 16])?;
            p.write_file(c, "/f", 9, b"CoW")?;
            p.remove(o)?;
            let mut out = OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(archive)?;
            p.export(c, sector, &backup, &mut out)?;
            out.flush()?;
        }
        "lire" => {
            let mut p = SecurePool::open(
                FileImage::from_file(File::open(image)?, BLOCKS, sector)?,
                POOL,
                &admin,
            )?;
            let list = p.list()?;
            let s = list[0].id;
            let c = list[1].id;
            assert!(p.read_file(c, "/f", 0, BYTES).is_err());
            let domain = p.domain(c)?;
            p.unlock(domain, Credential::Recovery(&recovery))?;
            p.check()?;
            assert_eq!(p.read_file(s, "/f", 0, BYTES)?, data);
            let mut changed = data;
            changed[9..12].copy_from_slice(b"CoW");
            assert_eq!(p.read_file(c, "/f", 0, BYTES)?, changed);
            println!("{}", json!({"domaine_source":domain}));
        }
        "restaurer" => {
            assert!(!image.exists());
            let mut p = SecurePool::format(FileImage::create(BLOCKS, sector)?, DEST, &admin)?;
            let c = p.restore(
                File::open(archive)?,
                &backup,
                OWNER,
                PHRASE,
                &recovery,
                Compression::None,
            )?;
            let mut changed = data;
            changed[9..12].copy_from_slice(b"CoW");
            assert_eq!(p.read_file(c, "/f", 0, BYTES)?, changed);
            p.check()?;
            println!("{}", json!({"domaine_restaure":p.domain(c)?}));
        }
        _ => return Err(Error::Invalid("commande d'échange inconnue")),
    }
    Ok(())
}

fn worker(args: &[String]) -> Result<()> {
    match args {
        [op] if op == "horloge" => {
            println!("{}", thread_clock::calibrate());
            Ok(())
        }
        [op] if op == "crypto" => crypto(),
        [op, sector, version, codec, pattern, seed] if op == "mesure" => {
            let policy = match codec.as_str() {
                "none" => Compression::None,
                "lz4" => Compression::Lz4,
                _ => return Err(Error::Invalid("codec")),
            };
            if !["p6", "p7-clair", "p7-chiffre"].contains(&version.as_str()) {
                return Err(Error::Invalid("format"));
            }
            // Amorcer le tampon stdout du banc avant la mesure des allocations vivantes.
            println!("{{\"repere\":\"debut\"}}");
            let before = heap_meter::LIVE.load(Relaxed);
            bench(
                sector.parse().map_err(|_| Error::Invalid("secteur"))?,
                version,
                policy,
                pattern,
                seed.parse().map_err(|_| Error::Invalid("graine"))?,
            )?;
            let after = heap_meter::LIVE.load(Relaxed);
            println!(
                "{}",
                json!({"repere":"fin","tas_avant":before,"tas_apres":after})
            );
            assert_eq!(
                before, after,
                "tas vivant après restitution de tous les objets"
            );
            Ok(())
        }
        [op, sector, dir] => exchange(
            op,
            sector.parse().map_err(|_| Error::Invalid("secteur"))?,
            Path::new(dir),
        ),
        _ => Err(Error::Invalid("arguments du banc")),
    }
}

fn main() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    std::thread::Builder::new()
        .stack_size(2 * 1024 * 1024)
        .spawn(move || worker(&args))?
        .join()
        .expect("thread du banc")
}
