use crate::{Error, Result, abi, call};
use core::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

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
    .map(|key| JoinHandle(AtomicU64::new(key)))
}

#[must_use = "une jonction rapporte la valeur du thread, le drop le détache"]
pub struct JoinHandle(AtomicU64);
impl JoinHandle {
    /// Génération du droit, ou zéro après sa consommation.
    pub fn raw(&self) -> u64 {
        self.0.load(Ordering::Relaxed)
    }
    fn key(&self) -> Result<u64> {
        match self.raw() {
            0 => Err(Error(abi::DENIED)),
            key => Ok(key),
        }
    }
    pub fn cancel(&self) -> Result<()> {
        call(abi::THREAD_CANCEL, self.key()?, 0, 0).map(|_| ())
    }
    /// Emprunte le handle. Seule une collecte réussie consomme le droit.
    /// Un membre annulé peut transmettre le handle à un autre membre du processus.
    pub fn join(&self) -> Result<usize> {
        let result = call(abi::THREAD_JOIN, self.key()?, 0, 0)?;
        self.0.store(0, Ordering::Relaxed);
        Ok(result as usize)
    }
    /// Un détachement réussi abandonne définitivement le résultat.
    pub fn detach(&self) -> Result<()> {
        call(abi::THREAD_DETACH, self.key()?, 0, 0)?;
        self.0.store(0, Ordering::Relaxed);
        Ok(())
    }
}
impl Drop for JoinHandle {
    fn drop(&mut self) {
        let key = *self.0.get_mut();
        if key != 0 {
            let _ = call(abi::THREAD_DETACH, key, 0, 0);
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
