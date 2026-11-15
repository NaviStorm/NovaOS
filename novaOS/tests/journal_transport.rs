//! Injecte seulement les résultats des primitives IPC. Le client testé est celui de l'ELF.
#[allow(dead_code)]
#[path = "../abi/native.rs"]
mod abi;
#[allow(dead_code)]
#[path = "../user/services/journal_backend.rs"]
mod backend;
#[allow(dead_code)]
#[path = "../abi/files.rs"]
mod files;
#[allow(dead_code)]
#[path = "../user/services/journal_storage_wire.rs"]
mod wire;
mod runtime {
    pub use super::abi::{LIMIT, OK, REFUSED, REVOKED, TIMED_OUT};
    use std::{cell::RefCell, collections::VecDeque};
    pub type Result<T> = std::result::Result<T, u64>;
    pub struct Msg {
        pub tag: u64,
        pub kind: u64,
        pub len: u64,
        pub payload: [u8; 128],
    }
    #[derive(Default)]
    struct End {
        sent: usize,
        received: usize,
        delivery: Option<u64>,
        replies: VecDeque<Result<Msg>>,
    }
    thread_local! { static END:RefCell<End> = RefCell::new(End::default()); }
    pub fn fixture(delivery: Option<u64>, replies: Vec<Result<Msg>>) {
        END.with(|s| {
            *s.borrow_mut() = End {
                delivery,
                replies: replies.into(),
                ..Default::default()
            }
        });
    }
    pub fn counts() -> (usize, usize) {
        END.with(|s| (s.borrow().sent, s.borrow().received))
    }
    pub fn send_for(port: u64, _: u64, data: &[u8], timeout: u64) -> Result<()> {
        assert_eq!((port, data.len(), timeout), (101, 128, 15000));
        END.with(|s| {
            let mut e = s.borrow_mut();
            e.sent += 1;
            e.delivery.map_or(Ok(()), Err)
        })
    }
    pub fn receive_for(port: u64, timeout: u64) -> Result<Msg> {
        assert_eq!((port, timeout), (102, 15000));
        END.with(|s| {
            let mut e = s.borrow_mut();
            e.received += 1;
            e.replies.pop_front().expect("réception superflue")
        })
    }
}
fn reply(op: u64, tag: u64, status: u64, value: u64) -> runtime::Msg {
    runtime::Msg {
        tag,
        kind: 1,
        len: 128,
        payload: wire::header(op, status, value),
    }
}
#[test]
fn absence_de_remise() {
    runtime::fixture(Some(runtime::REVOKED), vec![]);
    let mut cli = backend::Client::new(101, 102);
    assert_eq!(cli.append(1, &[0; 96]), Err(runtime::REVOKED));
    assert_eq!(runtime::counts(), (1, 0));
}
#[test]
fn perte_apres_remise() {
    for error in [runtime::TIMED_OUT, runtime::REVOKED] {
        runtime::fixture(None, vec![Err(error)]);
        let mut cli = backend::Client::new(101, 102);
        assert_eq!(cli.append(1, &[0; 96]), Err(files::UNCERTAIN));
        assert_eq!(runtime::counts(), (1, 1));
        runtime::fixture(None, vec![Err(error)]);
        assert_eq!(cli.rotate(4), Err(files::UNCERTAIN));
        assert_eq!(runtime::counts(), (1, 1));
        runtime::fixture(None, vec![Err(error)]);
        assert_eq!(cli.status(), Err(error));
        assert_eq!(runtime::counts(), (1, 1));
    }
}
#[test]
fn acquittement_tardif_et_numero_de_sequence() {
    runtime::fixture(None, vec![Err(runtime::TIMED_OUT)]);
    let mut cli = backend::Client::new(101, 102);
    assert_eq!(cli.append(5, &[0; 96]), Err(files::UNCERTAIN));
    let mut later = reply(wire::STATUS, 2, runtime::OK, 0);
    wire::put(&mut later.payload, 4, 1);
    wire::put(&mut later.payload, 5, 6);
    runtime::fixture(
        None,
        vec![Ok(reply(wire::APPEND, 1, runtime::OK, 5)), Ok(later)],
    );
    assert_eq!(cli.status().unwrap().next, 6);
    assert_eq!(runtime::counts(), (1, 2));
    runtime::fixture(None, vec![Ok(reply(wire::APPEND, 3, runtime::OK, 8))]);
    assert_eq!(cli.append(6, &[0; 96]), Err(files::UNCERTAIN));
    assert_eq!(runtime::counts(), (1, 1));
}
#[test]
fn acquittement_authentifie_ou_echec_explicite() {
    for (status, result) in [
        (runtime::OK, Ok(6)),
        (files::FULL, Err(files::FULL)),
        (files::UNCERTAIN, Err(files::UNCERTAIN)),
    ] {
        let mut cli = backend::Client::new(101, 102);
        runtime::fixture(None, vec![Ok(reply(wire::APPEND, 1, status, 6))]);
        assert_eq!(cli.append(6, &[0; 96]), result);
        assert_eq!(runtime::counts(), (1, 1));
    }
}
