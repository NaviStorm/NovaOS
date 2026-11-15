use crate::{Error, Result, abi, call};
use core::sync::atomic::{AtomicUsize, Ordering};

/// Point d’entrée indépendant de l’architecture. L’argument est un scalaire.
pub trait Task {
    fn run(arg: usize) -> usize;
}

extern "C" fn start<T: Task>(arg: usize) -> ! {
    exit(T::run(arg))
}

pub fn exit(result: usize) -> ! {
    let _ = call(abi::THREAD_EXIT, result as u64, 0, 0);
    panic!("THREAD_EXIT a rendu la main")
}

pub fn spawn<T: Task>(arg: usize, stack_pages: usize) -> Result<JoinHandle> {
    call(
        abi::THREAD_CREATE,
        start::<T> as *const () as u64,
        arg as u64,
        stack_pages as u64,
    )
    .map(JoinHandle)
}

#[must_use = "une jonction rapporte la valeur du thread, le drop le détache"]
pub struct JoinHandle(u64);
impl JoinHandle {
    pub fn raw(&self) -> u64 {
        self.0
    }
    pub fn cancel(&self) -> Result<()> {
        call(abi::THREAD_CANCEL, self.0, 0, 0).map(|_| ())
    }
    pub fn join(mut self) -> Result<usize> {
        let result = call(abi::THREAD_JOIN, self.0, 0, 0)?;
        self.0 = 0;
        Ok(result as usize)
    }
    pub fn detach(mut self) -> Result<()> {
        call(abi::THREAD_DETACH, self.0, 0, 0)?;
        self.0 = 0;
        Ok(())
    }
}
impl Drop for JoinHandle {
    fn drop(&mut self) {
        if self.0 != 0 {
            let _ = call(abi::THREAD_DETACH, self.0, 0, 0);
        }
    }
}

pub fn id() -> Result<u64> {
    call(abi::THREAD_ID, 0, 0, 0)
}
pub fn yield_now() {
    let _ = call(abi::THREAD_YIELD, 0, 0, 0);
}
pub fn cancelled() -> Result<bool> {
    call(abi::THREAD_CANCELLED, 0, 0, 0).map(|v| v != 0)
}

fn slot(index: usize) -> Result<*mut usize> {
    if index >= 4096 / size_of::<usize>() {
        return Err(Error(abi::BAD_ARGUMENT));
    }
    let base = call(abi::THREAD_TLS, 0, 0, 0)? as *mut usize;
    Ok(base.wrapping_add(index))
}
pub fn tls_get(index: usize) -> Result<usize> {
    // Sûreté : la page alignée appartient au thread courant jusqu’à sa sortie.
    Ok(unsafe { AtomicUsize::from_ptr(slot(index)?) }.load(Ordering::Relaxed))
}
pub fn tls_set(index: usize, value: usize) -> Result<()> {
    // Sûreté : chaque emplacement est un mot aligné et accessible en écriture.
    unsafe { AtomicUsize::from_ptr(slot(index)?) }.store(value, Ordering::Relaxed);
    Ok(())
}
