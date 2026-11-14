//! Slabs superviseur dans l'arène noyau ou sur les cadres privés du pool.
//! Les bitmaps résident dans Core. Une suppression logique ne démape jamais en IRQ.
use crate::abi::{LIMIT, NO_MEMORY};
use crate::{
    hal::memory::{Access, VirtualAddress},
    heap, memory,
};
use core::{marker::PhantomData, mem::size_of};

#[derive(Clone, Copy, Default)]
pub(super) struct Bits([u64; 4]);
impl Bits {
    pub fn has(self, i: usize) -> bool {
        self.0[i / 64] & (1 << (i % 64)) != 0
    }
    pub fn set(&mut self, i: usize) {
        self.0[i / 64] |= 1 << (i % 64);
    }
    pub fn clear(&mut self, i: usize) {
        self.0[i / 64] &= !(1 << (i % 64));
    }
    pub fn indices(self) -> impl Iterator<Item = usize> {
        self.0.into_iter().enumerate().flat_map(|(word, mut bits)| {
            core::iter::from_fn(move || {
                if bits == 0 {
                    return None;
                }
                let i = bits.trailing_zeros() as usize;
                bits &= bits - 1;
                Some(word * 64 + i)
            })
        })
    }
}

pub(super) struct Pool<
    T: Copy,
    const N: usize,
    const BASE: usize,
    const PER: usize,
    const DIRECT: usize = 0,
> {
    used: Bits,
    mapped: u64,
    dirty: u64,
    private: [Option<memory::KernelFrame>; DIRECT],
    marker: PhantomData<T>,
}
impl<T: Copy, const N: usize, const BASE: usize, const PER: usize, const DIRECT: usize>
    Pool<T, N, BASE, PER, DIRECT>
{
    pub const MAX_PAGES: usize = N.div_ceil(PER);
    pub const FIRST: usize = BASE;
    pub const fn new() -> Self {
        assert!(N <= 256 && PER > 0 && PER * size_of::<T>() <= 4096 && N.div_ceil(PER) <= 64);
        assert!(DIRECT == 0 || DIRECT == Self::MAX_PAGES);
        Self {
            used: Bits([0; 4]),
            mapped: 0,
            dirty: 0,
            private: [const { None }; DIRECT],
            marker: PhantomData,
        }
    }
    fn address(page: usize) -> usize {
        0xffff_f030_0000_0000 + (BASE + page) * 4096
    }
    fn pointer(&self, i: usize) -> *mut T {
        let page = if DIRECT == 0 {
            Self::address(i / PER)
        } else {
            self.private[i / PER]
                .as_ref()
                .expect("cadre IPC absent")
                .alias()
        };
        (page + (i % PER) * size_of::<T>()) as *mut T
    }
    pub fn ids(&self) -> impl Iterator<Item = usize> + use<T, N, BASE, PER, DIRECT> {
        self.used.indices()
    }
    pub fn vacant(&self) -> Result<usize, u64> {
        (0..N).find(|&i| !self.used.has(i)).ok_or(LIMIT)
    }
    pub fn has(&self, i: usize) -> bool {
        i < N && self.used.has(i)
    }
    pub fn get(&self, i: usize) -> &T {
        assert!(self.has(i));
        // Sûreté : le bitmap implique un slot initialisé et sa page mappée.
        // La garde de l'ordonnanceur protège Core sur chaque CPU. La référence
        // est liée au pool et ne survit pas à la transaction.
        unsafe { &*self.pointer(i) }
    }
    pub fn get_mut(&mut self, i: usize) -> &mut T {
        assert!(self.has(i));
        // Sûreté : même invariant, emprunt exclusif du pool.
        unsafe { &mut *self.pointer(i) }
    }
    pub fn insert(&mut self, i: usize, value: T) -> Result<(), u64> {
        assert!(i < N && !self.has(i));
        let page = i / PER;
        if self.mapped & (1 << page) == 0 {
            match heap::with_pages(|m| {
                if DIRECT == 0 {
                    m.allocate_page(VirtualAddress(Self::address(page)), Access::ReadWrite)
                } else {
                    self.private[page] = Some(m.take_kernel_frame()?);
                    Ok(())
                }
            }) {
                Ok(()) => (),
                Err(memory::Error::Exhausted) => return Err(NO_MEMORY),
                Err(e) => panic!("slab IPC : {e:?}"),
            }
            #[cfg(feature = "ipc-smp-lab")]
            if BASE == super::Waits::FIRST {
                super::multicore::page_attente(true);
            }
            self.mapped |= 1 << page;
        }
        // Sûreté : slot libre dans une page RW privée, aligné pour T, publication après écriture.
        unsafe { self.pointer(i).write(value) };
        self.used.set(i);
        Ok(())
    }
    pub fn remove(&mut self, i: usize) {
        assert!(self.has(i));
        self.used.clear(i);
        self.dirty |= 1 << (i / PER);
        // Sûreté : plus aucun emprunt ou lecteur du slot retiré ; effacement
        // brut de T:Copy avant toute réattribution, aucune lecture typée à vide.
        unsafe { self.pointer(i).write_bytes(0, 1) };
    }
    pub fn pages(&self) -> u32 {
        self.mapped.count_ones()
    }
    pub fn collect(&mut self) {
        let mut mapped = core::mem::take(&mut self.dirty);
        while mapped != 0 {
            let p = mapped.trailing_zeros() as usize;
            mapped &= mapped - 1;
            if (p * PER..((p + 1) * PER).min(N)).any(|i| self.used.has(i)) {
                continue;
            }
            // Sûreté : page du pool vide, sans référence survivant à la transaction.
            heap::with_pages(|m| {
                if DIRECT == 0 {
                    unsafe { m.release_page(VirtualAddress(Self::address(p))) }
                } else {
                    m.release_kernel_frame(self.private[p].take().expect("cadre IPC retiré"));
                    Ok(())
                }
            })
            .expect("collecte IPC");
            #[cfg(feature = "ipc-smp-lab")]
            if BASE == super::Waits::FIRST {
                super::multicore::page_attente(false);
            }
            self.mapped &= !(1 << p);
        }
    }
}
