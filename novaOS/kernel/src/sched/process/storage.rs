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

fn supervise(first: &[u8], lifetime: Lifetime) -> bool {
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
