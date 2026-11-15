//! Le noyau joue le rôle d’écrivain témoin pour choisir chaque point de remise.
//! Le client et le récepteur des notifications exécutent le véritable ABI invité.
use super::*;

const BUFFER: u64 = 0x401000 + 256;

fn await_state(mut ready: impl FnMut(&Core) -> bool) {
    let deadline = ticks() + 10000;
    while !with(|s| ready(s)) {
        assert!(ticks() < deadline, "délai du banc d’attente tmp");
        sleep(1);
    }
}

fn word(s: &Core, pid: Pid, index: usize, set: Option<u64>) -> u64 {
    let p = s.processes.get(pid);
    assert!(p.outcome.is_none(), "acteur tmp sorti {:?}", p.outcome);
    let space = p.space.as_ref().unwrap();
    heap::with_pages(|m| {
        if let Some(value) = set {
            space.write_data(m, index, value);
        }
        Ok(space.read_data(m, index))
    })
    .unwrap()
}

fn waiting(s: &Core, client: Pid) -> bool {
    let Some(request) = s
        .processes
        .temporary
        .requests
        .iter()
        .find(|r| r.owner == client)
    else {
        return false;
    };
    s.slots.iter().flatten().any(|t| t.owner == Some(client)
        && matches!(t.state, State::Blocked(Wait::Temporary { token, .. }) if token == request.ticket))
}

fn take(s: &mut Core, server: Pid, rx: u64, operation: u64) -> u64 {
    let ticket = next(s, server, rx, BUFFER).unwrap();
    assert_eq!(s.processes.temporary.flight.unwrap().operation, operation);
    ticket
}

pub(crate) fn run() {
    let image = include_bytes!(concat!(env!("OUT_DIR"), "/tmp_sample.bin"));
    let (server, client, cap) = {
        let _guard = Guard::new();
        let server = create_image(image, &201u64.to_le_bytes(), None, None).unwrap();
        let client = create_image(image, &200u64.to_le_bytes(), None, None).unwrap();
        let cap = with(|s| {
            s.processes.get_mut(client.0).application_threads = true;
            install(s, server.0, client.0).unwrap();
            s.processes.get(client.0).tmp_cap
        });
        (server, client, cap)
    };
    await_state(|s| word(s, server.0, 1, None) != 0);
    let rx = with(|s| {
        let rx = word(s, server.0, 1, None);
        let creation = take(s, server.0, rx, wire::CREATE);
        complete(s, server.0, creation, 0, OK).unwrap();
        assert_eq!(next(s, server.0, rx, BUFFER), Err(WOULD_BLOCK));
        word(s, server.0, 5, Some(1));
        rx
    });
    await_state(|s| {
        s.slots
            .iter()
            .flatten()
            .any(|t| t.owner == Some(server.0) && matches!(t.state, State::Blocked(Wait::Ipc(_))))
    });
    with(|s| word(s, client.0, 5, Some(1)));
    await_state(|s| waiting(s, client.0) && word(s, server.0, 4, None) == 1);
    with(|s| word(s, client.0, 5, Some(2)));
    await_state(|s| word(s, client.0, 4, None) == 1);
    with(|s| {
        assert_eq!(next(s, server.0, rx, BUFFER), Err(WOULD_BLOCK));
        word(s, client.0, 5, Some(3));
    });
    await_state(|s| waiting(s, client.0));
    let ticket = with(|s| {
        let ticket = take(s, server.0, rx, wire::IO);
        word(s, client.0, 5, Some(4));
        ticket
    });
    await_state(|s| word(s, client.0, 4, None) == 2);
    with(|s| {
        assert!(s.processes.temporary.requests.is_empty());
        complete(s, server.0, ticket, BUFFER, OK).unwrap();
        assert_eq!(complete(s, server.0, ticket, BUFFER, OK), Err(REVOKED));
        word(s, client.0, 5, Some(5));
    });
    await_state(|s| waiting(s, client.0));
    with(|s| {
        let t = take(s, server.0, rx, wire::IO);
        stop_process(s, client.0, Outcome::Exit(125), hal::timer::counter());
        assert_eq!(authority(s, client.0, cap), Err(DENIED));
        assert!(s.processes.temporary.requests.is_empty());
        complete(s, server.0, t, BUFFER, OK).unwrap();
        let remove = take(s, server.0, rx, wire::REMOVE);
        complete(s, server.0, remove, 0, OK).unwrap();
        assert!(s.processes.temporary.leases.is_empty());
        word(s, server.0, 5, Some(2));
    });
    assert_eq!(client.join(), Outcome::Exit(125));
    assert_eq!(server.join(), Outcome::Exit(0));
    hal::log(format_args!(
        "TMP attentes reveil=OK avant=CANCELLED apres=UNCERTAIN retrait=DENIED tickets=0"
    ));
    // Le dernier serveur teste aussi le ticket remis par l'incarnation précédente.
    let mut ancien = 0;
    for phase in [5u64, 4, 3, 2, 1] {
        ancien = fin_service(image, phase, ancien);
    }
}

/// L'admission, NEXT et l'effet sont séparés par des rendez-vous. Pour les deux
/// résultats déjà acceptés, COMPLETE et le retrait se font sous la même garde
/// afin que le client ne puisse consommer la réponse entre les deux.
fn fin_service(image: &'static [u8], phase: u64, ancien: u64) -> u64 {
    let (serveur, client) = {
        let _guard = Guard::new();
        let serveur = create_image(image, &201u64.to_le_bytes(), None, None).unwrap();
        let client = create_image(image, &200u64.to_le_bytes(), None, None).unwrap();
        with(|s| {
            word(s, serveur.0, 2, Some(phase));
            word(s, serveur.0, 3, Some(ancien));
            word(s, client.0, 2, Some(phase));
            s.processes.get_mut(client.0).application_threads = true;
            install(s, serveur.0, client.0).unwrap();
        });
        (serveur, client)
    };
    await_state(|s| word(s, serveur.0, 4, None) == 1);
    with(|s| word(s, client.0, 5, Some(1)));
    await_state(|s| waiting(s, client.0));
    with(|s| word(s, client.0, 5, Some(2)));
    await_state(|s| {
        let b = &s.processes.temporary;
        b.requests.len() == 2 && b.requests.iter().all(|r| {
            r.owner == client.0 && s.slots.iter().flatten().any(|t| t.owner == Some(client.0)
                && matches!(t.state, State::Blocked(Wait::Temporary { token, .. }) if token == r.ticket))
        })
    });
    let (ticket, mut effets) = with(|s| {
        let b = &s.processes.temporary;
        assert!(b.flight.is_none());
        assert_eq!(b.requests[0].frame[files::PAYLOAD], 1);
        assert_eq!(b.requests[1].frame[files::PAYLOAD], 2);
        let ticket = b.requests[0].ticket;
        assert!(ticket > ancien);
        let compteur = word(s, serveur.0, 6, None);
        assert_eq!(compteur, 0);
        assert_eq!(word(s, serveur.0, 9, None), 0);
        word(s, serveur.0, 5, Some(1));
        (ticket, compteur)
    });
    if phase != 1 {
        await_state(|s| word(s, serveur.0, 4, None) == 2);
        effets = with(|s| {
            assert_eq!(word(s, serveur.0, 3, None), ticket);
            let b = &s.processes.temporary;
            assert_eq!(b.flight.unwrap().ticket, ticket);
            assert!(b.requests.iter().all(|r| r.done.is_none()));
            let effets = word(s, serveur.0, 6, None);
            assert_eq!(effets, u64::from(matches!(phase, 3 | 4)));
            assert_eq!(word(s, serveur.0, 9, None), effets);
            if phase >= 4 {
                let (adresse, statut) = if phase == 4 {
                    let mut reponse = files::frame(files::WRITE, 1, 1);
                    reponse[files::PAYLOAD] = 0xa7;
                    copy_out(s, serveur.0, BUFFER, &reponse).unwrap();
                    (BUFFER, OK)
                } else {
                    (0, files::FULL)
                };
                complete(s, serveur.0, ticket, adresse, statut).unwrap();
                assert!(s.processes.temporary.flight.is_none());
                assert_eq!(s.processes.temporary.requests[0].done, Some(statut));
                stop_process(s, serveur.0, Outcome::Exit(0), hal::timer::counter());
            } else {
                word(s, serveur.0, 5, Some(2));
            }
            effets
        });
    }
    assert_eq!(serveur.join(), Outcome::Exit(0));
    await_state(|s| word(s, client.0, 4, None) == 1);
    let (premier, second) = with(|s| {
        let b = &s.processes.temporary;
        assert!(b.server.is_none() && b.flight.is_none());
        assert!(b.requests.is_empty() && b.leases.is_empty());
        let resultats = (word(s, client.0, 6, None), word(s, client.0, 7, None));
        word(s, client.0, 5, Some(3));
        resultats
    });
    assert_eq!(client.join(), Outcome::Exit(0));
    assert_eq!(
        premier,
        match phase {
            1 => CLOSED,
            2 | 3 => files::UNCERTAIN,
            4 => OK,
            5 => files::FULL,
            _ => unreachable!(),
        }
    );
    assert_eq!(second, CLOSED);
    hal::log(format_args!(
        "TMP fin-service phase={phase} appel={premier} autre={second} effets={effets} ancien={ancien} ticket={ticket} nouveau=DENIED file=0"
    ));
    ticket
}
