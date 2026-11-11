use super::{Error, Result};
use crate::memory;
use core::{
    mem::{align_of, size_of},
    sync::atomic::{Ordering, fence},
};

pub(super) const PAGE: usize = 4096;
pub(super) const WINDOW: u64 = 0xffff_c900_0000_0000;

/// Région bornée, toujours superviseur. Les constructeurs fixent l'origine et la durée de vie.
#[derive(Clone, Copy)]
pub(super) struct Region {
    base: usize,
    len: usize,
}
impl Region {
    pub fn map(pa: u64, slot: usize, bytes: usize) -> Result<Self> {
        if (pa as usize | bytes) % PAGE != 0
            || bytes == 0
            || slot >= 4096
            || bytes / PAGE > 4096 - slot
        {
            return Err(Error::Geometry);
        }
        let va = WINDOW + (slot * PAGE) as u64;
        for off in (0..bytes).step_by(PAGE) {
            memory::map_io(
                pa.checked_add(off as u64).ok_or(Error::Geometry)?,
                va + off as u64,
            )
            .map_err(|_| Error::Memory)?;
        }
        Ok(Self {
            base: va as usize,
            len: bytes,
        })
    }
    pub fn sub(self, offset: usize, len: usize) -> Result<Self> {
        if len == 0 || offset.checked_add(len).is_none_or(|end| end > self.len) {
            return Err(Error::Geometry);
        }
        Ok(Self {
            base: self.base + offset,
            len,
        })
    }
    fn ptr<T>(&self, off: usize) -> *mut T {
        assert!(
            off <= self.len
                && size_of::<T>() <= self.len - off
                && (self.base + off) % align_of::<T>() == 0
        );
        (self.base + off) as *mut T
    }
    pub fn r8(self, o: usize) -> u8 {
        self.read(o)
    }
    pub fn r16(self, o: usize) -> u16 {
        self.read(o)
    }
    pub fn r32(self, o: usize) -> u32 {
        self.read(o)
    }
    pub fn r64(self, o: usize) -> u64 {
        self.read(o)
    }
    pub fn w8(self, o: usize, v: u8) {
        self.write(o, v)
    }
    pub fn w16(self, o: usize, v: u16) {
        self.write(o, v)
    }
    pub fn w32(self, o: usize, v: u32) {
        self.write(o, v)
    }
    pub fn w64(self, o: usize, v: u64) {
        self.write(o, v)
    }
    fn read<T: Copy>(self, o: usize) -> T {
        // Sûreté : type entier, alignement et étendue contrôlés. Les registres sont
        // mappés Device/UC ou appartiennent à nos cadres DMA encore épinglés.
        unsafe { self.ptr::<T>(o).read_volatile() }
    }
    fn write<T: Copy>(self, o: usize, v: T) {
        // Sûreté : même vérification de plage. Aucun emprunt Rust de la mémoire DMA.
        unsafe { self.ptr::<T>(o).write_volatile(v) }
    }
    pub fn bytes(self, off: usize, out: &mut [u8]) {
        for (i, v) in out.iter_mut().enumerate() {
            *v = self.r8(off + i)
        }
    }
    pub fn put(self, off: usize, input: &[u8]) {
        for (i, v) in input.iter().enumerate() {
            self.w8(off + i, *v)
        }
    }
    pub fn base(self) -> usize {
        self.base
    }
}
#[derive(Clone, Copy)]
pub(super) struct Frame {
    pub pa: u64,
    pub mem: Region,
}
pub(super) const EMPTY: Frame = Frame {
    pa: 0,
    mem: Region { base: 0, len: 0 },
};
impl Frame {
    pub fn allocate() -> Result<Self> {
        memory::pin_dma()
            .map(|(pa, base)| Frame {
                pa,
                mem: Region { base, len: PAGE },
            })
            .map_err(|_| Error::Memory)
    }
}
pub(super) struct Pages(pub [Frame; 24]);
impl Pages {
    pub fn new() -> Result<Self> {
        let mut pages = Self([EMPTY; 24]);
        for i in 0..pages.0.len() {
            if !(16..23).contains(&i) && !(cfg!(target_arch = "aarch64") && i < 9) {
                continue;
            }
            match Frame::allocate() {
                Ok(f) => pages.0[i] = f,
                Err(_) => {
                    // Sûreté : échec avant la première publication au matériel.
                    unsafe { pages.release() };
                    return Err(Error::Memory);
                }
            }
        }
        Ok(pages)
    }
    /// # Safety
    /// Toutes les traductions et lectures/écritures en vol doivent avoir cessé.
    pub unsafe fn release(self) {
        for f in self.0.into_iter().filter(|f| f.pa != 0) {
            // Sûreté : propriété unique du lot, quiescence démontrée par l'appelant.
            unsafe { memory::release_dma(f.pa) }
        }
    }
}
pub(super) fn barrier() {
    fence(Ordering::SeqCst);
    #[cfg(target_arch = "aarch64")]
    // Sûreté : barrière système entre la RAM cohérente et le transport Device sur virt.
    unsafe {
        core::arch::asm!("dsb sy", options(nostack, preserves_flags))
    }
}
pub(super) fn wait(mut ready: impl FnMut() -> bool) -> Result<()> {
    let begin = crate::hal::timer::counter();
    // Limite de secours, indépendante d'une IRQ qu'un arrêt peut masquer.
    for _ in 0..4_000_000 {
        if ready() {
            barrier();
            return Ok(());
        }
        if crate::hal::timer::counter().wrapping_sub(begin) > 2_000_000_000 {
            break;
        }
        core::hint::spin_loop()
    }
    Err(Error::Timeout)
}
