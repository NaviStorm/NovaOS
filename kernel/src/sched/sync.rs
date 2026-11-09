//! Mutex bloquant pour threads noyau (BSP). Les gestionnaires IRQ ne le prennent pas.
use super::{Event, with};
use crate::hal::interrupts::Guard;
use core::{
    cell::{Cell, UnsafeCell},
    marker::PhantomData,
    ops::{Deref, DerefMut},
};

pub struct Mutex<T> {
    owner: Cell<Option<super::Id>>,
    changed: Event,
    data: UnsafeCell<T>,
}
// Sûreté : CPU0 seul ; test, publication du dormeur et transfert à l’ordonnanceur
// sont atomiques vis-à-vis des IRQ. Seul le propriétaire accède à data.
unsafe impl<T: Send> Sync for Mutex<T> {}
pub struct Locked<'a, T> {
    mutex: &'a Mutex<T>,
    _local: PhantomData<*mut ()>,
}
impl<T> Mutex<T> {
    pub const fn new(value: T) -> Self {
        Self {
            owner: Cell::new(None),
            changed: Event::new(),
            data: UnsafeCell::new(value),
        }
    }
    pub fn lock(&self) -> Locked<'_, T> {
        let irq = Guard::new();
        assert!(
            irq.was_enabled(),
            "mutex en interruption ou section critique"
        );
        let id = with(|s| s.thread(s.current).id);
        while let Some(owner) = self.owner.get() {
            assert_ne!(owner, id, "mutex récursif");
            self.changed.wait(&irq);
        }
        self.owner.set(Some(id));
        Locked {
            mutex: self,
            _local: PhantomData,
        }
    }
}
impl<T> Deref for Locked<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        // Sûreté : la garde vivante conserve la propriété exclusive du mutex.
        unsafe { &*self.mutex.data.get() }
    }
}
impl<T> DerefMut for Locked<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        // Sûreté : emprunt unique de la garde du propriétaire.
        unsafe { &mut *self.mutex.data.get() }
    }
}
impl<T> Drop for Locked<'_, T> {
    fn drop(&mut self) {
        let _irq = Guard::new();
        self.mutex.owner.set(None);
        self.mutex.changed.notify_one();
    }
}
