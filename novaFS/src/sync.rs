//! Sur cible nue un panic termine le processus. Les gardes ne survivent pas à sa mort.
pub use alloc::sync::Arc;
pub use core::sync::atomic;
#[cfg(not(target_os = "none"))]
pub use std::sync::{Mutex, MutexGuard, RwLock, TryLockError};

#[cfg(target_os = "none")]
pub use native::*;
#[cfg(target_os = "none")]
mod native {
    use core::convert::Infallible;
    pub use spin::MutexGuard;
    pub struct Poison<T>(T);
    impl<T> core::fmt::Debug for Poison<T> {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            f.write_str("verrou interrompu")
        }
    }
    impl<T> Poison<T> {
        pub fn into_inner(self) -> T {
            self.0
        }
    }
    #[derive(Debug)]
    pub enum TryLockError {
        WouldBlock,
        Poisoned(Infallible),
    }
    #[derive(Default, Debug)]
    pub struct Mutex<T>(spin::Mutex<T>);
    impl<T> Mutex<T> {
        pub const fn new(v: T) -> Self {
            Self(spin::Mutex::new(v))
        }
        pub fn lock(&self) -> Result<MutexGuard<'_, T>, Poison<MutexGuard<'_, T>>> {
            Ok(self.0.lock())
        }
        pub fn try_lock(&self) -> Result<MutexGuard<'_, T>, TryLockError> {
            self.0.try_lock().ok_or(TryLockError::WouldBlock)
        }
        pub fn get_mut(&mut self) -> Result<&mut T, Poison<&mut T>> {
            Ok(self.0.get_mut())
        }
    }
    pub struct RwLock<T>(spin::RwLock<T>);
    impl<T> RwLock<T> {
        pub fn new(v: T) -> Self {
            Self(spin::RwLock::new(v))
        }
        pub fn read(&self) -> Result<spin::RwLockReadGuard<'_, T>, Infallible> {
            Ok(self.0.read())
        }
        pub fn write(&self) -> Result<spin::RwLockWriteGuard<'_, T>, Infallible> {
            Ok(self.0.write())
        }
        pub fn try_read(&self) -> Result<spin::RwLockReadGuard<'_, T>, TryLockError> {
            self.0.try_read().ok_or(TryLockError::WouldBlock)
        }
        pub fn get_mut(&mut self) -> Result<&mut T, Poison<&mut T>> {
            Ok(self.0.get_mut())
        }
        pub fn into_inner(self) -> Result<T, Infallible> {
            Ok(self.0.into_inner())
        }
    }
}
