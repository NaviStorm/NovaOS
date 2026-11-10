//! Modèle hôte indépendant. Les fichiers attendus sont de simples vecteurs
//! d'octets, ni B+Trees, ni extents, ni appels au résolveur du moteur.
use novafs::{Engine, Error, Ids, Kind, Mode, device::SimDevice};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

const IDS: Ids = Ids {
    fs: [0xa1; 16],
    lv: [0xb2; 16],
};
const STEPS: usize = 10_000;
const SEEDS: usize = 100;
struct Random(u64);
impl Random {
    fn word(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e3779b97f4a7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
        z ^ (z >> 31)
    }
    fn index(&mut self, n: usize) -> usize {
        (self.word() % n as u64) as usize
    }
}

struct ModelFile {
    id: u64,
    bytes: Vec<u8>,
}

fn path(rng: &mut Random) -> String {
    format!("/d{}/f{:02}", rng.index(4), rng.index(8))
}
fn inspect(fs: &Engine<SimDevice>, files: &BTreeMap<String, ModelFile>) {
    for d in 0..4 {
        let prefix = format!("/d{d}/");
        let expected: Vec<_> = files
            .keys()
            .filter_map(|p| p.strip_prefix(&prefix))
            .collect();
        assert_eq!(fs.list(&format!("/d{d}"), None, 64).unwrap(), expected);
    }
    for (path, file) in files {
        let stat = fs.stat(path).unwrap();
        assert_eq!(
            (stat.kind, stat.size, stat.id),
            (Kind::File, file.bytes.len() as u64, file.id)
        );
        assert_eq!(fs.read(path, 0, 65536).unwrap(), file.bytes);
    }
    fs.verify().unwrap();
}
fn run_seed(seed: u64, steps: usize) -> (usize, usize, usize) {
    let mut rng = Random(seed);
    let dev = SimDevice::new(1024, if seed.is_multiple_of(2) { 512 } else { 4096 }).unwrap();
    let mut fs = Engine::format(dev, IDS).unwrap();
    for i in 0..4 {
        fs.mkdir(&format!("/d{i}")).unwrap();
    }
    let mut next_oid = 6;
    let mut files: BTreeMap<String, ModelFile> = BTreeMap::new();
    let mut commits = 0;
    let mut shortages = 0;
    let mut mounts = 0;
    for step in 0..steps {
        let name = path(&mut rng);
        let before = fs.receipt();
        match rng.index(12) {
            0 | 1 => {
                let actual = fs.create(&name);
                if files.contains_key(&name) {
                    assert!(matches!(actual, Err(Error::Exists)));
                } else {
                    actual.unwrap();
                    files.insert(
                        name,
                        ModelFile {
                            id: next_oid,
                            bytes: vec![],
                        },
                    );
                    next_oid += 1;
                }
            }
            2 => {
                let actual = fs.unlink(&name);
                if files.remove(&name).is_some() {
                    actual.unwrap();
                } else {
                    assert!(matches!(actual, Err(Error::NotFound)));
                }
            }
            3 => {
                let dest = path(&mut rng);
                let actual = fs.rename(&name, &dest);
                if !files.contains_key(&name) {
                    assert!(matches!(actual, Err(Error::NotFound)));
                } else if name == dest {
                    actual.unwrap();
                } else if files.contains_key(&dest) {
                    assert!(matches!(actual, Err(Error::Exists)));
                } else {
                    actual.unwrap();
                    let obj = files.remove(&name).unwrap();
                    files.insert(dest, obj);
                }
            }
            4..=6 => {
                // Inclut EOF, trous, petites écritures et passages de frontières physiques.
                let off = (rng.index(3) * 4096 + rng.index(97)).saturating_sub(rng.index(17));
                let n = rng.index(257);
                let body: Vec<_> = (0..n).map(|_| rng.word() as u8).collect();
                let actual = fs.write(&name, off as u64, &body);
                if let Some(file) = files.get_mut(&name) {
                    actual.unwrap();
                    if n > 0 {
                        file.bytes.resize(file.bytes.len().max(off + n), 0);
                        file.bytes[off..off + n].copy_from_slice(&body);
                    }
                } else {
                    assert!(matches!(actual, Err(Error::NotFound)));
                }
            }
            7 => {
                let off = rng.index(8400);
                let n = rng.index(512);
                let actual = fs.read(&name, off as u64, n);
                if let Some(file) = files.get(&name) {
                    let tail = file.bytes.get(off..).unwrap_or(&[]);
                    assert_eq!(actual.unwrap(), &tail[..n.min(tail.len())]);
                } else {
                    assert!(matches!(actual, Err(Error::NotFound)));
                }
            }
            8 => {
                let actual = fs.stat(&name);
                if let Some(file) = files.get(&name) {
                    let s = actual.unwrap();
                    assert_eq!((s.id, s.size), (file.id, file.bytes.len() as u64));
                } else {
                    assert!(matches!(actual, Err(Error::NotFound)));
                }
            }
            9 => {
                let d = rng.index(4);
                let prefix = format!("/d{d}/");
                let want: Vec<_> = files
                    .keys()
                    .filter_map(|s| s.strip_prefix(&prefix))
                    .collect();
                let limit = rng.index(9);
                assert_eq!(
                    fs.list(&format!("/d{d}"), None, limit).unwrap(),
                    &want[..want.len().min(limit)]
                );
            }
            10 => {
                fs.collect().unwrap();
            }
            _ => {
                if files.contains_key(&name) {
                    fs.set_staging_budget(0);
                    assert!(matches!(fs.write(&name, 0, &[0xa5]), Err(Error::NoMemory)));
                    fs.set_staging_budget(64 * 4096);
                    assert_eq!(before, fs.receipt());
                    shortages += 1;
                } else {
                    assert!(matches!(fs.stat(&name), Err(Error::NotFound)));
                }
            }
        }
        commits += usize::from(before != fs.receipt());
        if step % 128 == 127 {
            while fs.collect().unwrap() {}
        }
        if step % 1000 == 999 || step + 1 == steps {
            let receipt = fs.receipt();
            fs = Engine::mount(fs.unmount().unwrap(), IDS, Mode::ReadWrite).unwrap();
            assert_eq!(receipt, fs.receipt());
            inspect(&fs, &files);
            mounts += 1;
        }
    }
    (commits, shortages, mounts)
}

#[test]
fn reference_smoke() {
    let (commits, failures, mounts) = run_seed(5, 320);
    assert!(commits > 0 && failures > 0 && mounts == 1);
}

#[test]
#[ignore = "campagne différentielle complète ; utiliser --ignored --nocapture"]
fn one_million_model_operations() {
    let start = Instant::now();
    let next = AtomicUsize::new(0);
    let totals = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..4)
            .map(|_| {
                scope.spawn(|| {
                    let mut total = (0, 0, 0);
                    loop {
                        let seed = next.fetch_add(1, Ordering::Relaxed);
                        if seed >= SEEDS {
                            break;
                        }
                        let (commits, failures, mounts) = run_seed(seed as u64, STEPS);
                        total.0 += commits;
                        total.1 += failures;
                        total.2 += mounts;
                    }
                    total
                })
            })
            .collect();
        workers
            .into_iter()
            .map(|h| h.join().unwrap())
            .fold((0, 0, 0), |a, v| (a.0 + v.0, a.1 + v.1, a.2 + v.2))
    });
    assert_eq!(totals.2, SEEDS * 10);
    println!(
        "DIFFERENTIEL graines={SEEDS} opérations={} commits_observés={} pénuries_injectées={} remontages_et_audits={} secondes={:.3}",
        SEEDS * STEPS,
        totals.0,
        totals.1,
        totals.2,
        start.elapsed().as_secs_f64()
    );
}
