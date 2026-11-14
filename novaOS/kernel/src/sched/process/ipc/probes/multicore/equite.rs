//! Charge fonctionnelle soutenue. Les acteurs avancent pendant les observations
//! espacées du contrôleur et pendant la révocation de deux attentes simultanées.
use super::*;
use crate::sched::equite::Fil;

const PAIRES: usize = 4;
const PERIODES: usize = 3;
const QUANTA: u64 = HZ as u64; // une seconde d'horloge d'ordonnancement par période
const TRAVAIL: u64 = u64::MAX - 5;

struct Actif {
    acteur: Actor,
    id: Id,
    role: &'static str,
    cumul: u64,
    minimum: u64,
}

fn observer(a: Actor, role: &'static str) -> Actif {
    let id = with(|s| {
        let t = s
            .slots
            .iter()
            .flatten()
            .find(|t| t.owner == Some(a.0))
            .unwrap();
        t.id
    });
    Actif {
        acteur: a,
        id,
        role,
        cumul: 0,
        minimum: u64::MAX,
    }
}

fn progres(f: &Actif) -> u64 {
    f.acteur.memory(|s, m| s.read_data(m, 11))
}

fn signal(a: Actor) {
    a.memory(|s, m| s.write_data(m, 10, 1));
}

fn rapport(fil: &Actif, mesures: Fil, hz: u64) {
    let ns = |t| u128::from(t) * 1_000_000_000 / u128::from(hz);
    hal::log(format_args!(
        "EQUITE fil={} generation={} role={} departs={} reveils={} preemptions={} cpus={:#x} entre_ns={} bloque_ns={} pret_ns={} alertes={} progres={} minimum={}",
        fil.id.slot,
        fil.id.generation,
        fil.role,
        mesures.departs,
        mesures.reveils,
        mesures.preemptions,
        mesures.cpus,
        ns(mesures.entre_max),
        ns(mesures.bloque_max),
        ns(mesures.pret_max),
        mesures.lents,
        fil.cumul,
        fil.minimum,
    ));
}

pub(super) fn run() -> ! {
    let cpu = smp::count();
    let hz = timer::counter_hz();
    assert!(matches!(cpu, 1 | 2 | 4));
    QUIET.store(true, Ordering::Relaxed);
    let initial = heap::audit(false);
    {
        let mut couples = Vec::with_capacity(PAIRES);
        let mut fils = Vec::with_capacity(2 * PAIRES + 2);
        let tous = (1 << cpu) - 1;
        for i in 0..PAIRES {
            // Deux couples peuvent migrer. Les autres exercent des affinités fixes.
            let masque = if i % 2 == 0 { tous } else { 1 << (i % cpu) };
            let couple = Couple::preparer([masque; 2]);
            for (acteur, role) in couple.acteurs.into_iter().zip(["client", "serveur"]) {
                fils.push(observer(acteur, role));
            }
            couples.push(couple);
        }
        let (aux, [calcul, plein]) = group([ALL, ALL]);
        let canal = plein.call(IPC_CREATE, 0, 0, 0).unwrap();
        for tag in 0..8 {
            plein.send(canal, tag, 128, 0);
            assert_eq!(plein.result(), Ok(128));
        }
        perf::word(plein, 6, canal);
        fils.push(observer(calcul, "calcul"));
        fils.push(observer(plein, "file_pleine"));
        let ([proprietaire, receveur1, receveur2], [a, b, c]) = group([ALL, RECEIVE, RECEIVE]);
        let (cle, bcle, ccle) = (a.boot(), b.boot(), c.boot());
        b.recv(bcle, 15000, false);
        c.recv(ccle, 15000, false);
        b.blocked();
        c.blocked();
        let mut responsable = observer(a, "proprietaire");
        let mut b_obs = observer(b, "revoque");
        let mut c_obs = observer(c, "revoque");
        let preemptions = with(|s| s.stats.preemptions);
        hal::log(format_args!(
            "EQUITE debut cpu={cpu} hz={hz} couples={PAIRES} periodes={PERIODES} quanta={QUANTA} seuil_ns=1000000000"
        ));
        {
            let _g = Guard::new();
            with(|s| {
                s.equite.commencer(hz);
                for suivi in fils.iter().chain([&responsable, &b_obs, &c_obs]) {
                    let etat = s.thread(suivi.id.slot).state;
                    s.equite.inscrire(suivi.id, etat);
                }
            });
            for couple in &couples {
                let [client, serveur] = couple.acteurs;
                serveur.start(TRAVAIL, 1, 0, 0);
                client.start(TRAVAIL, 0, 0, 0);
            }
            plein.start(TRAVAIL, 2, 0, 0);
            calcul.start(TRAVAIL, 3, 0, 0);
        }
        // La garde publie les deux retraits dans la même transaction IPC.
        sleep(5);
        assert_eq!(a.call(IPC_REVOKE, cle, 0, 0), Ok(2));
        assert_eq!(b.result(), Err(REVOKED));
        assert_eq!(c.result(), Err(REVOKED));
        b_obs.cumul = 1;
        c_obs.cumul = 1;
        b_obs.minimum = 1;
        c_obs.minimum = 1;
        responsable.cumul = 2;
        responsable.minimum = 2;
        let (ma, mb, mc) = with(|s| {
            (
                s.equite.lire(responsable.id),
                s.equite.lire(b_obs.id),
                s.equite.lire(c_obs.id),
            )
        });
        stop([receveur1, receveur2, proprietaire], [b, c, a]);
        for f in &mut fils {
            f.cumul = progres(f);
        }
        let mut pre = 0;
        let mut deltas = [[0u64; PAIRES * 2 + 2]; PERIODES];
        for periode in 0..PERIODES {
            if cpu > 1 && periode < 2 {
                // Déplacer deux acteurs vivants en excluant leur PE précédent.
                // La sélection normale applique le masque au dispatch suivant.
                let cible = 1 << periode;
                with(|s| {
                    for fil in &fils[..2] {
                        s.thread_mut(fil.id.slot).cpu_mask = cible;
                    }
                });
            }
            sleep(QUANTA);
            for (i, f) in fils.iter_mut().enumerate() {
                let total = progres(f);
                let delta = total.checked_sub(f.cumul).unwrap();
                assert!(
                    delta > 0,
                    "aucun progrès de {} en période {periode}",
                    f.role
                );
                f.cumul = total;
                f.minimum = f.minimum.min(delta);
                deltas[periode][i] = delta;
            }
            pre = with(|s| s.stats.preemptions) - preemptions;
            hal::log(format_args!(
                "EQUITE periode={periode} vivants={} preemptions={pre}",
                fils.len()
            ));
        }
        with(|s| s.equite.active = false);
        for (periode, fenetre) in deltas.iter().enumerate() {
            for (f, delta) in fils.iter().zip(fenetre) {
                hal::log(format_args!(
                    "EQUITE intervalle={periode} generation={} delta={delta}",
                    f.id.generation
                ));
            }
        }
        for f in &fils {
            let mesures = with(|s| s.equite.lire(f.id));
            assert!(mesures.departs > 0);
            rapport(f, mesures, hz);
        }
        rapport(&responsable, ma, hz);
        rapport(&b_obs, mb, hz);
        rapport(&c_obs, mc, hz);
        assert!(mb.reveils > 0 && mc.reveils > 0 && pre > 0);
        if cpu > 1 {
            for f in &fils[..2] {
                assert!(
                    with(|s| s.equite.lire(f.id).cpus) & 3 == 3,
                    "migration absente"
                );
            }
        }
        signal(calcul);
        signal(plein);
        assert!(calcul.result().unwrap() >= fils[2 * PAIRES].cumul);
        assert!(plein.result().unwrap() >= fils[2 * PAIRES + 1].cumul);
        for (i, couple) in couples.into_iter().enumerate() {
            let [client, serveur] = couple.acteurs;
            signal(client);
            let total = client.result().unwrap();
            assert_eq!(serveur.result(), Ok(total));
            assert!(total >= fils[2 * i].cumul && total >= fils[2 * i + 1].cumul);
            hal::log(format_args!("EQUITE couple={i} tours={total}"));
            stop(couple.enfants, couple.acteurs);
        }
        for tag in 0..8 {
            plein.recv(canal, 0, false);
            assert_eq!(plein.result(), Ok(1));
            contract::check_data(plein, tag, 128);
        }
        stop(aux, [calcul, plein]);
    }
    assert_eq!(with(|s| s.ipc.pages()), 0);
    assert_eq!(heap::audit(false), initial);
    hal::log(format_args!(
        "EQUITE validation OK cadres={} pages_ipc=0",
        initial.free_pages
    ));
    hal::cpu::halt()
}
