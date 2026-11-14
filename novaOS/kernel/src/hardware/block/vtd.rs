//! VT-d Q35 en mode legacy, sans ATS ni PASID. Un SID, un domaine, une route.
use super::{
    Device, Error, IOVA, Pages, Result, SID,
    io::{EMPTY, Frame, Region, barrier, wait},
};
const TE: u32 = 1 << 31;
const QIE: u32 = 1 << 26;
const IRE: u32 = 1 << 25;
const RUN: u32 = TE | QIE | IRE;
const VECTOR: u8 = 0x51;
const ENTRY: usize = 4;

use crate::sched::sync::Mutex;
static PARKED: Mutex<Option<Domain>> = Mutex::new(None);
#[derive(PartialEq, Eq)]
enum Installation {
    Racine,
    Interruptions,
    File,
    Active,
    Stationnee,
}
pub(super) struct Domain {
    phase: Installation,
    en_vol: bool,
    anchors: [Frame; 9],
    regs: Region,
    apic: Region,
    tail: u16,
    cookie: u32,
    record: usize,
}
impl Domain {
    pub fn preparer(sid: u8) -> Result<Self> {
        if u16::from(sid) != SID {
            return Err(Error::Route);
        }
        {
            let mut garee = PARKED.lock();
            if let Some(prior) = garee.as_ref() {
                if prior.regs.r32(0x1c) & RUN != RUN
                    || prior.anchors[1].mem.r64(16 * usize::from(SID)) != 0
                {
                    return Err(Error::Quarantine);
                }
                return Ok(garee.take().unwrap());
            }
        }
        let regs = Region::map(0xfed9_0000, 320, 4096)?;
        let (lo, hi): (u32, u32);
        // Sûreté : lecture d'IA32_APIC_BASE à CPL0. Le profil utilise le LAPIC xAPIC du BSP.
        unsafe {
            core::arch::asm!("rdmsr",in("ecx")0x1bu32,out("eax")lo,out("edx")hi,options(nostack))
        }
        if lo & 0xc00 != 0x800 || lo & !0xfff != 0xfee0_0000 || hi != 0 {
            return Err(Error::Route);
        }
        let apic = Region::map(0xfee0_0000, 321, 4096)?;
        if apic.r32(0x20) >> 24 != 0 {
            return Err(Error::Route);
        }
        super::LAPIC.store(apic.base() as u64, core::sync::atomic::Ordering::Release);
        let caps = regs.r64(8);
        let ext = regs.r64(16);
        if regs.r32(0) >> 4 != 1
            || caps & (1 << 10) == 0
            || (caps >> 54) & 3 != 3
            || ext & 0x1a != 0x1a
        {
            return Err(Error::Feature);
        }
        if regs.r32(0x1c) & RUN != 0 {
            return Err(Error::State);
        }
        let record = (((caps >> 24) & 0x3ff) * 16) as usize;
        regs.sub(record, 16)?;
        let mut anchors = [EMPTY; 9];
        for i in 0..9 {
            match Frame::allocate() {
                Ok(v) => anchors[i] = v,
                Err(e) => {
                    // Sûreté : le moteur ne connaît encore aucun de ces cadres.
                    for f in &anchors[..i] {
                        unsafe { crate::memory::release_dma(f.pa) }
                    }
                    return Err(e);
                }
            }
        }
        let d = Self {
            phase: Installation::Racine,
            en_vol: false,
            anchors,
            regs,
            apic,
            tail: 0,
            cookie: 0,
            record,
        };
        anchors[0].mem.w64(0, anchors[1].pa | 1);
        anchors[1].mem.w64(16 * usize::from(SID), anchors[2].pa | 1);
        anchors[1].mem.w64(16 * usize::from(SID) + 8, 0x102);
        for (level, shift) in [39, 30, 21].into_iter().enumerate() {
            anchors[2 + level].mem.w64(
                8 * ((IOVA >> shift) & 511) as usize,
                anchors[3 + level].pa | 3,
            );
        }
        Ok(d)
    }
    // Les neuf cadres appartiennent à la machine après publication de RTADDR.
    // Une reprise termine l'étape amorcée sans réinitialiser un anneau actif.
    fn activer(&mut self) -> Result<()> {
        let regs = self.regs;
        let anchors = &self.anchors;
        if self.phase == Installation::Racine {
            barrier();
            regs.w64(0x20, anchors[0].pa);
            regs.w32(0x18, 1 << 30);
            wait(|| regs.r32(0x1c) & (1 << 30) != 0)?;
            self.phase = Installation::Interruptions;
        }
        if self.phase == Installation::Interruptions {
            regs.w64(0xb8, anchors[6].pa | 0x803);
            regs.w32(0x18, 1 << 24);
            wait(|| regs.r32(0x1c) & (1 << 24) != 0)?;
            regs.w64(0x88, 0);
            if regs.r64(0x88) != 0 {
                return Err(Error::Quarantine);
            }
            regs.w64(0x90, anchors[7].pa);
            regs.w32(0x18, RUN);
            self.phase = Installation::File;
        }
        if self.phase == Installation::File {
            wait(|| regs.r32(0x1c) & 0xc7800000 == 0xc7000000)?;
            if regs.r64(0xb8) != anchors[6].pa | 0x803 {
                return Err(Error::Route);
            }
            self.phase = Installation::Active;
        }
        Ok(())
    }
    pub fn start(&mut self, p: &Pages) -> Result<()> {
        self.activer()?;
        if self.phase == Installation::Stationnee {
            self.anchors[1].mem.w64(16 * usize::from(SID) + 8, 0x102);
            self.anchors[1]
                .mem
                .w64(16 * usize::from(SID), self.anchors[2].pa | 1);
            barrier();
            self.phase = Installation::Active;
        }
        self.invalidate(p, &[0x11, 0xd2, 4])
    }
    fn collecter(&mut self) -> Result<()> {
        if self.en_vol {
            wait(|| self.anchors[8].mem.r32(0) == self.cookie || self.regs.r32(0x34) & 0x10 != 0)?;
            if self.regs.r32(0x34) & 0x10 != 0 || self.regs.r64(0x80) != u64::from(self.tail) {
                return Err(Error::Quarantine);
            }
            self.en_vol = false;
        }
        Ok(())
    }
    fn invalidate(&mut self, _p: &Pages, kinds: &[u64]) -> Result<()> {
        self.collecter()?;
        self.cookie = self.cookie.checked_add(1).ok_or(Error::State)?;
        self.anchors[8].mem.w32(0, 0);
        for (lo, hi) in kinds
            .iter()
            .copied()
            .map(|x| (x, 0))
            .chain(core::iter::once((
                (u64::from(self.cookie) << 32) | 0x25,
                self.anchors[8].pa,
            )))
        {
            let at = usize::from(self.tail);
            self.anchors[7].mem.w64(at, lo);
            self.anchors[7].mem.w64(at + 8, hi);
            self.tail = (self.tail + 16) % 4096;
        }
        barrier();
        self.en_vol = true;
        self.regs.w64(0x88, u64::from(self.tail));
        self.collecter()
    }
    pub fn map(&mut self, p: &Pages, n: usize, pa: u64, rights: u32) -> Result<()> {
        if n >= 7 || pa % 4096 != 0 || rights > 3 {
            return Err(Error::Geometry);
        }
        let offset = 8 * ((super::addr(n) >> 12) & 511) as usize;
        self.anchors[5].mem.w64(offset, pa | u64::from(rights));
        self.invalidate(p, &[0xd2])
    }
    pub fn route(&mut self, p: &Pages, d: Device, on: bool) -> Result<()> {
        let (cap, table, _) = d.msix.ok_or(Error::Route)?;
        let ctrl = d.cfg.get(cap + 2, 2);
        if ctrl & 0x7ff != 1 {
            return Err(Error::Route);
        }
        if on {
            d.cfg.intx(false);
            d.cfg.put(cap + 2, 2, ctrl | 0xc000);
            d.common.w16(16, 0xffff); // aucune interruption de configuration
            d.common.w16(22, 0);
            d.common.w16(26, 0);
            if d.common.r16(26) != 0 || d.common.r16(16) != 0xffff {
                return Err(Error::Route);
            }
            // Un seul handle remappable attribué par le noyau, SID exact et destination CPU0.
            self.anchors[6]
                .mem
                .w64(ENTRY * 16 + 8, (1 << 18) | u64::from(SID));
            self.anchors[6]
                .mem
                .w64(ENTRY * 16, (u64::from(VECTOR) << 16) | 1);
            self.invalidate(p, &[4])?;
            table.w32(12, 1);
            table.w32(0, 0xfee0_0010 | ((ENTRY as u32) << 5));
            table.w32(4, 0);
            table.w32(8, 0);
            table.w32(16 + 12, 1);
            table.w32(12, 0);
            self.apic.w32(0xf0, (self.apic.r32(0xf0) & !0xff) | 0x1ff);
            d.cfg.put(cap + 2, 2, (ctrl | 0x8000) & !0x4000);
        } else {
            table.w32(12, 1);
            d.cfg.put(cap + 2, 2, ctrl | 0x4000);
            self.anchors[6].mem.w64(ENTRY * 16, 0);
            self.invalidate(p, &[4])?;
        }
        Ok(())
    }
    pub fn clear_pending(&self, d: Device) -> Result<()> {
        let (_, _, pba) = d.msix.ok_or(Error::Route)?;
        if pba.r32(0) & 3 != 0 {
            return Err(Error::Quarantine);
        }
        // Les IRQ locales sont permises pendant l'attente, la porte conserve l'ancien propriétaire.
        wait(|| self.apic.r32(0x220) & (1 << 17) == 0 && self.apic.r32(0x120) & (1 << 17) == 0)
    }
    pub fn faults(&self, _: &Pages) -> Result<()> {
        if self.regs.r32(0x34) & 0x13 != 0 || self.regs.r64(self.record + 8) & (1 << 63) != 0 {
            Err(Error::Fault)
        } else {
            Ok(())
        }
    }
    pub fn retirer_slot(&mut self, p: &Pages, n: usize) -> Result<()> {
        self.map(p, n, 0, 0)
    }
    pub fn stop(&mut self, p: &Pages) -> Result<()> {
        self.activer()?;
        self.collecter()?;
        self.anchors[1].mem.w64(16 * usize::from(SID), 0);
        self.invalidate(p, &[0x11, 0xd2])?;
        // Un échec d'accès attendu appartient à cette incarnation. Ne jamais
        // acquitter silencieusement le SID d'un autre périphérique, une faute
        // de file ou la perte d'un enregistrement. Q35 expose un seul FRCD.
        let fsts = self.regs.r32(0x34);
        let fr = self.regs.r64(self.record + 8);
        if fsts & !2 != 0 {
            return Err(Error::Quarantine);
        }
        if fr >> 63 != 0 {
            if fr as u16 != SID {
                return Err(Error::Quarantine);
            }
            self.regs.w32(self.record + 12, 1 << 31);
        }
        if self.faults(p).is_err() {
            return Err(Error::Quarantine);
        }
        // Entre deux incarnations, la traduction et le remappage demeurent ACTIVÉS :
        // aucune fenêtre de bypass pour un autre SID. Les neuf cadres machine ne sont
        // jamais remis au Buddy. Toutes les pages propres à la session le sont.
        if self.regs.r32(0x1c) & RUN != RUN {
            return Err(Error::Quarantine);
        }
        self.phase = Installation::Stationnee;
        Ok(())
    }
    pub fn garer(self) {
        assert!(self.phase == Installation::Stationnee && !self.en_vol);
        let mut parked = PARKED.lock();
        assert!(parked.is_none());
        *parked = Some(self);
    }
}
impl Domain {
    pub fn witnesses(&self, out: &mut [u64; 24]) {
        for (i, f) in self.anchors.iter().enumerate() {
            out[i] = f.pa
        }
    }
    pub fn hold(&self, d: Device) -> Result<()> {
        let (_, t, _) = d.msix.ok_or(Error::Route)?;
        t.w32(12, 1);
        if t.r32(12) & 1 != 1 {
            return Err(Error::Route);
        }
        Ok(())
    }
    pub fn resume_line(&self, d: Device) -> Result<()> {
        let (_, table, _) = d.msix.ok_or(Error::Route)?;
        table.w32(12, 0);
        Ok(())
    }
    pub fn pending_on_mask(&self, d: Device) -> bool {
        d.msix.is_some_and(|(_, _, p)| p.r32(0) & 1 != 0)
    }
}
