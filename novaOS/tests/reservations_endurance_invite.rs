//! Banc de durée de vie sur trois acteurs réels. Un disque fictif est réservé au profil.
use super::*;
use crate::hardware::block;

const TAILLE: u64 = 256;
const TOURS: usize = 125;
const SERIES: usize = 8;
const LOT: u64 = 0x1002e;
static RESTITUTIONS: AtomicU64 = AtomicU64::new(0);
static MAX_CYCLES: AtomicU64 = AtomicU64::new(0);

pub(in crate::sched::process::ipc) fn restitution(cycles: u64) {
    MAX_CYCLES.fetch_max(cycles, Ordering::Relaxed);
    RESTITUTIONS.fetch_add(1, Ordering::Relaxed);
}

fn mots(acteur: Actor, table: &[u64]) {
    let mut octets = [0; 64];
    for (dst, mot) in octets.chunks_exact_mut(8).zip(table) {
        dst.copy_from_slice(&mot.to_le_bytes());
    }
    acteur.put(DESC, &octets[..8 * table.len()]);
}

fn photo(etape: &str) -> [u64; 8] {
    let (epoch, quarantine) = block::observation_bail();
    let mem = heap::audit(false);
    let pins = heap::with_pages(|m| Ok(m.observation_dma())).unwrap();
    let (objets, handles, lots, pages, attente, generation) = with(|s| {
        let droits = s
            .ipc
            .locals
            .ids()
            .map(|i| {
                s.ipc
                    .locals
                    .get(i)
                    .entries
                    .iter()
                    .filter(|e| e.key != 0)
                    .count()
            })
            .sum::<usize>();
        let (lots, pages) = s.ipc.observation_lots();
        (
            s.ipc.ends.ids().count(),
            droits,
            lots,
            pages,
            s.ipc.waits.ids().count(),
            s.ipc.incarnation,
        )
    });
    hal::log(format_args!(
        "LOTS0170 etape={etape} libres={} objets={objets} handles={handles} lots={lots} pages_logiques={pages} attente={attente} epinglees={pins} bail={epoch} quarantaine={quarantine} generation={generation}",
        mem.free_pages
    ));
    [
        mem.free_pages,
        objets as u64,
        handles as u64,
        lots as u64,
        pages as u64,
        attente as u64,
        pins,
        u64::from(quarantine),
    ]
}

fn ouvrir(pilote: Actor, moteur: Actor, envoi: u64, reponse: u64) -> (u64, u64) {
    let root = pilote.call(IPC_CREATE, 0, 0, 0).unwrap();
    let cap = with(|s| {
        let cap = s
            .ipc
            .derive(pilote.0, root, moteur.0, (SEND | RECEIVE).into())?;
        s.ipc.bind_lane(pilote.0, root, moteur.0, cap)?;
        s.ipc
            .lier_attentes_lot(pilote.0, root, moteur.0, envoi, reponse)?;
        Ok::<_, u64>(cap)
    })
    .unwrap();
    (root, cap)
}

fn reserver(moteur: Actor, cap: u64, base: u64, tag: u64, pages: u64) -> Res<u64> {
    mots(moteur, &[tag, base, pages, 0, base + 4096]);
    moteur.call(LOT, cap, DESC, 40)
}

// La requête de lecture légitime ne vise que la première page du disque privé.
// La soumission se produit exclusivement sur un nouveau lot, précédemment admis.
fn succes(pilote: Actor, moteur: Actor, envoi: u64, reponse: u64, base: u64, epoch: u64, tag: u64) {
    let (root, cap) = ouvrir(pilote, moteur, envoi, reponse);
    assert_eq!(reserver(moteur, cap, base, tag, 1), Ok(0));
    mots(pilote, &[tag, root, epoch, 0, 0, 1, 1, 0]);
    assert_eq!(pilote.call(0x1003a, DESC, 64, 0), Ok(1));
    mots(pilote, &[tag, base, 1, 0]);
    assert_eq!(pilote.call(0x10032, root, DESC, 32), Ok(0));
    assert_eq!(moteur.bytes::<16>(base), *b"NovaOS-test-DMA!");
    assert_eq!(moteur.call(IPC_CLOSE, cap, 0, 0), Ok(0));
    assert_eq!(pilote.call(IPC_CLOSE, root, 0, 0), Ok(0));
}

fn migrer(a: Actor, cpu: usize) {
    with(|s| {
        s.slots
            .iter_mut()
            .flatten()
            .find(|t| t.owner == Some(a.0))
            .unwrap()
            .cpu_mask = 1 << cpu
    });
    until(|| {
        with(|s| {
            s.slots
                .iter()
                .flatten()
                .find(|t| t.owner == Some(a.0))
                .unwrap()
                .last_cpu
                == cpu
        })
    });
}

fn envoyer_lot(moteur: Actor, canal: u64, tag: u64, attente: u64) {
    let mut message = send_wire(tag, 128, attente);
    let mut lot = block::wire::blank(block::wire::READ);
    lot[2] = 4;
    block::wire::put(&mut lot, 4, TAILLE as u32);
    message[32..].copy_from_slice(&lot);
    moteur.put(DESC, &message);
    moteur.start(IPC_SEND, canal, DESC, 160);
}

fn fin_attente(
    moteur: Actor,
    canal: u64,
    tag: u64,
    envoi: bool,
    annuler: Option<(Actor, u64, Actor, u64)>,
) {
    races::clock(Some(1000));
    if envoi {
        envoyer_lot(moteur, canal, tag, 500);
    } else {
        moteur.recv(canal, 500, false);
    }
    moteur.blocked();
    if let Some((a, h, b, k)) = annuler {
        a.start(IPC_CANCEL, h, 0, 0);
        b.start(IPC_CANCEL, k, 0, 0);
        assert_eq!(a.result().unwrap() + b.result().unwrap(), 1);
        assert_eq!(moteur.result(), Err(CANCELLED));
    } else {
        races::clock(Some(1500));
        assert_eq!(moteur.result(), Err(TIMED_OUT));
    }
    races::clock(None);
}

pub(super) fn run() {
    let initial = photo("avant");
    let mut delai_max = 0;
    let mut preemptions = 0;
    let mut dernier_pid = 0;
    let mut derniere_epoque = 0;
    for serie in 0..SERIES {
        let (enfants, [pilote, moteur, relais]) = group([ALL, ALL, ALL]);
        let epoch = block::begin_session(1).unwrap();
        assert!(epoch > derniere_epoque && moteur.0.number > dernier_pid);
        derniere_epoque = epoch;
        dernier_pid = moteur.0.number;
        assert!(block::attach(pilote.0.number, epoch));
        let base = moteur.memory(|s, m| {
            let p = s.resize_heap(m, 0, 0).unwrap();
            s.resize_heap(m, p, p + TAILLE * 4096).unwrap();
            p
        });
        moteur.put(base + 4096, &[0; TAILLE as usize * 8]);
        let requetes = relais.call(IPC_CREATE, 0, 0, 0).unwrap();
        let reponses = relais.call(IPC_CREATE, 0, 0, 0).unwrap();
        let (tx, rx, annule) = with(|s| {
            let tx = s
                .ipc
                .derive(relais.0, requetes, moteur.0, SEND.into())
                .unwrap();
            let annule = s
                .ipc
                .derive(relais.0, reponses, pilote.0, ALL.into())
                .unwrap();
            let rx = s
                .ipc
                .derive(pilote.0, annule, moteur.0, RECEIVE.into())
                .unwrap();
            (tx, rx, annule)
        });
        if crate::smp::count() > 1 {
            migrer(moteur, 0);
        }
        succes(pilote, moteur, tx, rx, base, epoch, 1);
        let chaud = photo("service_chaud");
        let mut dernier_handle = 0;
        for i in 0..TOURS {
            let tag = 2 + i as u64;
            let (root, cap) = ouvrir(pilote, moteur, tx, rx);
            assert_ne!(cap, dernier_handle);
            dernier_handle = cap;
            assert_eq!(reserver(moteur, cap, base, tag, TAILLE), Ok(0));
            assert_eq!(reserver(moteur, cap, base, tag + 1, TAILLE), Err(DENIED));
            assert_eq!(with(|s| s.ipc.observation_lots()), (1, TAILLE as usize));
            let kicks = block::kicks();

            if i % 3 == 0 {
                // Saturer l'endpoint de publication de huit messages ordinaires.
                for j in 0..8 {
                    moteur.send(tx, j + 1, 0, 0);
                    assert_eq!(moteur.result(), Ok(0));
                }
                fin_attente(moteur, tx, tag, true, None);
                for _ in 0..8 {
                    relais.recv(requetes, 0, false);
                    assert_eq!(relais.result(), Ok(1));
                }
            } else {
                envoyer_lot(moteur, tx, tag, 0);
                assert_eq!(moteur.result(), Ok(0));
                relais.recv(requetes, 0, false);
                assert_eq!(relais.result(), Ok(1));
                let annulation = (i % 3 == 2).then_some((relais, reponses, pilote, annule));
                fin_attente(moteur, rx, tag, false, annulation);
            }
            assert_eq!(with(|s| s.ipc.observation_lots()), (0, 0));
            let debut = hal::timer::counter();
            // Borne haute du retour IPC_CLOSE, y compris l’ordonnancement des acteurs.
            assert_eq!(moteur.call(IPC_CLOSE, cap, 0, 0), Ok(0));
            delai_max = delai_max.max(hal::timer::counter() - debut);
            assert_eq!(with(|s| s.ipc.observation_lots()), (0, 0));
            assert_eq!(block::kicks(), kicks);
            assert!(!block::inflight());
            assert_eq!(pilote.call(IPC_CLOSE, root, 0, 0), Ok(0));
            if i == TOURS / 2 && crate::smp::count() > 1 {
                migrer(moteur, 1);
            }
            if i % 8 == 0 {
                succes(pilote, moteur, tx, rx, base, epoch, tag + 1000);
            }
        }
        assert_eq!(photo("service_apres"), chaud);
        // Chaque série s'achève par une disparition réelle de l'un des deux
        // propriétaires alors qu'un dernier lot logiciel demeure réservé.
        let (_root, cap) = ouvrir(pilote, moteur, tx, rx);
        assert_eq!(reserver(moteur, cap, base, 5000, TAILLE), Ok(0));
        let [p, m, r] = enfants;
        preemptions += with(|s| s.processes.get(moteur.0).preemptions);
        if serie % 2 == 0 {
            moteur.start(crate::abi::PROCESS_EXIT, 0, 0, 0);
            assert_eq!(m.join(), Outcome::Exit(0));
            assert_eq!(with(|s| s.ipc.observation_lots()), (0, 0));
            stop([p, r], [pilote, relais]);
        } else {
            pilote.start(crate::abi::PROCESS_EXIT, 0, 0, 0);
            assert_eq!(p.join(), Outcome::Exit(0));
            assert_eq!(with(|s| s.ipc.observation_lots()), (0, 0));
            stop([m, r], [moteur, relais]);
        }
        block::retire().unwrap();
        assert_eq!(photo("fin_serie"), initial);
        hal::log(format_args!(
            "LOTS0170 serie={} abandons={TOURS} succes=17 generation={epoch}",
            serie + 1
        ));
    }
    let retraits = RESTITUTIONS.load(Ordering::Relaxed);
    let maximum = MAX_CYCLES.load(Ordering::Relaxed);
    assert_eq!(retraits, (TOURS * SERIES) as u64);
    hal::log(format_args!(
        "LOTS0170 fin abandons={} succes={} series={SERIES} preemptions={preemptions} restitutions={retraits} transition_max_cycles={maximum} retrait_max_cycles={delai_max} hz={} CPU={} OK",
        SERIES * TOURS,
        SERIES * 17,
        hal::timer::counter_hz(),
        crate::smp::count()
    ));
}
