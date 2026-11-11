use super::{
    Error, Result,
    io::{PAGE, Region},
};

#[derive(Clone, Copy)]
pub(super) struct Config {
    #[cfg(target_arch = "x86_64")]
    bdf: u8,
    #[cfg(target_arch = "aarch64")]
    mem: Region,
}
impl Config {
    pub fn open(bdf: u8) -> Result<Self> {
        #[cfg(target_arch = "x86_64")]
        {
            Ok(Self { bdf })
        }
        #[cfg(target_arch = "aarch64")]
        {
            // Profil virt approuvé avec ECAM haut, un bus racine. Une page par fonction.
            let mem = Region::map(
                0x0040_1000_0000 + (bdf as u64) * 4096,
                usize::from(bdf),
                PAGE,
            )?;
            Ok(Self { mem })
        }
    }
    pub fn get(self, off: usize, bytes: usize) -> u32 {
        #[cfg(target_arch = "x86_64")]
        {
            self.port(off, bytes, None)
        }
        #[cfg(target_arch = "aarch64")]
        {
            match bytes {
                1 => self.mem.r8(off) as u32,
                2 => self.mem.r16(off) as u32,
                4 => self.mem.r32(off),
                _ => unreachable!(),
            }
        }
    }
    pub fn put(self, off: usize, bytes: usize, value: u32) {
        #[cfg(target_arch = "x86_64")]
        {
            self.port(off, bytes, Some(value));
        }
        #[cfg(target_arch = "aarch64")]
        {
            match bytes {
                1 => self.mem.w8(off, value as u8),
                2 => self.mem.w16(off, value as u16),
                4 => self.mem.w32(off, value),
                _ => unreachable!(),
            }
        }
    }
    #[cfg(target_arch = "x86_64")]
    fn port(self, off: usize, bytes: usize, v: Option<u32>) -> u32 {
        use core::arch::asm;
        assert!(off < 256 && matches!(bytes, 1 | 2 | 4) && off % bytes == 0);
        let _guard = crate::hal::interrupts::Guard::new();
        let address = 0x8000_0000 | ((self.bdf as u32) << 8) | (off as u32 & !3);
        let port = 0xcfcu16 + (off as u16 & 3);
        // Sûreté : CPL0 uniquement. L'adresse et la donnée du mécanisme PCI #1
        // sont sérialisées sur le BSP. Les écritures sont réservées au transport.
        unsafe {
            asm!("out dx,eax",in("dx")0xcf8u16,in("eax")address,options(nostack,preserves_flags));
            match (v, bytes) {
                (Some(x), 1) => {
                    asm!("out dx,al",in("dx")port,in("al")x as u8,options(nostack,preserves_flags))
                }
                (Some(x), 2) => {
                    asm!("out dx,ax",in("dx")port,in("ax")x as u16,options(nostack,preserves_flags))
                }
                (Some(x), 4) => {
                    asm!("out dx,eax",in("dx")port,in("eax")x,options(nostack,preserves_flags))
                }
                (None, 1) => {
                    let x: u8;
                    asm!("in al,dx",in("dx")port,out("al")x,options(nostack,preserves_flags));
                    return x.into();
                }
                (None, 2) => {
                    let x: u16;
                    asm!("in ax,dx",in("dx")port,out("ax")x,options(nostack,preserves_flags));
                    return x.into();
                }
                (None, 4) => {
                    let x: u32;
                    asm!("in eax,dx",in("dx")port,out("eax")x,options(nostack,preserves_flags));
                    return x;
                }
                _ => unreachable!(),
            }
        }
        0
    }
    fn bar(self, n: usize) -> Result<u64> {
        let offset = 0x10 + 4 * n;
        let lo = self.get(offset, 4);
        if (n == 4 && lo & 7 != 4) || (n == 1 && lo & 7 != 0) {
            return Err(Error::Geometry);
        }
        let hi = if lo & 4 != 0 {
            self.get(offset + 4, 4)
        } else {
            0
        };
        let addr = (u64::from(hi) << 32) | u64::from(lo & !15);
        if addr == 0 || addr % 4096 != 0 {
            return Err(Error::Geometry);
        }
        Ok(addr)
    }
    pub fn master(self, yes: bool) {
        // Mémoire décodée, INTx initialement masquée. Aucune restauration d'une
        // configuration MSI ou d'une adresse de file confiée au firmware.
        let prev = self.get(4, 2);
        self.put(4, 2, (prev & !7) | 2 | if yes { 4 } else { 0 });
    }
    pub fn intx(self, enabled: bool) {
        let v = self.get(4, 2);
        self.put(4, 2, if enabled { v & !0x400 } else { v | 0x400 })
    }
}
#[derive(Clone, Copy)]
pub(super) struct Device {
    pub cfg: Config,
    pub common: Region,
    pub notify: Region,
    pub isr: Region,
    pub specific: Region,
    pub stride: usize,
    pub msix: Option<(usize, Region, Region)>,
}
impl Device {
    pub fn discover(bdf: u8, kind: u16, window: usize) -> Result<Self> {
        let cfg = Config::open(bdf)?;
        if cfg.get(0, 4) != (u32::from(kind) << 16 | 0x1af4) {
            return Err(Error::Absent);
        }
        if cfg.get(6, 2) & 16 == 0 || cfg.get(0xe, 1) & 127 != 0 {
            return Err(Error::Geometry);
        }
        let bar4 = Region::map(cfg.bar(4)?, window, 4 * PAGE)?;
        let mut regions = [None; 4];
        let mut stride = 0;
        let mut msix = None;
        let mut at = cfg.get(0x34, 1) as usize;
        let mut seen = 0u64;
        while at != 0 {
            if !(0x40..=0xfc).contains(&at) || at % 4 != 0 || seen & (1u64 << (at / 4)) != 0 {
                return Err(Error::Geometry);
            }
            seen |= 1u64 << (at / 4);
            match cfg.get(at, 1) {
                9 => {
                    let typ = cfg.get(at + 3, 1) as usize;
                    if (1..=4).contains(&typ) {
                        let len = if typ == 2 { 20 } else { 16 };
                        if at + len > 256
                            || (cfg.get(at + 2, 1) as usize) < len
                            || cfg.get(at + 4, 1) != 4
                            || regions[typ - 1].is_some()
                        {
                            return Err(Error::Geometry);
                        }
                        let offset = cfg.get(at + 8, 4) as usize;
                        let size = cfg.get(at + 12, 4) as usize;
                        // Profil QEMU : quatre pages disjointes. Aucun chevauchement avec PCI_CFG ou MSI-X.
                        let page = match typ {
                            1 => 0,
                            2 => 3,
                            3 => 1,
                            _ => 2,
                        };
                        if offset != page * PAGE || size > PAGE {
                            return Err(Error::Geometry);
                        }
                        regions[typ - 1] = Some(bar4.sub(offset, size)?);
                        if typ == 2 {
                            stride = cfg.get(at + 16, 4) as usize
                        }
                    }
                }
                0x11 => {
                    if at + 12 > 256 || msix.is_some() {
                        return Err(Error::Geometry);
                    }
                    let t = cfg.get(at + 4, 4);
                    let p = cfg.get(at + 8, 4);
                    let count = (cfg.get(at + 2, 2) & 0x7ff) as usize + 1;
                    if t & 7 != 1 || p & 7 != 1 || count > 64 {
                        return Err(Error::Geometry);
                    }
                    let bar = Region::map(cfg.bar(1)?, window + 4, PAGE)?;
                    msix = Some((
                        at,
                        bar.sub((t & !7) as usize, 16 * count)?,
                        bar.sub((p & !7) as usize, 8)?,
                    ));
                }
                5 => return Err(Error::Geometry), // aucune seconde source MSI programmable
                _ => (),
            }
            at = cfg.get(at + 1, 1) as usize;
        }
        let [Some(common), Some(notify), Some(isr), Some(specific)] = regions else {
            return Err(Error::Geometry);
        };
        cfg.intx(false);
        if let Some((c, _, _)) = msix {
            cfg.put(c + 2, 2, (cfg.get(c + 2, 2) | 0x4000) & !0x8000)
        }
        cfg.master(false);
        let d = Self {
            cfg,
            common,
            notify,
            isr,
            specific,
            stride,
            msix,
        };
        d.reset()?;
        Ok(d)
    }
    pub fn reset(&self) -> Result<()> {
        self.common.w8(20, 0);
        super::io::wait(|| self.common.r8(20) == 0)
    }
    pub fn read_only(&self) -> bool {
        self.common.w32(0, 0);
        self.common.r32(4) & (1 << 5) != 0
    }
    pub fn start(&self, wanted: u64) -> Result<()> {
        self.reset()?;
        self.common.w8(20, 1);
        self.common.w8(20, 3);
        for i in 0..2 {
            self.common.w32(0, i);
            let req = (wanted >> (32 * i)) as u32;
            if self.common.r32(4) & req != req {
                return Err(Error::Feature);
            }
            self.common.w32(8, i);
            self.common.w32(12, req);
        }
        self.common.w8(20, 11);
        if self.common.r8(20) != 11 {
            return Err(Error::Feature);
        }
        Ok(())
    }
    pub fn ready(&self) -> Result<()> {
        self.cfg.master(true);
        self.common.w8(20, 15);
        if self.common.r8(20) != 15 {
            return Err(Error::Feature);
        }
        Ok(())
    }
    pub fn geometry(&self) -> Result<(u64, usize)> {
        for _ in 0..4 {
            let revision = self.common.r8(21);
            let sectors = self.specific.r64(0);
            let size = self.specific.r32(20) as usize;
            if self.common.r8(21) == revision {
                if matches!(size, 512 | 4096) && sectors > 0 && sectors % (size as u64 / 512) == 0 {
                    return Ok((sectors / (size as u64 / 512), size));
                }
                return Err(Error::Geometry);
            }
        }
        Err(Error::Geometry)
    }
}

#[cfg(target_arch = "aarch64")]
pub(super) fn exclusive_intx() -> Result<()> {
    for slot in 0..32 {
        let cfg = Config::open(slot * 8)?;
        if cfg.get(0, 2) == 0xffff {
            continue;
        }
        let count = if cfg.get(0xe, 1) & 128 != 0 { 8 } else { 1 };
        for function in 0..count {
            let f = Config::open(slot * 8 + function)?;
            if f.get(0, 2) == 0xffff {
                continue;
            }
            let pin = f.get(0x3d, 1);
            if f.get(0xe, 1) & 127 != 0 || pin > 4 {
                return Err(Error::Geometry);
            }
            if pin != 0
                && (u32::from(slot) + pin - 1) % 4 == 2
                && !(slot == 6 && function == 0 && pin == 1)
            {
                return Err(Error::Route);
            }
        }
    }
    if Config::open(48)?.get(0x3d, 1) == 1 {
        Ok(())
    } else {
        Err(Error::Route)
    }
}
