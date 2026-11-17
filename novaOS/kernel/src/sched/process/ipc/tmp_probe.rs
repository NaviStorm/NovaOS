//! Conduit des processus invités ordinaires sur une image de qualification.
use super::*;

/// Produit uniquement par le scénario tmp-contention, après copie des paramètres de boot.
pub(super) struct TicketBoot(());

fn await_condition(mut predicate: impl FnMut() -> bool) {
    let end = ticks() + 120 * u64::from(HZ);
    while !predicate() {
        assert!(ticks() < end, "délai dépassé dans la sonde tmp");
        sleep(1);
    }
}

fn launch(mode: u64) -> Child {
    let app = spawn::tmp_sample();
    let admission = app.admission(0, false).unwrap();
    app.principal(&mode.to_le_bytes(), admission).unwrap()
}

fn word(child: &Child, index: usize, set: Option<u64>) -> u64 {
    with(|s| {
        let p = s.processes.get(child.0);
        assert!(p.outcome.is_none(), "processus tmp terminé prématurément pid={} résultat={:?}", p.pid.number, p.outcome);
        let space = p.space.as_ref().unwrap();
        heap::with_pages(|m| {
            if let Some(value) = set {
                space.write_data(m, index, value);
            }
            Ok(space.read_data(m, index))
        }).unwrap()
    })
}

fn ready(mode: u64) -> Child {
    let child = launch(mode);
    await_condition(|| word(&child, 4, None) == 1);
    child
}

pub(in crate::sched::process) fn run(scenario: &[u8], client: &Child) {
    await_condition(super::super::tmp::ready_shell);
    let boot = crate::boot_id::current().expect("époque qualifiée");
    hal::log(format_args!("TMP démarrage {:032x}", u128::from_be_bytes(boot)));
    if scenario == b"tmp-contention" {
        tickets::ticket_race(&TicketBoot(()), client.0);
        tickets::contention(client);
        return;
    }
    if scenario == b"tmp-reprise" {
        let new = ready(4);
        let key = with(|s| s.processes.get(new.0).instance.identity(Some(boot)).unwrap());
        hal::log(format_args!("TMP nouvelle instance={} génération={}", key.instance, key.generation));
        word(&new, 5, Some(1));
        assert_eq!(new.join(), Outcome::Exit(0));
        hal::log(format_args!("TMP reprise vérifiée"));
        return;
    }
    // Les deux instances coexistent avec le même manifeste ; leurs noms "commun" se recouvrent.
    let first = ready(1);
    let first_key = with(|s| s.processes.get(first.0).instance.identity(Some(boot)).unwrap());
    hal::log(format_args!("TMP première instance={} génération={}", first_key.instance, first_key.generation));
    if scenario == b"tmp-coupure" {
        hal::log(format_args!("TMP coupure prête fichier synchronisé handle ouvert"));
        sleep(120 * u64::from(HZ));
        panic!("coupure du banc absente");
    }
    let second = ready(2);
    let other_cap = word(&second, 1, None);
    assert_ne!(other_cap, word(&first, 1, None));
    word(&first, 3, Some(other_cap));
    word(&first, 6, Some(word(&second, 2, None)));
    word(&first, 5, Some(1));
    await_condition(|| word(&first, 4, None) == 2);
    with(|s| stop_process(s, second.0, Outcome::Exit(125), hal::timer::counter()));
    assert_eq!(second.join(), Outcome::Exit(125));
    word(&first, 5, Some(2));
    assert_eq!(first.join(), Outcome::Exit(0));
    let fault = ready(3);
    word(&fault, 5, Some(1));
    assert!(matches!(fault.join(), Outcome::Fault(_)));
    hal::log(format_args!("TMP isolation, thread, enfants, arrêt forcé et faute vérifiés"));
}

mod tickets;
