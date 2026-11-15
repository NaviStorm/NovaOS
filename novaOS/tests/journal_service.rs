//! Exécute les décisions du vrai démon et le backend P9 réel. Les canaux IPC
//! sont substitués aux bornes pour choisir exactement les points de retrait.
extern crate alloc;
#[allow(dead_code)]
#[path = "../abi/native.rs"]
mod abi;
#[path = "../user/services/journal_access.rs"]
mod access;
#[allow(dead_code)]
#[path = "../user/novafs/files/config_store.rs"]
mod config_store;
#[allow(dead_code)]
#[path = "../abi/files.rs"]
mod file;
#[path = "../user/services/journal.rs"]
mod journal;
#[path = "../user/services/journal_wire.rs"]
mod journal_wire;
#[allow(dead_code)]
#[path = "../user/novafs/files/journal_store.rs"]
mod store;

mod runtime {
    pub use crate::abi::*;
    use crate::journal_wire as w;
    use std::cell::RefCell;
    pub type Result<T> = core::result::Result<T, u64>;
    thread_local! {
        static FRAMES: RefCell<Vec<(u64,u64,w::Frame)>> = const { RefCell::new(Vec::new()) };
        static REVOKED_CAPS: RefCell<Vec<u64>> = const { RefCell::new(Vec::new()) };
    }
    pub fn revoke(cap: u64) {
        REVOKED_CAPS.with_borrow_mut(|v| v.push(cap));
    }
    pub fn take() -> Vec<(u64, u64, w::Frame)> {
        FRAMES.with_borrow_mut(std::mem::take)
    }
    pub fn send_frame(cap: u64, tag: u64, bytes: &[u8], _: u64, _: bool) -> Result<()> {
        if REVOKED_CAPS.with_borrow(|v| v.contains(&cap)) {
            return Err(REVOKED);
        }
        FRAMES.with_borrow_mut(|v| v.push((cap, tag, w::decode(bytes).unwrap())));
        Ok(())
    }
}

mod journal_backend {
    use crate::{
        file, runtime as rt,
        store::{Journal, wire},
    };
    use novafs::{Engine, Ids, device::SimDevice, time::Atime};
    use std::{
        cell::{Cell, RefCell},
        rc::Rc,
    };
    pub use wire::Status;
    #[derive(Clone)]
    pub struct Client {
        fs: Rc<RefCell<Engine<SimDevice>>>,
        pub reads: Rc<Cell<usize>>,
        pub admissions: Rc<Cell<usize>>,
        pub lose_ack: Rc<Cell<bool>>,
        pub corrupt: Rc<Cell<bool>>,
    }
    impl Client {
        pub fn new(sector: u32) -> Self {
            let fs = Engine::format_p9(
                SimDevice::new(768, sector).unwrap(),
                Ids {
                    fs: [33; 16],
                    lv: [52; 16],
                },
                Atime::No,
                None,
            )
            .unwrap();
            Self {
                fs: Rc::new(RefCell::new(fs)),
                reads: Rc::default(),
                admissions: Rc::default(),
                lose_ack: Rc::default(),
                corrupt: Rc::default(),
            }
        }
        pub fn status(&mut self) -> rt::Result<Status> {
            if self.corrupt.get() {
                return Err(file::CORRUPT);
            }
            Ok(Journal::load(&self.fs.borrow()).unwrap().status())
        }
        pub fn read(&mut self, seq: u64) -> rt::Result<[u8; 96]> {
            if self.corrupt.get() {
                return Err(file::CORRUPT);
            }
            self.reads.set(self.reads.get() + 1);
            Journal::load(&self.fs.borrow())
                .unwrap()
                .read(seq)
                .map_err(|_| file::NOT_FOUND)
        }
        pub fn append(&mut self, expected: u64, event: &[u8; 96]) -> rt::Result<u64> {
            self.admissions.set(self.admissions.get() + 1);
            let mut fs = self.fs.borrow_mut();
            let seq = Journal::load(&fs)
                .unwrap()
                .append(&mut *fs, expected, event)
                .map_err(|_| file::BUSY)?;
            if self.lose_ack.replace(false) {
                Err(file::UNCERTAIN)
            } else {
                Ok(seq)
            }
        }
        fn alter(&mut self, op: u64, rev: u64, count: u64) -> rt::Result<u64> {
            let mut fs = self.fs.borrow_mut();
            Journal::load(&fs)
                .unwrap()
                .administer(&mut *fs, op, rev, count)
                .map_err(|_| file::BUSY)
        }
        pub fn rotate(&mut self, r: u64) -> rt::Result<u64> {
            self.alter(wire::ROTATE, r, 0)
        }
        pub fn retain(&mut self, r: u64, n: u64) -> rt::Result<u64> {
            self.alter(wire::RETAIN, r, n)
        }
        pub fn purge(&mut self, r: u64) -> rt::Result<u64> {
            self.alter(wire::PURGE, r, 0)
        }
    }
}

use journal::Journal;
use journal_backend::Client;
use journal_wire as w;
use runtime as rt;
const M: w::Identity = [41, 65, 2, 1];
const C: w::Identity = [41, 65, 4, 1];
const FOREIGN: w::Identity = [41, 65, 4, 2];
const PORT: u64 = 17;
fn daemon(backend: &Client, owner: w::Identity, mask: u64, port: u64) -> Journal {
    Journal::new(backend.clone(), owner, M, mask, port)
}
fn emit(n: u64, v: u64) -> w::Frame {
    let mut f = w::request(w::SELF_NOTICE);
    f[3] = n;
    f[4] = v;
    f
}
fn ask(op: u64, service: u64, cursor: u64) -> w::Frame {
    let mut f = w::request(op);
    f[3] = cursor;
    f[4] = service;
    f
}
fn manager(n: u64) -> w::Frame {
    w::event(
        n,
        C,
        M,
        9,
        w::INFO,
        w::LIFECYCLE,
        w::MANAGER,
        w::STATE,
        n * 100,
        [3, 4],
    )
}
fn immediate(d: &mut Journal, f: &w::Frame) -> w::Frame {
    d.terminal(f, 11).unwrap().unwrap()
}

#[test]
fn six_independent_rights_and_all_64_combinations() {
    let cases = [
        (w::EMIT, 0, 0),
        (w::NEXT, w::SELF, w::OWN | w::SYSTEM_READ),
        (w::NEXT, 9, w::SYSTEM_READ),
        (w::WATCH, 0, w::SYSTEM_READ),
        (w::ROTATE, 0, w::ROTATION_RIGHT),
        (w::RETAIN, 0, w::RETENTION_RIGHT),
        (w::PURGE, 0, w::PURGE_RIGHT),
        (w::SELF_NOTICE, 0, w::EMIT_RIGHT),
        (
            w::STATUS,
            0,
            w::SYSTEM_READ | w::ROTATION_RIGHT | w::RETENTION_RIGHT | w::PURGE_RIGHT,
        ),
    ];
    for mask in 0..64 {
        for (op, id, need) in cases {
            let mut f = ask(op, id, 0);
            f[9] = 63; // une revendication de droits ne les accorde jamais
            assert_eq!(access::client_right(mask, &f).is_ok(), mask & need != 0);
        }
    }
}

#[test]
fn issuer_and_frame_bound_to_origin_channel() {
    let f = manager(1);
    let old = [42, 65, C[2], C[3]];
    let bad = w::event(
        9,
        old,
        M,
        9,
        w::INFO,
        w::LIFECYCLE,
        w::MANAGER,
        w::STATE,
        10,
        [0, 0],
    );
    assert_eq!(access::manager_event(&bad, M), Err(rt::BAD_ARGUMENT));
    let wider = w::event(
        9,
        C,
        M,
        65545,
        w::INFO,
        w::LIFECYCLE,
        w::MANAGER,
        w::STATE,
        10,
        [0, 0],
    );
    assert_eq!(access::manager_event(&wider, M), Err(rt::BAD_ARGUMENT));
    assert_eq!(w::decode(&w::encode(&f)), Some(f));
    for n in [0, 1, 32, 96, 127, 129, 4096] {
        assert!(w::decode(&vec![0; n]).is_none());
    }
    let valid = access::manager_event(&f, M).unwrap();
    assert_eq!(w::from_bytes(&w::bytes(&valid)), valid);
    for bad in [[0, 0, 2, 1], [41, 66, 2, 1], [41, 65, 3, 1], [41, 65, 2, 2]] {
        assert!(access::manager_event(&f, bad).is_err());
    }
    let notice = w::event(
        3,
        C,
        C,
        9,
        w::INFO,
        w::SERVICE,
        w::PRODUCER,
        w::NOTICE,
        140,
        [16, 1],
    );
    assert!(access::manager_event(&notice, M).is_ok());
    for (i, val) in [(3, 1), (4, 0), (5, 1), (8, 0), (11, 0)] {
        let mut bad = notice;
        bad[i] = val;
        assert!(access::manager_event(&bad, M).is_err());
    }
    let mut bad = notice;
    bad[12] = w::meta(9, w::INFO, w::SERVICE, w::PRODUCER, w::STATE, w::MONOTONIC);
    assert_eq!(access::manager_event(&bad, M), Err(rt::DENIED));
    let mut bad = emit(1, 22);
    bad[8] = C[2];
    assert_eq!(access::notice(&bad, C), Err(rt::BAD_ARGUMENT));
    for n in [0, 1, 2, 3, 4, 5] {
        let mut f = ask(w::NEXT, 9, 0);
        f[5] = n;
        assert!(access::Filter::parse(&f).is_ok());
    }
    for (idx, val) in [(4, 65536), (5, 6), (8, 2), (15, 1)] {
        let mut f = ask(w::NEXT, 9, 0);
        f[idx] = val;
        assert!(access::Filter::parse(&f).is_err());
    }
    let mut reserved = emit(1, 255);
    reserved[4] = 256;
    assert_eq!(access::notice(&reserved, C), Err(rt::BAD_ARGUMENT));
}

#[test]
fn own_scope_hides_sequence_other_boot_and_other_generation() {
    for sector in [512, 4096] {
        let db = Client::new(sector);
        let mut admin = daemon(&db, C, 63, PORT);
        admin
            .append(access::manager_event(&manager(3), M).unwrap())
            .unwrap();
        immediate(&mut admin, &emit(50, 17));
        let mut other = daemon(&db, FOREIGN, w::OWN, 18);
        assert_eq!(other.terminal(&ask(w::NEXT, 9, 0), 1), Err(rt::DENIED));
        assert_eq!(other.terminal(&w::request(w::STATUS), 1), Err(rt::DENIED));
        assert_eq!(
            immediate(&mut other, &ask(w::NEXT, w::SELF, 0))[2..4],
            [rt::WOULD_BLOCK, 0]
        );
        let oldboot = daemon(&db, [42, 65, 4, 1], w::OWN, 19);
        assert_eq!(
            immediate(&mut { oldboot }, &ask(w::NEXT, w::SELF, 0))[2..4],
            [rt::WOULD_BLOCK, 0]
        );
        let mut own = daemon(&db, C, w::OWN, 20);
        let visible = immediate(&mut own, &ask(w::NEXT, w::SELF, 0));
        assert_eq!(visible[3..6], [51, 50, 50]); // n'expose pas la séquence système 2
        let mut r = ask(w::NEXT, w::SELF, 0);
        r[8] = 1;
        assert_eq!(immediate(&mut own, &r)[3], 49);
        for (i, v) in [(5, u64::from(w::WARN)), (7, 999)] {
            let mut r = ask(w::NEXT, w::SELF, 0);
            r[i] = v;
            assert_eq!(immediate(&mut own, &r)[2..4], [rt::WOULD_BLOCK, 0]);
        }
        let mut rev = daemon(&db, C, w::ROTATION_RIGHT, 21);
        let status = immediate(&mut rev, &w::request(w::STATUS));
        let mut expected = w::request(w::STATUS);
        expected[7] = 2;
        expected[13] = 1;
        assert_eq!(status, expected);
    }
}

#[test]
fn watch_fairness_snapshot_and_wakeup_after_durable_append() {
    for sector in [512, 4096] {
        let db = Client::new(sector);
        let mut j = daemon(&db, C, 63, PORT);
        let query = ask(w::WATCH, w::SELF, 0);
        assert!(j.terminal(&query, 411).unwrap().is_none());
        assert_eq!(j.terminal(&query, 412), Err(file::BUSY));
        j.append(access::manager_event(&manager(4), M).unwrap())
            .unwrap();
        assert!(rt::take().is_empty());
        for _ in 0..30 {
            j.advance(2);
        }
        assert_eq!(db.reads.get(), 0);
        j.advance(2);
        let replies = rt::take();
        assert_eq!(
            replies,
            vec![(PORT, 411, journal::reply(w::WATCH, rt::WOULD_BLOCK, 0))]
        );
        assert!(j.terminal(&query, 413).unwrap().is_none());
        immediate(&mut j, &emit(1, 17));
        let got = rt::take();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].1, 413);
        assert_eq!(got[0].2[2..6], [rt::OK, 2, 1, 1]);
        let mut watch = ask(w::WATCH, 0, 3);
        assert!(j.terminal(&watch, 500).unwrap().is_none());
        let mut rot = w::request(w::ROTATE);
        rot[3] = 2;
        immediate(&mut j, &rot);
        assert_eq!(
            rt::take(),
            vec![(PORT, 500, journal::reply(w::WATCH, rt::WOULD_BLOCK, 3))]
        );
        watch[3] = 3;
        assert!(j.terminal(&watch, 501).unwrap().is_none());
        for _ in 0..64 {
            j.advance(0);
        }
        assert_eq!(
            rt::take(),
            vec![(PORT, 501, journal::reply(w::WATCH, rt::WOULD_BLOCK, 3))]
        );
        let mut purge = w::request(w::PURGE);
        purge[3] = 2;
        assert_eq!(j.terminal(&purge, 11), Err(file::BUSY));
        purge[3] = 3;
        immediate(&mut j, &purge);
        assert_eq!(
            immediate(&mut j, &ask(w::NEXT, 0, 1))[2..4],
            [rt::REVOKED, 3]
        );
        // Le plus ancien conservé porte désormais la séquence 3. La fin
        // naturelle en lecture inverse ne doit pas être signalée comme un retrait.
        immediate(&mut j, &emit(2, 8));
        let mut reverse = ask(w::NEXT, 0, 0);
        reverse[8] = 1;
        let last = immediate(&mut j, &reverse);
        assert_eq!((last[2], last[3], last[5]), (rt::OK, w::END, 3));
        reverse[3] = last[3];
        assert_eq!(immediate(&mut j, &reverse)[2], rt::WOULD_BLOCK);
    }
}

#[test]
fn revoked_pending_destination_and_restarted_identity_are_separate() {
    let db = Client::new(4096);
    let cap = 400;
    let mut j = daemon(&db, C, 63, cap);
    assert!(j.terminal(&ask(w::WATCH, 0, 0), 199).unwrap().is_none());
    rt::revoke(cap);
    let a = access::manager_event(&manager(1), M).unwrap();
    assert_eq!(j.append(a), Ok(1));
    assert!(rt::take().is_empty());
    assert!(j.terminal(&ask(w::WATCH, 0, 2), 201).unwrap().is_none());
    j.drop_client();
    j.append(access::manager_event(&manager(2), M).unwrap())
        .unwrap();
    assert!(rt::take().is_empty());
    drop(j);
    let mut new = daemon(&db, FOREIGN, 63, 401); // autre incarnation, nouveaux handles
    assert!(
        new.terminal(&ask(w::WATCH, w::SELF, 0), 211)
            .unwrap()
            .is_none()
    );
    immediate(&mut new, &emit(1, 42));
    let a = rt::take();
    assert_eq!(a.len(), 1);
    assert_eq!((a[0].0, a[0].1), (401, 211));
    let e: w::Event = a[0].2[4..].try_into().unwrap();
    assert_eq!(w::issuer(&e), FOREIGN);
}

#[test]
fn lost_durable_ack_is_never_replayed() {
    for sector in [512, 4096] {
        let mut db = Client::new(sector);
        let mut j = daemon(&db, C, 63, PORT);
        let e = emit(1, 7);
        db.lose_ack.set(true);
        assert!(j.terminal(&ask(w::WATCH, 0, 0), 700).unwrap().is_none());
        let before = db.admissions.get();
        assert_eq!(j.terminal(&e, 31), Err(file::UNCERTAIN));
        assert_eq!(j.terminal(&e, 32), Err(file::BUSY));
        assert_eq!(
            rt::take(),
            vec![(PORT, 700, journal::reply(w::WATCH, rt::WOULD_BLOCK, 1))]
        );
        let observed = immediate(&mut j, &ask(w::NEXT, 0, 1));
        assert_eq!((observed[2], observed[5]), (rt::OK, 1));
        assert_eq!(db.admissions.get(), before + 1);
        let s = db.status().unwrap();
        assert_eq!((s.first, s.next, s.revision), (1, 2, 1));
        assert_eq!(db.read(1).unwrap(), {
            let mut e = access::notice(&e, C).unwrap();
            e[1] = 1;
            w::bytes(&e)
        });
        immediate(&mut j, &emit(2, 5));
        assert_eq!(db.status().unwrap().next, 3);
        drop(j);
        let mut new = daemon(&db, C, 63, 25);
        immediate(&mut new, &e); // même EID retenu, sans nouvelle écriture P9
        assert_eq!(db.status().unwrap().revision, 2);
        assert_eq!(db.status().unwrap().next, 3);
    }
}

#[test]
fn own_follow_does_not_rescan_the_volume_when_idle() {
    let db = Client::new(512);
    let mut writer = daemon(&db, C, 63, 10);
    for i in 1..=48 {
        writer
            .append(access::manager_event(&manager(i), M).unwrap())
            .unwrap();
    }
    let mut reader = daemon(&db, FOREIGN, w::OWN, PORT);
    let query = ask(w::WATCH, w::SELF, 0);
    assert!(reader.terminal(&query, 710).unwrap().is_none());
    assert_eq!(db.reads.get(), 48);
    for tag in 711..714 {
        for _ in 0..31 {
            reader.advance(2);
        }
        assert_eq!(rt::take()[0].2[2..4], [rt::WOULD_BLOCK, 0]);
        assert!(reader.terminal(&query, tag).unwrap().is_none());
    }
    assert_eq!(db.reads.get(), 48);
}

#[test]
fn known_corruption_blocks_previously_cached_answers() {
    let db = Client::new(512);
    let mut j = daemon(&db, C, 63, PORT);
    for i in 1..=33 {
        j.append(access::manager_event(&manager(i), M).unwrap())
            .unwrap();
    }
    immediate(&mut j, &w::request(w::STATUS));
    db.corrupt.set(true);
    assert_eq!(j.terminal(&ask(w::NEXT, 0, 1), 680), Err(file::CORRUPT));
    assert_eq!(j.terminal(&ask(w::NEXT, 0, 33), 681), Err(file::CORRUPT));
    db.corrupt.set(false);
    assert_eq!(immediate(&mut j, &ask(w::NEXT, 0, 33))[5], 33);
}
