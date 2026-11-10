//! Horloge CPU du thread et contrôle des unités avant chaque banc Darwin.
use std::time::{Duration, Instant};

fn darwin_nsec() -> u64 {
    unsafe extern "C" {
        fn clock_gettime_nsec_np(clock_id: u32) -> u64;
    }
    // Sûreté : CLOCK_THREAD_CPUTIME_ID = 16 dans le SDK, sans pointeur.
    unsafe { clock_gettime_nsec_np(16) }
}

#[cfg(target_arch = "aarch64")]
pub fn thread_cpu_ns() -> u64 {
    darwin_nsec()
}

#[cfg(target_arch = "x86_64")]
pub fn thread_cpu_ns() -> u64 {
    unsafe extern "C" {
        fn pthread_self() -> *mut std::ffi::c_void;
        fn pthread_mach_thread_np(thread: *mut std::ffi::c_void) -> u32;
        fn thread_info(thread: u32, flavor: i32, info: *mut i32, n: *mut u32) -> i32;
    }
    let mut info = [0i32; 10];
    let mut count = info.len() as u32;
    // Sûreté : le port emprunté désigne notre pthread vivant. THREAD_BASIC_INFO = 3
    // écrit au plus les dix entiers 32 bits annoncés. Le port reste la propriété du pthread.
    let result = unsafe {
        thread_info(
            pthread_mach_thread_np(pthread_self()),
            3,
            info.as_mut_ptr(),
            &mut count,
        )
    };
    assert!(result == 0 && count == 10, "échec Mach THREAD_BASIC_INFO");
    // Les deux premiers time_value_t décrivent les temps utilisateur et noyau,
    // chacun en secondes et microsecondes signées non négatives.
    (info[0] as u64 + info[2] as u64) * 1_000_000_000 + (info[1] as u64 + info[3] as u64) * 1000
}

#[repr(C)]
#[derive(Default)]
struct Timeval {
    seconds: i64,
    micros: i32,
}

#[repr(C)]
#[derive(Default)]
struct Rusage {
    user: Timeval,
    system: Timeval,
    counters: [i64; 14],
}
const _: () = assert!(size_of::<Rusage>() == 144);

fn process_cpu_ns() -> u64 {
    unsafe extern "C" {
        fn getrusage(who: i32, usage: *mut Rusage) -> i32;
    }
    let mut usage = Rusage::default();
    // Sûreté : le SDK Darwin 64 bits définit RUSAGE_SELF = 0 et cette structure
    // contient deux timeval puis quatorze long. Le noyau remplit un tampon valide.
    assert_eq!(unsafe { getrusage(0, &mut usage) }, 0);
    (usage.user.seconds as u64 + usage.system.seconds as u64) * 1_000_000_000
        + (usage.user.micros as u64 + usage.system.micros as u64) * 1000
}

/// À appeler avant de créer les threads de travail. Comparer une horloge du
/// thread avec getrusage du processus monothread fournit un second chemin de mesure.
pub fn calibrate() -> serde_json::Value {
    let measure = |work: &dyn Fn()| {
        let start = Instant::now();
        let process = process_cpu_ns();
        let old = darwin_nsec();
        let chosen = thread_cpu_ns();
        work();
        let chosen = thread_cpu_ns() - chosen;
        let old = darwin_nsec() - old;
        let process = process_cpu_ns() - process;
        (start.elapsed().as_nanos() as u64, process, chosen, old)
    };
    let burn = measure(&|| {
        let start_cpu = process_cpu_ns();
        let start_wall = Instant::now();
        let mut state = 1u64;
        while process_cpu_ns() - start_cpu < 120_000_000 {
            assert!(
                start_wall.elapsed() < Duration::from_secs(15),
                "étalonnage CPU interrompu"
            );
            for _ in 0..65_536 {
                state = std::hint::black_box(state.wrapping_mul(3).wrapping_add(1));
            }
        }
    });
    let sleep = measure(&|| std::thread::sleep(Duration::from_millis(120)));
    assert!(
        burn.2.abs_diff(burn.1) <= burn.1 / 20,
        "échelle CPU erronée : {burn:?}"
    );
    assert!(
        sleep.0 >= 120_000_000 && sleep.1 < 12_000_000 && sleep.2 < 12_000_000,
        "la pause a été comptabilisée comme du CPU : {sleep:?}"
    );
    let row = |(wall, process, chosen, old)| {
        serde_json::json!({
            "mur_ns": wall, "getrusage_processus_ns": process,
            "horloge_thread_retenue_ns": chosen, "clock_gettime_nsec_np_brut": old,
        })
    };
    serde_json::json!({
        "architecture": std::env::consts::ARCH,
        "horloge_retenue": if cfg!(target_arch = "x86_64") { "Mach THREAD_BASIC_INFO" } else { "clock_gettime_nsec_np" },
        "charge": row(burn), "pause": row(sleep),
    })
}
