//! Tampon de démarrage du gestionnaire. Un envoi admis quitte la file une
//! seule fois. Seule une réponse authentifiée du journal atteste sa persistance.
use super::{Binding, Frame, instance, j, rt};

const CAPACITY: usize = 32;
const TAG_BASE: u64 = 1 << 63;
pub struct Publisher {
    pub identity: [u64; 4],
    queue: [Frame; CAPACITY],
    first: usize,
    count: usize,
    serial: u64,
    flight: Option<(u64, u64)>,
    pub lost: u64,
    pub uncertain: u64,
    pub last_error: u64,
}

impl Publisher {
    pub fn new(active: bool) -> rt::Result<Self> {
        let mut identity = [0u64; 4];
        if active {
            rt::value(rt::call(
                instance::INFO,
                2,
                identity.as_mut_ptr() as u64,
                32,
            ))?;
            if !j::valid_identity(identity) {
                return Err(rt::REFUSED);
            }
        }
        Ok(Self {
            identity,
            queue: [[0; 16]; CAPACITY],
            first: 0,
            count: 0,
            serial: 0,
            flight: None,
            lost: 0,
            uncertain: 0,
            last_error: rt::OK,
        })
    }

    pub fn record(
        &mut self,
        id: u64,
        subject: [u64; 4],
        level: u8,
        category: u8,
        code: u16,
        time: u64,
        data: [u64; 2],
    ) {
        self.enqueue(
            id,
            subject,
            self.identity,
            level,
            category,
            j::MANAGER,
            code,
            time,
            data,
        );
    }

    pub fn notice(&mut self, live: Binding, time: u64) {
        self.enqueue(
            live.id,
            live.identity,
            live.identity,
            j::INFO,
            j::SERVICE,
            j::PRODUCER,
            j::NOTICE,
            time,
            [super::p::ANNOUNCE, live.attempt],
        );
    }

    fn enqueue(
        &mut self,
        id: u64,
        subject: [u64; 4],
        source: [u64; 4],
        level: u8,
        category: u8,
        origin: u8,
        code: u16,
        time: u64,
        data: [u64; 2],
    ) {
        if self.identity == [0; 4] {
            return;
        }
        if self.count == CAPACITY || self.serial == TAG_BASE - 1 {
            self.lost = self.lost.saturating_add(1);
            self.last_error = rt::LIMIT;
            return;
        }
        self.serial += 1;
        let tail = (self.first + self.count) % CAPACITY;
        self.queue[tail] = j::event(
            self.serial,
            subject,
            source,
            id,
            level,
            category,
            origin,
            code,
            time,
            data,
        );
        self.count += 1;
    }

    pub fn submit(&mut self, logger: Binding, time: u64) {
        if self.flight.is_some_and(|(_, deadline)| time >= deadline) {
            self.disconnected();
        }
        if self.flight.is_some() || self.count == 0 {
            return;
        }
        let frame = &self.queue[self.first];
        let tag = TAG_BASE | frame[4];
        match rt::send_frame(logger.tx, tag, &j::encode(frame), 0, false) {
            Ok(()) => self.flight = Some((tag, time.saturating_add(20_000))),
            Err(rt::WOULD_BLOCK | rt::REVOKED | rt::DENIED | rt::CLOSED) => return,
            Err(e) => {
                self.last_error = e;
                self.lost = self.lost.saturating_add(1);
            }
        }
        self.first = (self.first + 1) % CAPACITY;
        self.count -= 1;
    }

    pub fn acknowledge(&mut self, tag: u64, frame: &Frame) -> bool {
        if tag & TAG_BASE == 0 {
            return false;
        }
        if self.flight.is_some_and(|(current, _)| tag == current) {
            if frame[..2] != [j::VERSION, j::EMIT] || (frame[2] == rt::OK && frame[3] == 0) {
                self.disconnected();
            } else {
                self.flight = None;
                self.last_error = frame[2];
                if frame[2] == super::config_client::UNCERTAIN {
                    self.uncertain = self.uncertain.saturating_add(1);
                } else if frame[2] != rt::OK {
                    self.lost = self.lost.saturating_add(1);
                }
            }
        }
        true
    }

    pub fn disconnected(&mut self) {
        if self.flight.take().is_some() {
            self.uncertain = self.uncertain.saturating_add(1);
            self.last_error = super::config_client::UNCERTAIN;
        }
    }
    pub fn idle(&self) -> bool {
        self.count == 0 && self.flight.is_none()
    }
}
