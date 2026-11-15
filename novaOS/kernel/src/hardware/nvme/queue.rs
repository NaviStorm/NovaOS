//! Les pages SQ sont privées au courtier. Le périphérique écrit les CQ et la
//! destination. Ce profil x86 cohérent publie les octets avant la doorbell.
use super::{Command, Error, Memory, PAGE};
use core::sync::atomic::{Ordering, fence};

pub(super) const DEPTH: u16 = 16;
pub(super) const ASQ: usize = 0;
pub(super) const ACQ: usize = 1;
const IOSQ: usize = 2;
const IOCQ: usize = 3;
pub(super) const DATA: usize = 4;

impl Memory {
    pub(super) fn valid(self) -> bool {
        self.epoch != 0
            && self.bar != 0
            && self.bar.is_multiple_of(PAGE)
            && self.len >= 0x1010
            && self.bar.checked_add(self.len).is_some()
            && self.cpu.iter().enumerate().all(|(i, &p)| {
                p != 0
                    && p.is_multiple_of(PAGE)
                    && p.checked_add(PAGE).is_some()
                    && !self.cpu[..i].contains(&p)
                    && (p + PAGE <= self.bar || self.bar + self.len <= p)
            })
            && self.iova.iter().enumerate().all(|(i, &p)| {
                p != 0
                    && p.is_multiple_of(PAGE as u64)
                    && p.checked_add(PAGE as u64).is_some()
                    && !self.iova[..i].contains(&p)
            })
    }

    // Les seuls offsets admis sont les constantes de registres et les quatre
    // doorbells dont la borne supérieure a été contrôlée à l'admission.
    pub(super) fn reg32(&self, offset: usize) -> u32 {
        unsafe { ((self.bar + offset) as *const u32).read_volatile() }
    }

    pub(super) fn reg64(&self, offset: usize) -> u64 {
        unsafe { ((self.bar + offset) as *const u64).read_volatile() }
    }

    pub(super) fn put32(&self, offset: usize, value: u32) {
        unsafe { ((self.bar + offset) as *mut u32).write_volatile(value) }
    }

    pub(super) fn put64(&self, offset: usize, value: u64) {
        unsafe { ((self.bar + offset) as *mut u64).write_volatile(value) }
    }

    pub(super) fn zero(&self, page: usize) {
        unsafe { core::ptr::write_bytes(self.cpu[page] as *mut u8, 0, PAGE) };
    }
}

#[derive(Clone, Copy)]
pub(super) struct Queue {
    id: u16,
    tail: u16,
    head: u16,
    phase: bool,
}

impl Queue {
    pub(super) const fn new(id: u16) -> Self {
        Self {
            id,
            tail: 0,
            head: 0,
            phase: true,
        }
    }

    fn cq(&self, mem: &Memory) -> *const u32 {
        let page = if self.id == 0 { ACQ } else { IOCQ };
        (mem.cpu[page] + 16 * usize::from(self.head)) as *const u32
    }

    pub(super) fn has_completion(&self, mem: &Memory) -> bool {
        let status = unsafe { self.cq(mem).add(3).read_volatile() };
        (status & (1 << 16) != 0) == self.phase
    }

    pub(super) fn publish(&mut self, mem: &Memory, stride: usize, sqe: &[u32; 16]) {
        let page = if self.id == 0 { ASQ } else { IOSQ };
        let dest = (mem.cpu[page] + 64 * usize::from(self.tail)) as *mut u32;
        for (i, &value) in sqe.iter().enumerate() {
            unsafe { dest.add(i).write_volatile(value) };
        }
        self.tail = (self.tail + 1) % DEPTH;
        fence(Ordering::SeqCst);
        mem.put32(0x1000 + 2 * usize::from(self.id) * stride, self.tail.into());
    }

    pub(super) fn consume(
        &mut self,
        mem: &Memory,
        stride: usize,
        cid: u16,
    ) -> Result<Option<u32>, Error> {
        if !self.has_completion(mem) {
            return Ok(None);
        }
        fence(Ordering::Acquire);
        let cq = self.cq(mem);
        let result = unsafe { cq.read_volatile() };
        let heads = unsafe { cq.add(2).read_volatile() };
        let last = unsafe { cq.add(3).read_volatile() };
        if (last & (1 << 16) != 0) != self.phase
            || last as u16 != cid
            || (heads >> 16) as u16 != self.id
            || heads as u16 != self.tail
        {
            return Err(Error::Completion);
        }
        let status = (last >> 17) as u16;
        if status & 0x7ff != 0 {
            return Err(Error::Status(status));
        }
        self.head = (self.head + 1) % DEPTH;
        if self.head == 0 {
            self.phase = !self.phase;
        }
        fence(Ordering::SeqCst);
        mem.put32(
            0x1000 + (2 * usize::from(self.id) + 1) * stride,
            self.head.into(),
        );
        Ok(Some(result))
    }
}

pub(super) fn entry(command: Command, cid: u16, mem: &Memory) -> [u32; 16] {
    use Command::*;
    let mut entry = [0u32; 16];
    let (opcode, page) = match command {
        IdentifyController | ListNamespaces | IdentifyNamespace => {
            entry[10] = match command {
                IdentifyController => 1,
                ListNamespaces => 2,
                _ => 0,
            };
            if command == IdentifyNamespace {
                entry[1] = 1;
            }
            (0x06, Some(DATA))
        }
        NumberQueues => {
            entry[10] = 7;
            (0x09, None)
        }
        CreateCq | CreateSq => {
            entry[10] = 1 | (u32::from(DEPTH - 1) << 16);
            entry[11] = 1; // PC=1; CQ de polling IEN=0.
            if command == CreateSq {
                entry[11] |= 1 << 16; // SQ1 reliée à CQ1.
                (0x01, Some(IOSQ))
            } else {
                (0x05, Some(IOCQ))
            }
        }
        Read { nsid, lba, blocks } => {
            entry[1] = nsid;
            entry[10] = lba as u32;
            entry[11] = (lba >> 32) as u32;
            entry[12] = u32::from(blocks - 1); // Taille validée dans submit.
            (0x02, Some(DATA))
        }
        DeleteSq => {
            entry[10] = 1;
            (0x00, None)
        }
        DeleteCq => {
            entry[10] = 1;
            (0x04, None)
        }
    };
    entry[0] = opcode | (u32::from(cid) << 16);
    if let Some(index) = page {
        entry[6] = mem.iova[index] as u32;
        entry[7] = (mem.iova[index] >> 32) as u32;
    }
    entry
}
