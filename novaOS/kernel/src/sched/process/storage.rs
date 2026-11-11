//! Publication privée de la session de stockage par le lanceur de confiance.
use super::*;
use crate::abi::{CLOSED, DENIED};

const SERVICE: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/storage-service.bin"));
const CLIENT: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/storage-client.bin"));
const FAILURE: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/storage-failure.bin"));

enum Lifetime {
    Resident,
    BeforeHalt,
    Qualification,
}

// L'observateur d'un service résident dort jusqu'à la collecte de son espace.
// Le même masque couvre le prédicat et l'inscription comme dans PROCESS_WAIT.
fn wait(child: Child) -> Outcome {
    loop {
        let _irq = Guard::new();
        if let Some(outcome) = child.try_result() {
            return outcome;
        }
        dispatch(Reason::Block(Wait::Process(child.0)));
    }
}

fn supervise(first: &'static [u8], lifetime: Lifetime) -> bool {
    for (attempt, image) in [first, SERVICE].into_iter().enumerate() {
        let qualification = matches!(lifetime, Lifetime::Qualification);
        let (mut service, client) =
            match ipc::launch_session(image, CLIENT, if qualification { 0 } else { 1 }) {
                Ok(pair) => pair,
                Err(status) => {
                    hal::log(format_args!("STOCKAGE publication refusee code={status}"));
                    continue;
                }
            };
        let (server_pid, client_pid) = (service.process.0.number, client.0.number);
        let peer = wait(client);
        // La fin du client précède parfois le retour du serveur de son SEND.
        let deadline = ticks() + u64::from(HZ);
        while peer == Outcome::Exit(0)
            && !service.listening()
            && ticks() < deadline
            && service.process.try_result().is_none()
        {
            yield_now();
        }
        let ready = peer == Outcome::Exit(0) && service.listening();
        if ready {
            if matches!(lifetime, Lifetime::Resident) {
                measure("stockage_boot");
            }
            hal::log(format_args!(
                "STOCKAGE session={} service={server_pid} client={client_pid} INFO volumes=0 FIN confirmee",
                attempt + 1
            ));
            if qualification {
                measure("stockage_resident");
                let next = service.next_client(1).unwrap();
                let pid = next.0.number;
                assert_eq!(wait(next), Outcome::Exit(0));
                hal::log(format_args!(
                    "STOCKAGE seconde session service={server_pid} client={pid}"
                ));
            }
        }
        if !ready || !matches!(lifetime, Lifetime::Resident) {
            service.shutdown();
        }
        let outcome = wait(service.process);
        hal::log(format_args!(
            "STOCKAGE arret service={server_pid} resultats={outcome:?},{peer:?}"
        ));
        if ready && outcome == Outcome::Exit(0) && !matches!(lifetime, Lifetime::Resident) {
            return true;
        }
        if qualification && attempt == 0 {
            assert_eq!(outcome, Outcome::Exit(70));
            assert!(matches!(peer, Outcome::Exit(CLOSED | DENIED)));
        }
    }
    false
}

pub(crate) fn boot() {
    if let Err(e) = spawn(|| {
        if !supervise(SERVICE, Lifetime::Resident) {
            hal::log(format_args!("STOCKAGE indisponible apres deux tentatives"));
        }
    }) {
        hal::log(format_args!("STOCKAGE superviseur indisponible : {e:?}"));
    }
}

pub(crate) fn before_halt() {
    supervise(SERVICE, Lifetime::BeforeHalt);
}

pub(crate) fn test() {
    let base = heap::audit(true);
    measure("stockage_repos");
    heap::with_pages(|m| {
        m.begin_frame_sample();
        Ok(())
    })
    .unwrap();
    let outsider = spawn_image(CLIENT, &[], None, None).unwrap();
    assert_eq!(outsider.join(), Outcome::Exit(DENIED));
    assert!(supervise(FAILURE, Lifetime::Qualification));
    assert_eq!(heap::audit(true), base);
    let usage = heap::with_pages(|m| Ok(m.end_frame_sample())).unwrap();
    hal::log(format_args!(
        "STOCKAGE cadres allocations={} pic={} blocs_repos={} buddy_octets={}",
        usage.allocations,
        base.free_pages - usage.minimum_free,
        base.blocks,
        base.occupied
    ));
    measure("stockage_recupere");

    let hz = hal::timer::counter_hz();
    let (service, client, begin, published_pages) = {
        let _guard = Guard::new();
        let (mut service, client) = ipc::launch_session(SERVICE, CLIENT, 512).unwrap();
        assert!(matches!(service.next_client(1), Err(crate::abi::LIMIT)));
        measure("stockage_publie");
        let pages = base.free_pages - heap::audit(false).free_pages;
        ipc::begin_measure(client.0, service.process.0);
        (service, client, hal::timer::counter(), pages)
    };
    assert_eq!(client.join(), Outcome::Exit(0));
    service.shutdown();
    assert_eq!(wait(service.process), Outcome::Exit(0));
    let elapsed = hal::timer::counter() - begin;
    ipc::end_measure(hz);
    let duration = u128::from(elapsed) * 1_000_000_000 / u128::from(hz);
    hal::log(format_args!(
        "STOCKAGE campagne tours=513 ns={duration} hz={hz}"
    ));

    // Le dernier cadre du lancement est celui du second endpoint. Vérifier
    // le rollback après publication interne de la première moitié de la paire.
    heap::audit(true);
    let remaining = published_pages - 1;
    let mut held = heap::with_pages(|m| m.hold_for_probe(remaining)).unwrap();
    assert!(matches!(
        ipc::launch_session(SERVICE, CLIENT, 1),
        Err(crate::abi::NO_MEMORY)
    ));
    assert_eq!(heap::audit(false).free_pages, remaining);
    heap::with_pages(|m| {
        held.restore_to(m, base.free_pages);
        Ok(())
    })
    .unwrap();
    hal::log(format_args!(
        "STOCKAGE publication annulee budget={remaining}"
    ));
    let after = heap::audit(true);
    assert_eq!(after, base);
    assert!(with(
        |s| s.processes.slots.iter().all(Option::is_none) && s.ipc.pages() == 0
    ));
    measure("stockage_detruit");
    hal::log(format_args!("STOCKAGE validation OK"));
}

const RELAY: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/storage-relay.bin"));
const DISK: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/storage-disk.bin"));
const REQUESTER: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/storage-requester.bin"));
pub(crate) fn qualify_transport(delayed: bool) {
    use crate::hardware::block;
    let mut witness: Option<block::Witness> = None;
    let mut reference = None;
    for &mode in if delayed {
        &[2u64, 1][..]
    } else {
        &[1, 2, 1][..]
    } {
        if delayed && mode == 2 {
            hal::log(format_args!("DMA injection prete"));
            sleep(3 * u64::from(HZ));
        }
        let starts = block::kicks();
        let key = block::begin_session(1 | 8).expect("ouvrir une session en lecture seulement");
        let old_frame = block::private_frames().unwrap()[20];
        assert!(block::begin_session(1).is_err());
        let [relay, client, driver] =
            ipc::launch_block([(RELAY, 15), (REQUESTER, 1), (DISK, 0)], key, mode).unwrap();
        let driver_pid = driver.0.number;
        let driver_result = wait(driver);
        assert!(!block::admit(driver_pid, key));
        if delayed && mode == 2 {
            let outstanding = block::inflight();
            hal::log(format_args!(
                "DMA pilote mort transfert_en_vol={outstanding}"
            ));
            assert!(outstanding, "la lecture doit encore être en vol");
        }
        assert!(
            block::publication_unique(),
            "index réellement publié dans la file DMA"
        );
        let mut retired = block::retire();
        if delayed && mode == 2 && retired == Err(block::Error::Quarantine) {
            hal::log(format_args!("DMA quarantaine cadres epingles"));
            for _ in 0..3 {
                sleep(u64::from(HZ));
                retired = block::retire();
                if retired.is_ok() {
                    break;
                }
            }
        }
        retired.expect("retrait du bail");
        if mode == 2 {
            witness =
                Some(block::Witness::claim(old_frame).expect("réattribution de la page retirée"))
        } else if let Some(w) = witness.take() {
            w.verify().expect("témoin du propriétaire suivant")
        }
        let results = (wait(relay), wait(client), driver_result);
        if witness.is_none() {
            let snapshot = heap::audit(true);
            if let Some(old) = reference {
                assert_eq!(snapshot, old, "fuite entre sessions chaudes")
            }
            reference = Some(snapshot);
        }
        assert_eq!(
            block::kicks() - starts,
            1,
            "seule la demande valide peut notifier"
        );
        hal::log(format_args!(
            "DMA utilisateurs mode={mode} generation={key} resultats={results:?}"
        ));
        if mode == 2 {
            assert!(matches!(driver_result, Outcome::Fault(_)));
            assert!(!matches!(results.0, Outcome::Exit(0)));
            assert!(!matches!(results.1, Outcome::Exit(0)))
        } else {
            assert_eq!(
                results,
                (Outcome::Exit(0), Outcome::Exit(0), Outcome::Exit(0))
            )
        }
    }
    hal::log(format_args!("DMA utilisateurs bilan OK"));
    hal::cpu::halt()
}

const MEDIA: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/storage-media.bin"));
const CONTROL: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/storage-control.bin"));
const FILE_CLIENT: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/storage-fileclient.bin"));
pub(crate) fn volume(mode: u64) -> bool {
    use crate::hardware::block;
    FILE_TIMING.store(matches!(mode, 25 | 36 | 42), Ordering::Relaxed);
    let start = heap::audit(true);
    let hz = hal::timer::counter_hz();
    if matches!(mode, 6 | 25 | 36 | 42) {
        measure("bloc_repos");
        heap::with_pages(|m| {
            m.begin_frame_sample();
            Ok(())
        })
        .unwrap();
        block::metrics::start(hz);
    }
    let rights = if matches!(mode, 4 | 6 | 8 | 9 | 11 | 14..=22 | 24 | 25 | 27 | 28 | 29 | 31..=36 | 39..=42)
    {
        1 | 2 | 4 | 8
    } else {
        1 | 8
    };
    let key = match block::begin_session(rights) {
        Ok(v) => v,
        Err(e) => {
            hal::log(format_args!("BLOC ouverture refusee {e:?}"));
            return false;
        }
    };
    let members = [
        (RELAY, 15),
        (MEDIA, 1),
        (DISK, 0),
        (
            if matches!(mode, 41 | 42) {
                include_bytes!(concat!(env!("OUT_DIR"), "/shell.elf"))
            } else if mode >= 11 {
                FILE_CLIENT
            } else {
                CONTROL
            },
            0,
        ),
    ];
    let group = if matches!(mode, 29 | 30 | 38) {
        let [r, s, d, a] = members;
        ipc::launch_block([r, s, d, a, (FILE_CLIENT, 0)], key, mode)
            .map(|[r, s, d, a, p]| ([r, s, d, a], Some(p)))
    } else {
        ipc::launch_block(members, key, mode).map(|group| (group, None))
    };
    let ([relay, service, driver, app], peer) = match group {
        Ok(v) => v,
        Err(e) => {
            let _ = block::retire();
            hal::log(format_args!("BLOC admission refusee {e}"));
            return false;
        }
    };
    hal::log(format_args!(
        "BLOC chaine novaFS={} bloc={} pilote={} client={} generation={key}",
        service.0.number, relay.0.number, driver.0.number, app.0.number
    ));
    if matches!(mode, 6 | 25 | 36 | 42) {
        measure("bloc_publie")
    }
    let ids = [relay.0, service.0, driver.0];
    let a = wait(app);
    if peer.is_none() && a != Outcome::Exit(0) {
        block::died(driver.0.number);
        ipc::close_block_channels(&ids);
    }
    let second = peer.map(wait);
    if let Some(result) = second {
        hal::log(format_args!("NOMS fin du second client {result:?}"));
    }
    let s = wait(service);
    let d = wait(driver);
    let t = crate::hal::timer::counter();
    let retired = block::retire();
    let ticks = crate::hal::timer::counter() - t;
    let b = wait(relay);
    hal::log(format_args!(
        "BLOC bilan mode={mode} service={s:?} bloc={b:?} pilote={d:?} client={a:?} retrait={retired:?} cycles_retrait={ticks}"
    ));
    if matches!(mode, 6 | 25 | 36 | 42) {
        ipc::end_measure(hz);
        block::metrics::report();
        let frames = heap::with_pages(|m| Ok(m.end_frame_sample())).unwrap();
        let after = heap::audit(true);
        measure("bloc_collecte");
        hal::log(format_args!(
            "BLOC memoire cadres_init={} cadres_fin={} allocations={} pic_cadres={} buddy_init={} buddy_fin={} hz={hz} retrait_ns={}",
            start.free_pages,
            after.free_pages,
            frames.allocations,
            start.free_pages - frames.minimum_free,
            start.occupied,
            after.occupied,
            (u128::from(ticks) * 1_000_000_000 / u128::from(hz))
        ));
    }
    second.is_none_or(|outcome| outcome == Outcome::Exit(if mode == 38 { 9 } else { 0 }))
        && [a, s, d, b].iter().all(|x| *x == Outcome::Exit(0))
        && retired.is_ok()
}

pub(crate) fn file_perf(mode: u64) -> bool {
    if !volume(mode) {
        return false;
    }
    let warm = heap::audit(true);
    if !volume(mode) {
        return false;
    }
    let clean = heap::audit(true) == warm
        && with(|s| s.processes.slots.iter().all(Option::is_none) && s.ipc.pages() == 0);
    hal::log(format_args!(
        "STOCKAGE restitution apres deux sessions={clean}"
    ));
    clean
}
/// Exerce le même ELF que le vrai service bloc, dans son espace utilisateur privé.
pub(crate) fn elf_test() {
    fn spawn(mode: u64) -> Child {
        let mut config = [0; 16];
        config[8..].copy_from_slice(&mode.to_le_bytes());
        spawn_image(MEDIA, &config, None, None).unwrap()
    }
    user::check_elf_headers(MEDIA);
    let initial = heap::audit(true);
    // Épuisement réel pendant le chargement, puis après une première page de tas.
    heap::with_pages(|m| {
        let before = m.free_pages();
        let mut held = m.hold_for_probe(9)?;
        assert!(matches!(
            UserSpace::new(m, MEDIA, &[], None),
            Err(crate::memory::Error::Exhausted)
        ));
        assert_eq!(m.free_pages(), 9);
        held.restore_to(m, before);
        let mut image = UserSpace::new(m, MEDIA, &[], None)?;
        let saved = m.free_pages();
        let base = image.resize_heap(m, 0, 0)?;
        let mut held = m.hold_for_probe(5)?;
        assert_eq!(
            image.resize_heap(m, base, base + 5 * 4096),
            Err(crate::memory::Error::Exhausted)
        );
        assert_eq!(image.resize_heap(m, 0, 0)?, base);
        assert_eq!(m.free_pages(), 5);
        held.restore_to(m, saved);
        image.release(m);
        assert_eq!(m.free_pages(), before);
        Ok(())
    })
    .unwrap();
    let hz = hal::timer::counter_hz();
    heap::with_pages(|m| {
        m.begin_frame_sample();
        Ok(())
    })
    .unwrap();
    let load_start = hal::timer::counter();
    let image = heap::with_pages(|m| UserSpace::new(m, MEDIA, &[], None)).unwrap();
    let load = hal::timer::counter() - load_start;
    let free_start = hal::timer::counter();
    heap::with_pages(|m| {
        image.release(m);
        Ok(())
    })
    .unwrap();
    let free = hal::timer::counter() - free_start;
    TIMING.store(true, Ordering::Relaxed);
    let start = hal::timer::counter();
    let first = spawn(101);
    let created = hal::timer::counter() - start;
    assert_eq!(wait(first), Outcome::Exit(0));
    TIMING.store(false, Ordering::Relaxed);
    for mode in [102, 103, 104, 105, 108, 109, 110, 111] {
        let Outcome::Fault(f) = wait(spawn(mode)) else {
            panic!("faute utilisateur attendue")
        };
        assert_eq!(
            f.cause,
            if matches!(mode, 108 | 109 | 111) {
                "protection"
            } else {
                "absente"
            }
        );
        match mode {
            102 => assert_eq!(f.address, 1 << 46),
            103 => assert!(
                (user::ELF_TOP - 262144 - 4096..user::ELF_TOP - 262144).contains(&f.address)
            ),
            108 | 111 => assert_eq!(f.access, "ecriture"),
            109 => assert_eq!(f.access, "execution"),
            110 => assert_eq!(f.address, user::ELF_TOP),
            _ => assert_eq!(f.address, 0x20000),
        }
        assert_eq!(heap::audit(true), initial);
    }
    let a = spawn(106);
    let b = spawn(107);
    measure("elf_deux_processus");
    assert_eq!(wait(a), Outcome::Exit(0));
    assert_eq!(wait(b), Outcome::Exit(0));
    let stats = heap::with_pages(|m| Ok(m.end_frame_sample())).unwrap();
    assert_eq!(heap::audit(true), initial);
    hal::log(format_args!(
        "ELF restitution OK libres={} allocations={} pic_cadres={} creation_cycles={created} chargement_cycles={load} collecte_cycles={free} hz={hz}",
        initial.free_pages,
        stats.allocations,
        initial.free_pages - stats.minimum_free
    ));
}

use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
static TIMING: AtomicBool = AtomicBool::new(false);
static BEGIN: AtomicU64 = AtomicU64::new(0);
/// Marqueurs acceptés uniquement pendant la sonde mono-client ELF.
static FILE_TIMING: AtomicBool = AtomicBool::new(false);
static USER_SAMPLE: AtomicU64 = AtomicU64::new(0);
static BLOCK_SAMPLE: AtomicU64 = AtomicU64::new(0);
pub(super) fn time_marker(msg: &str) -> bool {
    if FILE_TIMING.load(Ordering::Relaxed) {
        let timer = if msg.starts_with("ech ") {
            Some(&USER_SAMPLE)
        } else if msg.starts_with("bar ") {
            Some(&BLOCK_SAMPLE)
        } else {
            None
        };
        if let Some(timer) = timer {
            let now = hal::timer::counter();
            let then = timer.swap(now, Ordering::Relaxed);
            if !msg.ends_with(" debut") {
                hal::log(format_args!(
                    "ECHANTILLON {msg} cycles={}",
                    now.wrapping_sub(then)
                ));
            }
            if msg == "bar f1" {
                measure("transaction");
                let (data, stack) = with(|s| {
                    s.processes
                        .get(s.thread(s.current).owner.unwrap())
                        .space
                        .as_ref()
                        .unwrap()
                        .page_counts()
                });
                hal::log(format_args!(
                    "ECHANTILLON service donnees={data} pile={stack}"
                ));
            }
            // Le temps de journalisation de cette frontière est exclu du prochain intervalle.
            timer.store(hal::timer::counter(), Ordering::Relaxed);
            return true;
        }
    }
    if !TIMING.load(Ordering::Relaxed) {
        return false;
    }
    if msg == "chrono debut" {
        BEGIN.store(hal::timer::counter(), Ordering::Relaxed);
        return true;
    }
    if matches!(
        msg,
        "alloc_free_4096" | "alloc_512" | "free_512" | "extension_2" | "retrait_2" | "vide"
    ) {
        let elapsed = hal::timer::counter() - BEGIN.load(Ordering::Relaxed);
        hal::log(format_args!("ELF mesure {msg} cycles={elapsed}"));
        return true;
    }
    false
}
