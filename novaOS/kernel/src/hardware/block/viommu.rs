//! Contrôleur virtio-IOMMU de virt, jamais accessible au pilote EL0.
use super::{
    Device, Error, Pages, Queue, Result, Segment,
    io::{barrier, wait},
};
use crate::hal::timer;
pub(super) struct Domain {
    sid: u8,
    unit: Device,
    requests: Queue,
    events: Queue,
}
impl Domain {
    pub fn start(p: &Pages, sid: u8) -> Result<Self> {
        super::pci::exclusive_intx(sid)?;
        let unit = Device::discover(5 << 3, 0x1057, 272)?;
        unit.cfg.intx(false);
        unit.start((1 << 32) | 0x57)?;
        if unit.specific.r64(0) & 4096 == 0
            || unit.specific.r8(36) != 0
            || unit.specific.r32(32) != 512
            || unit.specific.r64(8) > super::IOVA
            || unit.specific.r64(16) < super::addr(7) - 1
            || !(unit.specific.r32(24)..=unit.specific.r32(28)).contains(&1)
        {
            return Err(Error::Feature);
        }
        let a = [p.0[0], p.0[1], p.0[2]];
        let b = [p.0[3], p.0[4], p.0[5]];
        let requests = Queue::new(unit, 0, a, a.map(|f| f.pa))?;
        let mut events = Queue::new(unit, 1, b, b.map(|f| f.pa))?;
        unit.ready()?;
        events.faults(p.0[8]);
        let mut d = Self {
            sid,
            unit,
            requests,
            events,
        };
        let req = p.0[6].mem;
        req.w32(0, 5);
        req.w32(4, u32::from(sid)); // PROBE, valider aussi les réserves MSI
        d.request(p, 72, 516)?;
        if (0..512).any(|i| p.0[7].mem.r8(i) != 0) {
            return Err(Error::Route);
        }
        req.w32(0, 1);
        req.w32(4, 1);
        req.w32(8, u32::from(sid));
        req.w32(12, 0);
        req.w32(16, 0);
        d.request(p, 20, 4)?;
        Ok(d)
    }
    fn request(&mut self, p: &Pages, n: u32, res: u32) -> Result<()> {
        for i in 0..res as usize {
            p.0[7].mem.w8(i, 0xff)
        }
        self.requests.submit(&[
            Segment::new(p.0[6].pa, n, false),
            Segment::new(p.0[7].pa, res, true),
        ])?;
        if self.requests.completed()? != res || p.0[7].mem.r32(res as usize - 4) != 0 {
            return Err(Error::Fault);
        }
        Ok(())
    }
    pub fn map(&mut self, p: &Pages, n: usize, pa: u64, rights: u32) -> Result<()> {
        if n >= 7 || pa % 4096 != 0 || rights > 3 {
            return Err(Error::Geometry);
        }
        let req = p.0[6].mem;
        req.w32(0, if rights == 0 { 4 } else { 3 });
        req.w32(4, 1);
        req.w64(8, super::addr(n));
        req.w64(16, super::addr(n) + 4095);
        if rights == 0 {
            req.w32(24, 0);
            self.request(p, 28, 4)
        } else {
            req.w64(24, pa);
            req.w32(32, rights);
            self.request(p, 36, 4)
        }
    }
    pub fn route(&mut self, _: &Pages, d: Device, on: bool) -> Result<()> {
        if d.msix.is_some() {
            return Err(Error::Route);
        }
        timer::disk_line(on);
        d.cfg.intx(on);
        Ok(())
    }
    pub fn clear_pending(&self, d: Device) -> Result<()> {
        let _ = d.isr.r8(0);
        timer::purge_disk()
    }
    pub fn faults(&self, _: &Pages) -> Result<()> {
        barrier();
        if self.events.pages[2].mem.r16(2) == 0 {
            Ok(())
        } else {
            Err(Error::Fault)
        }
    }
    pub fn stop(&mut self, p: &Pages) -> Result<()> {
        let req = p.0[6].mem;
        req.w32(0, 2);
        req.w32(4, 1);
        req.w32(8, u32::from(self.sid));
        req.w32(12, 0);
        req.w32(16, 0); // réservé
        self.request(p, 20, 4)?;
        self.unit.reset()?;
        self.unit.cfg.master(false);
        wait(|| self.unit.cfg.get(4, 2) & 4 == 0)
    }
}
impl Domain {
    pub fn hold(&self, _: Device) -> Result<()> {
        timer::disk_line(false);
        Ok(())
    }
    pub fn resume_line(&self, _: Device) -> Result<()> {
        timer::disk_line(true);
        Ok(())
    }
    pub fn pending_on_mask(&self, _: Device) -> bool {
        timer::disk_pending()
    }
}
