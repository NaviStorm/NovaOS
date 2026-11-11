use super::{
    Error, Result,
    io::{Frame, Region, barrier, wait},
    pci::Device,
};
pub(super) const ENTRIES: u16 = 8;
#[derive(Clone, Copy)]
pub(super) struct Segment {
    pub addr: u64,
    pub len: u32,
    pub write: bool,
}
impl Segment {
    pub fn new(addr: u64, len: u32, write: bool) -> Self {
        Self { addr, len, write }
    }
}
pub(super) struct Queue {
    pub pages: [Frame; 3],
    door: Region,
    number: u16,
    pub index: u16,
}
impl Queue {
    pub fn new(device: Device, number: u16, pages: [Frame; 3], dma: [u64; 3]) -> Result<Self> {
        device.common.w16(22, number);
        if device.common.r16(28) != 0 || device.common.r16(24) < ENTRIES {
            return Err(Error::Queue);
        }
        let offset = usize::from(device.common.r16(30))
            .checked_mul(device.stride)
            .ok_or(Error::Geometry)?;
        let door = device.notify.sub(offset, 2)?;
        for p in pages {
            for off in (0..4096).step_by(8) {
                p.mem.w64(off, 0)
            }
        }
        device.common.w16(24, ENTRIES);
        for (i, addr) in dma.into_iter().enumerate() {
            device.common.w32(32 + i * 8, addr as u32);
            device.common.w32(36 + i * 8, (addr >> 32) as u32);
        }
        barrier();
        device.common.w16(28, 1);
        if device.common.r16(28) != 1 {
            return Err(Error::Queue);
        }
        Ok(Self {
            pages,
            door,
            number,
            index: 0,
        })
    }
    pub fn submit(&mut self, parts: &[Segment]) -> Result<()> {
        if self.pages[2].mem.r16(2) != self.index || parts.is_empty() || parts.len() > 3 {
            return Err(Error::Queue);
        }
        for (i, p) in parts.iter().enumerate() {
            if p.len == 0 || p.addr.checked_add(u64::from(p.len)).is_none() {
                return Err(Error::Geometry);
            }
            let desc = self.pages[0].mem;
            let o = i * 16;
            let more = i + 1 < parts.len();
            desc.w64(o, p.addr);
            desc.w32(o + 8, p.len);
            desc.w16(o + 12, (more as u16) | ((p.write as u16) << 1));
            desc.w16(o + 14, if more { i as u16 + 1 } else { 0 });
        }
        self.pages[1]
            .mem
            .w16(4 + 2 * usize::from(self.index % ENTRIES), 0);
        barrier();
        self.index = self.index.wrapping_add(1);
        self.pages[1].mem.w16(2, self.index);
        barrier();
        self.door.w16(0, self.number);
        Ok(())
    }
    pub fn completed(&self) -> Result<u32> {
        wait(|| self.pages[2].mem.r16(2) == self.index)?;
        let pos = 4 + 8 * usize::from(self.index.wrapping_sub(1) % ENTRIES);
        if self.pages[2].mem.r32(pos) != 0 {
            return Err(Error::Queue);
        }
        Ok(self.pages[2].mem.r32(pos + 4))
    }
    #[cfg(target_arch = "aarch64")]
    pub fn faults(&mut self, buffer: Frame) {
        for i in 0..usize::from(ENTRIES) {
            let m = self.pages[0].mem;
            m.w64(i * 16, buffer.pa + i as u64 * 24);
            m.w32(i * 16 + 8, 24);
            m.w16(i * 16 + 12, 2);
            self.pages[1].mem.w16(4 + i * 2, i as u16);
        }
        barrier();
        self.pages[1].mem.w16(2, ENTRIES);
        barrier();
        self.door.w16(0, self.number)
    }
}
