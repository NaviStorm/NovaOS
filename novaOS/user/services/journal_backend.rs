//! Client du canal privé de stockage du journal. L'appelant conserve la décision de reprise.
use super::runtime as rt;
#[path = "../../abi/files.rs"]
#[allow(dead_code)]
mod files;
#[path = "journal_storage_wire.rs"]
#[allow(dead_code)]
mod wire;
pub use wire::{Event, Status};
use wire::{Frame, get, header};

pub struct Client {
    tx: u64,
    rx: u64,
    tag: u64,
}
impl Client {
    pub fn new(tx: u64, rx: u64) -> Self {
        Self { tx, rx, tag: 0 }
    }

    fn exchange(&mut self, frame: Frame) -> rt::Result<Frame> {
        let op = get(&frame, 1);
        let mutation = !matches!(op, wire::STATUS | wire::READ);
        self.tag = self.tag.checked_add(1).ok_or(rt::LIMIT)?;
        // Le rendez-vous distingue la remise au serveur d'un refus d'admission.
        rt::send_for(self.tx, self.tag, &frame, 15000)?;
        let lost = |e| if mutation { files::UNCERTAIN } else { e };
        for _ in 0..16 {
            let msg = rt::receive_for(self.rx, 15000).map_err(lost)?;
            if msg.tag < self.tag && msg.kind == 1 {
                continue;
            }
            if msg.kind != 1
                || msg.len != 128
                || msg.tag != self.tag
                || get(&msg.payload, 0) != wire::MAGIC
                || get(&msg.payload, 1) != op
            {
                return Err(lost(rt::REFUSED));
            }
            let status = get(&msg.payload, 2);
            return if status == rt::OK {
                Ok(msg.payload)
            } else {
                Err(status)
            };
        }
        Err(lost(rt::REFUSED))
    }

    pub fn status(&mut self) -> rt::Result<Status> {
        let f = self.exchange(header(wire::STATUS, 0, 0))?;
        Ok(Status {
            first: get(&f, 4),
            next: get(&f, 5),
            segments: get(&f, 6),
            retention: get(&f, 7),
            revision: get(&f, 8),
            quota: get(&f, 9),
            bytes: get(&f, 10),
        })
    }
    pub fn read(&mut self, seq: u64) -> rt::Result<Event> {
        let f = self.exchange(header(wire::READ, seq, 0))?;
        let data: Event = f[32..].try_into().unwrap();
        if get(&data, 1) != seq {
            return Err(rt::REFUSED);
        }
        Ok(data)
    }
    pub fn append(&mut self, expected_next: u64, event: &Event) -> rt::Result<u64> {
        let mut f = header(wire::APPEND, expected_next, 0);
        f[32..].copy_from_slice(event);
        let seq = get(&self.exchange(f)?, 3);
        if seq == 0 || seq > expected_next {
            return Err(files::UNCERTAIN);
        }
        Ok(seq)
    }
    pub fn rotate(&mut self, revision: u64) -> rt::Result<u64> {
        Ok(get(&self.exchange(header(wire::ROTATE, revision, 0))?, 3))
    }
    pub fn purge(&mut self, revision: u64) -> rt::Result<u64> {
        Ok(get(&self.exchange(header(wire::PURGE, revision, 0))?, 3))
    }
    pub fn retain(&mut self, revision: u64, count: u64) -> rt::Result<u64> {
        Ok(get(
            &self.exchange(header(wire::RETAIN, revision, count))?,
            3,
        ))
    }
}
