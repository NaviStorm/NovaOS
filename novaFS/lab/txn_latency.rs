//! Distribution des phases d'écriture sur une image macOS et rang réel de F2.
mod thread_clock;

use novafs::device::FileImage;
use novafs::pool::{GRAIN, Pool, PoolCoordinator, Type, Uid, WriteRequest};
use novafs::{Block, Device, Engine, Error, Ids, Result};
use serde_json::{Value, json};
use std::cell::Cell;
use std::sync::Barrier;
use std::time::{Duration, Instant};
use thread_clock::thread_cpu_ns;

const FILE: &str = "/journal";
const VOLUMES: [Uid; 2] = [[1; 16], [2; 16]];
const PHASES: [&str; 8] = [
    "attente_prepare",
    "prepare",
    "attente_commit",
    "commit",
    "f1",
    "f2",
    "appels_busy_prepare",
    "appels_busy_commit",
];
const SAMPLES: usize = 128;
const CAMPAIGNS: usize = 3;

#[derive(Clone, Copy)]
struct Start {
    wall: Instant,
    cpu: u64,
}

#[derive(Clone, Copy, Default)]
struct Elapsed {
    wall: u64,
    cpu: u64,
}

impl Start {
    fn now() -> Self {
        Self {
            wall: Instant::now(),
            cpu: thread_cpu_ns(),
        }
    }

    fn since(self, before: Self) -> Elapsed {
        Elapsed {
            wall: (self.wall - before.wall).as_nanos().try_into().unwrap(),
            cpu: self.cpu.checked_sub(before.cpu).unwrap(),
        }
    }
}

#[derive(Clone, Copy, Default)]
struct Trace {
    flush: [Elapsed; 2],
    // Avant F1, entre F1 et F2, après F2.
    writes: [u32; 3],
    count: usize,
    rank: usize,
}

thread_local! {
    static ACTIVE: Cell<Option<Trace>> = const { Cell::new(None) };
}

struct MeasuredDevice {
    image: FileImage,
    // Accès sous l'exclusion du périphérique, au retour réussi de F2.
    completed: usize,
}

impl Device for MeasuredDevice {
    fn blocks(&self) -> u64 {
        self.image.blocks()
    }

    fn read(&self, block: u64) -> Result<Block> {
        self.image.read(block)
    }

    fn write(&mut self, block: u64, data: &Block) -> Result<()> {
        self.image.write(block, data)?;
        ACTIVE.update(|sample| {
            sample.map(|mut sample| {
                sample.writes[sample.count] += 1;
                sample
            })
        });
        Ok(())
    }

    fn flush(&mut self) -> Result<()> {
        let Some(mut trace) = ACTIVE.get() else {
            return self.image.flush();
        };
        let start = Start::now();
        self.image.flush()?;
        trace.flush[trace.count] = Start::now().since(start);
        trace.count += 1;
        if trace.count == 2 {
            self.completed += 1;
            trace.rank = self.completed;
        }
        ACTIVE.set(Some(trace));
        Ok(())
    }
}

fn fixture(sector: u32) -> Result<PoolCoordinator<MeasuredDevice>> {
    let device = MeasuredDevice {
        image: FileImage::create(3 * GRAIN, sector)?,
        completed: 0,
    };
    let mut pool = Pool::format(device)?;
    for lv in VOLUMES {
        let fs = [lv[0] + 10; 16];
        pool.create(lv, Some(fs), 1)?;
        {
            let mut engine = Engine::format(pool.view(lv, Type::Preparing, fs)?, Ids { lv, fs })?;
            engine.create(FILE)?;
            engine.write(FILE, 0, &[0; 4096])?;
        }
        pool.mark_ready(lv, fs)?;
    }
    PoolCoordinator::mount(pool)
}

struct Attempts {
    wait: Elapsed,
    accepted: Elapsed,
    rejected: Elapsed,
    busy: u32,
}

// Chaque attente commence à la première tentative de la phase, se termine
// à l'entrée de celle qui réussit et vaut zéro si la première réussit.
fn retry<T>(mut action: impl FnMut() -> Result<T>) -> Result<(T, Attempts)> {
    let first = Start::now();
    let mut attempt = first;
    let mut busy = 0;
    let mut rejected = Elapsed::default();
    loop {
        let result = action();
        let end = Start::now();
        let elapsed = end.since(attempt);
        match result {
            Ok(value) => {
                return Ok((
                    value,
                    Attempts {
                        wait: attempt.since(first),
                        accepted: elapsed,
                        rejected,
                        busy,
                    },
                ));
            }
            Err(Error::Busy) if end.wall - first.wall < Duration::from_secs(15) => {
                busy += 1;
                rejected.wall += elapsed.wall;
                rejected.cpu += elapsed.cpu;
                std::thread::sleep(Duration::from_micros(50));
                attempt = Start::now();
            }
            Err(error) => return Err(error),
        }
    }
}

struct Sample {
    index: usize,
    rank: usize,
    busy: [u32; 2],
    phases: [Elapsed; 8],
}

fn writer(
    mut request: WriteRequest<'_, MeasuredDevice>,
    barrier: &Barrier,
    index: usize,
) -> Result<Sample> {
    let mut payload = [0x91; 4096];
    payload[..8].copy_from_slice(&(index as u64 + 1).to_le_bytes());
    barrier.wait();
    let (_, prepare) = retry(|| request.prepare(FILE, 0, &payload))?;
    ACTIVE.set(Some(Trace::default()));
    let (head, commit) = retry(|| request.commit())?;
    let trace = ACTIVE.replace(None).expect("trace du ticket");
    assert_eq!(trace.count, 2);
    assert!(trace.writes[0] > 0);
    assert_eq!(trace.writes[1..], [1, 0]);
    // Vérifie le rang global entre les LV et la génération individuelle.
    assert_eq!(trace.rank, index + 1);
    assert_eq!(head.birth, 4 + (index / 2) as u64);
    Ok(Sample {
        index,
        rank: trace.rank,
        busy: [prepare.busy, commit.busy],
        phases: [
            prepare.wait,
            prepare.accepted,
            commit.wait,
            commit.accepted,
            trace.flush[0],
            trace.flush[1],
            prepare.rejected,
            commit.rejected,
        ],
    })
}

fn distribution(mut values: Vec<u64>) -> [u64; 4] {
    values.sort_unstable();
    let q = |n: usize| values[(n * values.len()).div_ceil(100) - 1];
    [q(50), q(95), q(99), *values.last().unwrap()]
}

fn run(campaign: usize, sector: u32, clients: usize, samples: usize) -> Result<Value> {
    let pool = fixture(sector)?;
    let mut records = Vec::with_capacity(samples);
    for start in (0..samples).step_by(clients) {
        let mut queue = Vec::with_capacity(clients);
        for id in start..start + clients {
            queue.push((id, pool.request_write(VOLUMES[id % 2])?));
        }
        let barrier = Barrier::new(clients);
        std::thread::scope(|scope| -> Result<()> {
            // Les derniers admis sont lancés en premier. Le rendez-vous
            // exclut l'attente de création des threads des phases mesurées.
            let mut jobs = Vec::with_capacity(clients);
            for (id, request) in queue.into_iter().rev() {
                let barrier = &barrier;
                jobs.push(scope.spawn(move || writer(request, barrier, id)));
            }
            for job in jobs {
                records.push(job.join().expect("thread du banc")?);
            }
            Ok(())
        })?;
    }
    records.sort_by_key(|s| s.index);
    if clients > 1 {
        assert!(
            records.iter().any(|s| s.busy != [0, 0]),
            "contention attendue"
        );
    } else {
        assert!(records.iter().all(|s| s.busy == [0, 0]));
    }
    let pool = PoolCoordinator::mount(Pool::import(pool.into_device()?)?)?;
    for (i, lv) in VOLUMES.iter().enumerate() {
        let final_serial = (samples - 1 + i) as u64;
        assert_eq!(
            pool.request_read(*lv)?.read(FILE, 0, 8)?,
            final_serial.to_le_bytes()
        );
        pool.request_read(*lv)?.verify()?;
    }
    let rows: Vec<Vec<u64>> = records
        .iter()
        .map(|s| {
            let mut row = vec![
                s.index as u64 + 1,
                s.rank as u64,
                s.busy[0].into(),
                s.busy[1].into(),
            ];
            for phase in s.phases {
                row.extend([phase.wall, phase.cpu]);
            }
            row
        })
        .collect();
    let summary: Value = PHASES
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let wall = distribution(records.iter().map(|s| s.phases[i].wall).collect());
            let cpu = distribution(records.iter().map(|s| s.phases[i].cpu).collect());
            (name.to_string(), json!({"mur_ns": wall, "cpu_ns": cpu}))
        })
        .collect::<serde_json::Map<_, _>>()
        .into();
    Ok(json!({
        "campagne": campaign + 1, "secteur": sector, "clients": clients,
        "transactions": records.len(), "distributions": summary, "echantillons": rows,
    }))
}

fn main() -> Result<()> {
    let (campaigns, samples) = match std::env::args().skip(1).collect::<Vec<_>>().as_slice() {
        [] => (CAMPAIGNS, SAMPLES),
        [option] if option == "--smoke" => (1, 8),
        [option] if option == "--help" => {
            println!(
                "txn-latency [--smoke]     3 campagnes de 128 transactions par condition, ou 1 campagne de 8"
            );
            return Ok(());
        }
        _ => return Err(Error::Invalid("usage : txn-latency [--smoke]")),
    };
    let calibration = thread_clock::calibrate();
    let mut columns = vec![
        "ticket".into(),
        "rang_f2".into(),
        "busy_prepare".into(),
        "busy_commit".into(),
    ];
    for name in PHASES {
        columns.push(format!("{name}_mur_ns"));
        columns.push(format!("{name}_cpu_ns"));
    }
    let mut series = Vec::new();
    for campaign in 0..campaigns {
        // Change l'ordre des conditions de mesure d'une campagne à l'autre.
        for &sector in if campaign % 2 == 0 {
            &[512, 4096]
        } else {
            &[4096, 512]
        } {
            for &clients in if campaign % 2 == 0 { &[1, 8] } else { &[8, 1] } {
                series.push(run(campaign, sector, clients, samples)?);
            }
        }
    }
    println!(
        "{}",
        json!({
            "architecture_binaire": std::env::consts::ARCH, "calibration_cpu": calibration,
            "octets_par_transaction": 4096,
            "quantiles_rang_superieur": ["p50", "p95", "p99", "maximum"],
            "colonnes": columns, "series": series
        })
    );
    Ok(())
}
