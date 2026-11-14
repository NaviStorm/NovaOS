//! Contrôleur virtio-IOMMU de virt, jamais accessible au pilote EL0.
use super::{
    Device, Error, Pages, Queue, Result, Segment,
    io::{barrier, wait},
};
use crate::hal::timer;
pub(super) struct Domain {
    sid: u8,
    unit: Device,
    requests: Option<Queue>,
    events: Option<Queue>,
    vol: Option<(u8, u32)>,
    retrait_slot: Option<usize>,
    retrait_global: bool,
    attache: bool,
}
impl Domain {
    pub fn preparer(sid: u8) -> Result<Self> {
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
        Ok(Self {
            sid,
            unit,
            requests: None,
            events: None,
            vol: None,
            retrait_slot: None,
            retrait_global: false,
            attache: false,
        })
    }
    pub fn start(&mut self, p: &Pages) -> Result<()> {
        let unit = self.unit;
        let a = [p.0[0], p.0[1], p.0[2]];
        let b = [p.0[3], p.0[4], p.0[5]];
        self.requests = Some(Queue::new(unit, 0, a, a.map(|f| f.pa))?);
        self.events = Some(Queue::new(unit, 1, b, b.map(|f| f.pa))?);
        unit.ready()?;
        self.events.as_mut().unwrap().faults(p.0[8]);
        let req = p.0[6].mem;
        req.w32(0, 5);
        req.w32(4, u32::from(self.sid)); // PROBE, valider aussi les réserves MSI
        self.request(p, 72, 516)?;
        if (0..512).any(|i| p.0[7].mem.r8(i) != 0) {
            return Err(Error::Route);
        }
        req.w32(0, 1);
        req.w32(4, 1);
        req.w32(8, u32::from(self.sid));
        req.w32(12, 0);
        req.w32(16, 0);
        self.request(p, 20, 4)
    }
    fn request(&mut self, p: &Pages, n: u32, res: u32) -> Result<()> {
        for i in 0..res as usize {
            p.0[7].mem.w8(i, 0xff)
        }
        let op = p.0[6].mem.r8(0);
        self.requests.as_mut().ok_or(Error::State)?.submit(&[
            Segment::new(p.0[6].pa, n, false),
            Segment::new(p.0[7].pa, res, true),
        ])?;
        self.vol = Some((op, res));
        self.collecter(p)
    }
    // Une commande non acquittée garde ses deux tampons de requête/réponse.
    // Ne jamais les écraser pour préparer une révocation pendant son exécution.
    fn collecter(&mut self, p: &Pages) -> Result<()> {
        let Some((op, res)) = self.vol else {
            return Ok(());
        };
        let longueur = self.requests.as_ref().ok_or(Error::State)?.completed()?;
        if longueur != res {
            return Err(Error::Quarantine);
        }
        self.vol = None;
        if p.0[7].mem.r32(res as usize - 4) != 0 {
            self.retrait_slot = None;
            return Err(Error::Fault);
        }
        match op {
            1 => self.attache = true,
            2 => self.attache = false,
            _ => (),
        }
        Ok(())
    }
    pub fn map(&mut self, p: &Pages, n: usize, pa: u64, rights: u32) -> Result<()> {
        if n >= 7 || pa % 4096 != 0 || rights > 3 {
            return Err(Error::Geometry);
        }
        // Après un timeout UNMAP, consommer son acquittement avant toute écriture
        // dans la paire requête/réponse. Une seule suppression pour ce slot.
        let precedent = self.retrait_slot;
        self.collecter(p)?;
        self.retrait_slot = None;
        if rights == 0 && precedent == Some(n) {
            return Ok(());
        }
        let req = p.0[6].mem;
        req.w32(0, if rights == 0 { 4 } else { 3 });
        req.w32(4, 1);
        req.w64(8, super::addr(n));
        req.w64(16, super::addr(n) + 4095);
        if rights == 0 {
            req.w32(24, 0);
            self.retrait_slot = Some(n);
            let r = self.request(p, 28, 4);
            if self.vol.is_none() {
                self.retrait_slot = None;
            }
            r
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
        if self.events.as_ref().ok_or(Error::State)?.pages[2]
            .mem
            .r16(2)
            == 0
        {
            Ok(())
        } else {
            Err(Error::Fault)
        }
    }
    fn detacher(&mut self, p: &Pages) -> Result<()> {
        self.collecter(p)?;
        if self.attache {
            let req = p.0[6].mem;
            req.w32(0, 2);
            req.w32(4, 1);
            req.w32(8, u32::from(self.sid));
            req.w32(12, 0);
            req.w32(16, 0);
            self.request(p, 20, 4)?;
        }
        Ok(())
    }
    // Uniquement après drainage du disque, reset, BME=0 et purge des IRQ.
    pub fn retirer_slot(&mut self, p: &Pages, n: usize) -> Result<()> {
        if !self.retrait_global {
            match self.map(p, n, 0, 0) {
                Ok(()) => return Ok(()),
                Err(Error::Fault | Error::Geometry) => self.retrait_global = true,
                Err(e) => return Err(e),
            }
        }
        // Le statut NOENT d’un UNMAP ne prouve rien sur les autres traductions.
        // Seul le retour du DETACH de cet endpoint révoque tout le domaine.
        self.detacher(p)
    }
    pub fn stop(&mut self, p: &Pages) -> Result<()> {
        self.detacher(p)?;
        self.unit.reset()?;
        self.unit.cfg.master(false);
        wait(|| self.unit.cfg.get(4, 2) & 4 == 0)
    }
}
impl Domain {
    pub fn garer(self) {}
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
