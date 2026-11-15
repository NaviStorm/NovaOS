use crate::{Error, Result, abi, call};
use core::{
    cell::UnsafeCell,
    marker::PhantomData,
    ops::{Deref, DerefMut},
    sync::atomic::{AtomicU64, Ordering},
};

/// Compare et endort en une seule opération. Zéro sonde, MAX attend sans échéance.
pub fn wait(word: &AtomicU64, expected: u64, timeout_ticks: u64) -> Result<()> {
    call(
        abi::THREAD_WAIT,
        word.as_ptr() as u64,
        expected,
        timeout_ticks,
    )
    .map(|_| ())
}
pub fn notify(word: &AtomicU64, count: usize) -> Result<usize> {
    call(abi::THREAD_NOTIFY, word.as_ptr() as u64, count as u64, 0).map(|n| n as usize)
}

pub struct Mutex<T> {
    locked: AtomicU64,
    value: UnsafeCell<T>,
}
// Sûreté : seuls compare_exchange(Acquire) et la garde exclusive autorisent
// l’accès à la valeur. Drop publie les écritures avec Release avant de notifier.
unsafe impl<T: Send> Sync for Mutex<T> {}
pub struct MutexGuard<'a, T> {
    mutex: &'a Mutex<T>,
    exclusive: PhantomData<&'a mut T>,
}

impl<T> Mutex<T> {
    pub const fn new(value: T) -> Self {
        Self {
            locked: AtomicU64::new(0),
            value: UnsafeCell::new(value),
        }
    }
    pub fn lock(&self) -> Result<MutexGuard<'_, T>> {
        while self
            .locked
            .compare_exchange(0, 1, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            match wait(&self.locked, 1, u64::MAX) {
                Ok(()) | Err(Error(abi::WOULD_BLOCK)) => (),
                Err(e) => return Err(e),
            }
        }
        Ok(MutexGuard {
            mutex: self,
            exclusive: PhantomData,
        })
    }
}
impl<T> Deref for MutexGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        // Sûreté : cette garde conserve la possession du verrou.
        unsafe { &*self.mutex.value.get() }
    }
}
impl<T> DerefMut for MutexGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        // Sûreté : la garde est unique et empruntée exclusivement.
        unsafe { &mut *self.mutex.value.get() }
    }
}
impl<T> Drop for MutexGuard<'_, T> {
    fn drop(&mut self) {
        self.mutex.locked.store(0, Ordering::Release);
        let _ = notify(&self.mutex.locked, usize::MAX);
    }
}
