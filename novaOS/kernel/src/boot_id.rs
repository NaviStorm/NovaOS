//! Époque opaque de ce démarrage. Publication unique par le BSP avant les AP.
use core::sync::atomic::{AtomicU8, AtomicU64, Ordering};

const EMPTY: u8 = 0;
const WRITING: u8 = 1;
const READY: u8 = 2;
const UNAVAILABLE: u8 = 3;

struct BootEpoch {
    state: AtomicU8,
    words: [AtomicU64; 2],
}

impl BootEpoch {
    const fn new() -> Self {
        Self {
            state: AtomicU8::new(EMPTY),
            words: [const { AtomicU64::new(0) }; 2],
        }
    }

    fn publish(&self, sample: Option<[u64; 2]>) -> bool {
        if self
            .state
            .compare_exchange(EMPTY, WRITING, Ordering::Relaxed, Ordering::Relaxed)
            .is_err()
        {
            return false;
        }
        if let Some([lo, hi]) = sample {
            self.words[0].store(lo, Ordering::Relaxed);
            self.words[1].store(hi, Ordering::Relaxed);
            self.state.store(READY, Ordering::Release);
        } else {
            self.state.store(UNAVAILABLE, Ordering::Release);
        }
        true
    }

    fn get(&self) -> Option<[u8; 16]> {
        if self.state.load(Ordering::Acquire) != READY {
            return None;
        }
        let mut id = [0; 16];
        id[..8].copy_from_slice(&self.words[0].load(Ordering::Relaxed).to_le_bytes());
        id[8..].copy_from_slice(&self.words[1].load(Ordering::Relaxed).to_le_bytes());
        Some(id)
    }
}

static EPOCH: BootEpoch = BootEpoch::new();

pub(crate) fn current() -> Option<[u8; 16]> {
    EPOCH.get()
}

#[cfg(not(test))]
pub(crate) fn init() {
    let sample = source();
    assert!(EPOCH.publish(sample.map(|(words, _)| words)));
    if crate::boot::scenario().starts_with(b"instances-") {
        let name = sample.map_or("indisponible", |(_, name)| name);
        crate::hal::log(format_args!("BOOTID source={name}"));
    }
}

fn read_pair(mut read: impl FnMut() -> Option<u64>) -> Option<[u64; 2]> {
    let mut words = [0; 2];
    for word in &mut words {
        *word = (0..10).find_map(|_| read())?;
    }
    // Rejeter également les pannes évidentes de générateur bloqué.
    (words[0] != words[1]).then_some(words)
}

#[cfg(target_arch = "x86_64")]
fn source() -> Option<([u64; 2], &'static str)> {
    use core::arch::x86_64::{__cpuid, _rdrand64_step};
    if __cpuid(1).ecx & (1 << 30) == 0 {
        return None;
    }
    let words = read_pair(|| {
        let mut word = 0;
        // Sûreté : CPUID.01H:ECX.RDRAND a été vérifié sur le BSP.
        (unsafe { _rdrand64_step(&mut word) } == 1).then_some(word)
    })?;
    Some((words, "RDRAND"))
}

#[cfg(target_arch = "aarch64")]
fn source() -> Option<([u64; 2], &'static str)> {
    let features: u64;
    // Sûreté : lecture d'un registre architectural accessible en EL1.
    unsafe {
        core::arch::asm!("mrs {}, id_aa64isar0_el1", out(reg) features, options(nomem, nostack, preserves_flags));
    }
    if features >> 60 == 1 {
        let words = read_pair(|| {
            let value: u64;
            let ok: u64;
            // RNDR : Z vaut 0 en cas de succès. L'encodage nommé par numéro
            // permet de conserver la cible minimale ARMv8 du reste du noyau.
            unsafe {
                core::arch::asm!("mrs {v}, S3_3_C2_C4_0", "cset {ok}, ne",
                    v = out(reg) value, ok = out(reg) ok, options(nomem, nostack));
            }
            (ok != 0).then_some(value)
        })?;
        return Some((words, "RNDR"));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panne_transitoire_et_panne_bornee() {
        let mut attempts = 0;
        assert_eq!(
            read_pair(|| {
                attempts += 1;
                None
            }),
            None
        );
        assert_eq!(attempts, 10);
        let mut draws = [None, Some(4), None, Some(8)].into_iter();
        assert_eq!(read_pair(|| draws.next().unwrap()), Some([4, 8]));
        let mut first = Some(1);
        assert_eq!(read_pair(|| first.take()), None);
        assert_eq!(read_pair(|| Some(u64::MAX)), None);
    }

    #[test]
    fn publication_entiere_et_unique() {
        let epoch = BootEpoch::new();
        assert_eq!(epoch.get(), None);
        let value = 0x1234567890abcdef_1122334455667788u128;
        assert!(epoch.publish(Some([value as u64, (value >> 64) as u64])));
        assert_eq!(epoch.get(), Some(value.to_le_bytes()));
        assert!(!epoch.publish(None));
        assert!(!epoch.publish(Some([10, 20])));
        assert_eq!(epoch.get(), Some(value.to_le_bytes()));
    }

    #[test]
    fn absence_definitive_pendant_ce_demarrage() {
        let epoch = BootEpoch::new();
        assert!(epoch.publish(None));
        assert!(!epoch.publish(Some([1, 2])));
        assert!(epoch.get().is_none());
    }
}
