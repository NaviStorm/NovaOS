//! État de la session d'information NVFS du service de stockage sans volume.
use crate::runtime as rt;

pub struct Session {
    last: u64,
}
impl Session {
    pub const fn new() -> Self {
        Self { last: 0 }
    }

    pub fn active(&self) -> bool {
        self.last != 0
    }

    pub fn answer(&mut self, tag: u64, packet: &[u8]) -> [u8; 12] {
        let status = if tag <= self.last || packet.len() != 8 {
            rt::BAD_FRAME
        } else {
            self.last = tag;
            if packet[0] != rt::WIRE_VERSION || packet[2..4] != [0; 2] {
                rt::BAD_FRAME
            } else if packet[4..8] != [0; 4] {
                rt::NO_VOLUME
            } else {
                match packet[1] {
                    rt::INFO => 0,
                    rt::FIN => {
                        self.last = 0;
                        0
                    }
                    _ => rt::UNSUPPORTED,
                }
            }
        };
        let mut answer = [0; 12];
        answer[0] = rt::WIRE_VERSION;
        answer[1] = status;
        answer[8..].copy_from_slice(rt::IDENTITY);
        answer
    }
}
