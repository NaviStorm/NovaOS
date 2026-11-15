//! Contrat de la file du gestionnaire face à l'absence de journal et à une réponse perdue.
#![allow(dead_code)]
#[path = "../user/services/journal_wire.rs"]
mod j;
#[path = "../user/services/journal_publisher.rs"]
mod publisher;
use j::Frame;
use std::sync::Mutex;
mod p {
    pub const ANNOUNCE: u64 = 16;
}
mod config_client {
    pub const UNCERTAIN: u64 = 22;
}
mod instance {
    pub const INFO: u64 = 1;
}

#[derive(Clone, Copy, Default)]
struct Binding {
    tx: u64,
    id: u64,
    identity: [u64; 4],
    attempt: u64,
}
static CHANNEL: Mutex<Vec<(u64, Frame)>> = Mutex::new(Vec::new());
mod rt {
    pub type Result<T> = core::result::Result<T, u64>;
    pub const OK: u64 = 0;
    pub const REFUSED: u64 = 12;
    pub const LIMIT: u64 = 6;
    pub const WOULD_BLOCK: u64 = 7;
    pub const REVOKED: u64 = 4;
    pub const DENIED: u64 = 5;
    pub const CLOSED: u64 = 11;

    pub fn call(_: u64, _: u64, address: u64, _: u64) -> u64 {
        unsafe {
            *(address as *mut [u64; 4]) = [1, 2, 3, 4];
        }
        0
    }
    pub fn value(n: u64) -> Result<u64> {
        Ok(n)
    }
    pub fn send_frame(tx: u64, tag: u64, buf: &[u8], _: u64, _: bool) -> Result<()> {
        if tx == 0 {
            return Err(WOULD_BLOCK);
        }
        super::CHANNEL
            .lock()
            .unwrap()
            .push((tag, super::j::decode(buf).unwrap()));
        Ok(())
    }
}

#[test]
fn saturation_retrait_incertitude_et_reponse_tardive() {
    let mut p = publisher::Publisher::new(true).unwrap();
    let mut logger = Binding::default();
    for n in 0..40 {
        p.record(9, [1, 2, 10, 2], j::INFO, j::LIFECYCLE, j::STATE, n, [4, 1]);
        p.submit(logger, n); // Journal indisponible, file bornée.
    }
    assert_eq!((p.lost, p.uncertain), (8, 0));
    logger.tx = 1;
    p.submit(logger, 100);
    let (one, first) = CHANNEL.lock().unwrap()[0];
    assert_eq!(first[4], 1);
    p.disconnected(); // Après remise, un seul résultat incertain, pas de répétition.
    p.submit(logger, 101);
    let (two, second) = CHANNEL.lock().unwrap()[1];
    assert_ne!(one, two);
    assert_eq!(second[4], 2);
    let mut ack = j::request(j::EMIT);
    ack[3] = 1;
    p.acknowledge(one, &ack); // L'ancien acquittement ne débloque pas le suivant.
    p.submit(logger, 102);
    assert_eq!(CHANNEL.lock().unwrap().len(), 2);
    ack[2] = config_client::UNCERTAIN;
    p.acknowledge(two, &ack);
    for n in 3..=32 {
        p.submit(logger, n + 102);
        let (tag, frame) = *CHANNEL.lock().unwrap().last().unwrap();
        assert_eq!(frame[4], n);
        ack[2] = rt::OK;
        ack[3] = n;
        assert!(p.acknowledge(tag, &ack));
    }
    assert!(p.idle());
    assert_eq!((p.lost, p.uncertain), (8, 2));
    assert_eq!(CHANNEL.lock().unwrap().len(), 32);
}
