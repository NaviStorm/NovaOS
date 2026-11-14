//! Le client réellement lié aux ELF, avec un transport déterministe qui ne fait aucune E/S.
#![allow(dead_code)]
#[path = "../user/storage/fileapi.rs"]
mod fileapi;
#[path = "../abi/files.rs"]
mod protocol;

mod runtime {
    pub use super::bulk;
    pub use super::native::*;
    use std::cell::RefCell;
    use std::collections::VecDeque;
    pub type Result<T> = core::result::Result<T, u64>;
    thread_local! { pub static PIPE: RefCell<(usize,VecDeque<Result<ReceiveV1>>)> = RefCell::default(); }
    pub fn channels() -> Result<(u64, u64)> {
        Ok((1, 2))
    }
    pub fn value(r: u64) -> Result<u64> {
        Ok(r)
    }
    pub fn call(_: u64, _: u64, _: u64, _: u64) -> u64 {
        unreachable!()
    }
    pub fn send_for(_: u64, tag: u64, q: &[u8], d: u64) -> Result<()> {
        assert_eq!((tag, d), (1, 15000));
        assert_eq!(q.len(), 128);
        PIPE.with(|p| p.borrow_mut().0 += 1);
        Ok(())
    }
    pub fn receive_for(_: u64, d: u64) -> Result<ReceiveV1> {
        assert_eq!(d, 15000);
        PIPE.with(|p| p.borrow_mut().1.pop_front().expect("réponse inattendue"))
    }
}
#[path = "../abi/bulk_call.rs"]
pub mod bulk;
#[path = "../abi/native.rs"]
mod native;
use protocol as w;
use runtime as rt;
fn msg(f: w::Frame) -> rt::ReceiveV1 {
    rt::ReceiveV1 {
        version: 1,
        size: 184,
        timeout: 0,
        options: 0,
        reserved: 0,
        kind: 1,
        len: 128,
        tag: 1,
        handle: 0,
        aux: 0,
        payload: f,
    }
}
fn client(seq: Vec<rt::Result<rt::ReceiveV1>>) -> fileapi::Client {
    rt::PIPE.with(|p| *p.borrow_mut() = (0, seq.into()));
    fileapi::Client::new().unwrap()
}
#[test]
fn arrivee_apres_deux_avancees() {
    let mut c = client(vec![
        Ok(msg(w::mount_progress(256))),
        Ok(msg(w::mount_progress(512))),
        Ok(msg(w::frame(w::MOUNT, 123, 0))),
    ]);
    c.mount().unwrap();
    assert_eq!(c.session, 123);
    rt::PIPE.with(|p| assert_eq!(p.borrow().0, 1));
}
#[test]
fn silence_et_retrait_ne_relancent_pas_la_requete() {
    for e in [rt::TIMED_OUT, rt::REVOKED, rt::CLOSED] {
        let mut c = client(vec![Ok(msg(w::mount_progress(256))), Err(e)]);
        assert_eq!(c.mount(), Err(e));
        assert_eq!(c.mount(), Err(rt::CLOSED));
        assert_eq!(c.session, 0);
        rt::PIPE.with(|p| assert_eq!(p.borrow().0, 1));
    }
}
#[test]
fn aucune_progression_inventee_ou_rejouee() {
    for i in 0..7 {
        let mut m = msg(w::mount_progress(512));
        match i {
            0 => m.tag = 2,
            1 => m.len = 127,
            2 => m.payload[127] = 1,
            3 => m.kind = 3,
            4 => w::set(&mut m.payload, 32, 256),
            5 => w::set(&mut m.payload, 32, 768),
            _ => m.payload[2] = 2,
        }
        let mut c = client(vec![Ok(msg(w::mount_progress(256))), Ok(m)]);
        assert_eq!(c.mount(), Err(rt::BAD_ARGUMENT));
        assert_eq!(c.session, 0);
    }
}
