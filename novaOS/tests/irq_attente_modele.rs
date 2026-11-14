//! Banc indépendant du matériel. Compiler à côté du véritable attente_irq.rs
//! extrait de l'archive du prototype. Le pilote, ses unités et les IRQ de la VM
//! font l'objet de contrôles séparés.
use core::sync::atomic::Ordering::Relaxed;
use core::sync::atomic::{AtomicBool, AtomicU16, AtomicU64, Ordering};
#[cfg(test)]
use std::sync::Mutex;
#[cfg(test)]
static SERIE: Mutex<()> = Mutex::new(());

#[path = "attente_irq.rs"]
mod attente_irq;

#[derive(Debug, PartialEq, Eq)]
enum Error {
    State,
    Queue,
    Timeout,
}
type Result<T> = core::result::Result<T, Error>;

static IRQS: AtomicU64 = AtomicU64::new(0);
static EXPECT: AtomicU64 = AtomicU64::new(0);
static USED: AtomicU16 = AtomicU16::new(0);
static INDEX: AtomicU16 = AtomicU16::new(0);
static CLOCK: AtomicU64 = AtomicU64::new(0);
static MASQUE: AtomicBool = AtomicBool::new(false);
static BLOQUE: AtomicBool = AtomicBool::new(false);
static PARCAGES: AtomicU64 = AtomicU64::new(0);
static REVEILS: AtomicU64 = AtomicU64::new(0);
static LECTURES: AtomicU64 = AtomicU64::new(0);
static SCENARIO: AtomicU64 = AtomicU64::new(0);
const HZ: u64 = 10_000;

mod hal {
    pub(super) mod interrupts {
        use super::super::*;
        pub(crate) struct Guard(bool);
        impl Guard {
            pub fn new() -> Self {
                Self(MASQUE.swap(true, Relaxed))
            }
            pub fn was_enabled(&self) -> bool {
                !self.0
            }
        }
        impl Drop for Guard {
            fn drop(&mut self) {
                MASQUE.store(self.0, Relaxed);
            }
        }
    }
    pub(super) mod timer {
        use super::super::*;
        pub fn counter_hz() -> u64 {
            HZ
        }
        pub fn counter() -> u64 {
            if SCENARIO.load(Relaxed) == 10 && CLOCK.load(Relaxed) >= HZ + 2 {
                return HZ + 2;
            }
            let c = CLOCK.fetch_add(1, Relaxed);
            if SCENARIO.load(Relaxed) == 6 && c == HZ + 16 {
                assert!(!MASQUE.load(Relaxed));
                interruption(true);
            }
            c
        }
    }
}

struct Mem;
impl Mem {
    fn r16(&self, o: usize) -> u16 {
        assert_eq!(o, 2);
        USED.load(Relaxed)
    }
}
struct Page {
    mem: Mem,
}
struct Queue {
    index: u16,
    pages: [Page; 3],
}
impl Queue {
    fn lire_utilise(&self) -> Result<u32> {
        LECTURES.fetch_add(1, Relaxed);
        if self.index != USED.load(Relaxed) || SCENARIO.load(Relaxed) == 9 {
            return Err(Error::Queue);
        }
        Ok(4097)
    }
}
struct Transport {
    q: Queue,
}

fn interruption(publier_used: bool) {
    if publier_used {
        USED.store(INDEX.load(Relaxed), Relaxed);
    }
    // Seule la publication validée par le gestionnaire réel peut appeler
    // reveil_irq(). Ce modèle représente ce contrat sur une file à un élément.
    let ticket = EXPECT.load(Relaxed);
    if ticket != 0 && USED.load(Relaxed) == ticket as u16 {
        EXPECT.store(0, Relaxed);
        IRQS.fetch_add(1, Relaxed);
        attente_irq::reveil_irq();
    }
}

mod sched {
    use super::*;
    pub(crate) struct Event;
    impl Event {
        pub const fn new() -> Self {
            Self
        }
        pub fn notify_one(&self) -> Option<()> {
            assert!(BLOQUE.swap(false, Relaxed), "réveil sans dormeur");
            REVEILS.fetch_add(1, Relaxed);
            Some(())
        }
        pub fn wait(&self, garde: &hal::interrupts::Guard) {
            assert!(garde.was_enabled() && MASQUE.load(Relaxed));
            assert!(!BLOQUE.swap(true, Relaxed));
            PARCAGES.fetch_add(1, Relaxed);
            let scenario = SCENARIO.load(Relaxed);
            match scenario {
                // L'IRQ peut être pendante avant la commutation, ou arriver
                // sur la nouvelle pile ; dans les deux cas le dormeur existe.
                1 => {
                    // Périphérique prêt juste avant la publication du dormeur :
                    // la mémoire est visible et la notification latched sous masque.
                    assert_eq!(USED.load(Relaxed), INDEX.load(Relaxed));
                    assert_eq!(REVEILS.load(Relaxed), 0);
                    interruption(false);
                }
                2 => interruption(true),
                3 => {
                    interruption(false);
                    assert!(BLOQUE.load(Relaxed));
                    interruption(true);
                    interruption(false); // notification dupliquée après consommation
                }
                4 | 5 | 6 | 7 | 8 | 10 => {
                    if scenario == 4 {
                        USED.store(INDEX.load(Relaxed), Relaxed);
                    }
                    if scenario == 7 {
                        EXPECT.store(0, Relaxed);
                        interruption(true);
                    } // ticket retiré
                    CLOCK.store(HZ - 1, Relaxed);
                    attente_irq::tick();
                    assert!(BLOQUE.load(Relaxed));
                    attente_irq::tick();
                    assert!(!BLOQUE.load(Relaxed));
                    if scenario == 8 {
                        interruption(true);
                        CLOCK.store(2 * HZ, Relaxed); // reprise après la deadline finale
                    }
                }
                _ => panic!("parcage inattendu"),
            }
            assert!(!BLOQUE.load(Relaxed));
        }
    }
}

#[test]
fn entrelacements_et_delais() {
    let _serie = SERIE.lock().unwrap();
    let mut completions = 0;
    for index in [1, 0xffff, 0] {
        for scenario in 0..=9 {
            SCENARIO.store(scenario, Relaxed);
            IRQS.store(u64::MAX, Relaxed); // le prochain IRQ amène aussi ce total à zéro
            EXPECT.store(0x10000 | u64::from(index), Relaxed);
            INDEX.store(index, Relaxed);
            USED.store(index.wrapping_sub(1), Relaxed);
            CLOCK.store(0, Relaxed);
            PARCAGES.store(0, Relaxed);
            REVEILS.store(0, Relaxed);
            LECTURES.store(0, Relaxed);
            assert!(!MASQUE.load(Relaxed) && !BLOQUE.load(Relaxed));
            attente_irq::preparer().unwrap();
            let pilote = Transport {
                q: Queue {
                    index,
                    pages: core::array::from_fn(|_| Page { mem: Mem }),
                },
            };
            if scenario == 1 {
                USED.store(index, Relaxed);
            }
            if matches!(scenario, 0 | 9) {
                interruption(true);
            }
            let r = pilote.collecter_irq(u64::MAX);
            assert_eq!(
                PARCAGES.load(Relaxed),
                u64::from(!matches!(scenario, 0 | 9))
            );
            assert_eq!(PARCAGES.load(Relaxed), REVEILS.load(Relaxed));
            let succes = matches!(scenario, 0 | 1 | 2 | 3 | 6);
            let attendu = if succes {
                Ok(4097)
            } else if scenario == 9 {
                Err(Error::Queue)
            } else {
                Err(Error::Timeout)
            };
            assert_eq!(r, attendu, "scénario {scenario}, index {index}");
            assert_eq!(LECTURES.load(Relaxed), u64::from(succes || scenario == 9));
            if r.is_ok() {
                completions += 1;
            }
        }
    }
    assert_eq!(completions, 15);
}

#[test]
fn repli_borne_si_horloge_gelee() {
    let _serie = SERIE.lock().unwrap();
    SCENARIO.store(10, Relaxed);
    INDEX.store(7, Relaxed);
    USED.store(6, Relaxed);
    IRQS.store(9, Relaxed);
    EXPECT.store(0x10007, Relaxed);
    CLOCK.store(0, Relaxed);
    LECTURES.store(0, Relaxed);
    attente_irq::preparer().unwrap();
    let pilote = Transport {
        q: Queue {
            index: 7,
            pages: core::array::from_fn(|_| Page { mem: Mem }),
        },
    };
    assert_eq!(pilote.collecter_irq(9), Err(Error::Timeout));
    assert_eq!(LECTURES.load(Relaxed), 0);
    assert!(!BLOQUE.load(Relaxed));
}
