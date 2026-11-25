//! Route INTx exclusive du profil QEMU EDU. Les écritures de BAR portent sur
//! la vivacité et la cause de test. Le ticket PCI encadre le décodage mémoire.
use crate::{memory, sched::Guard};
use core::cell::UnsafeCell;

#[cfg(target_arch = "aarch64")]
#[path = "lab_irq/arm.rs"]
mod platform;
#[cfg(target_arch = "x86_64")]
#[path = "lab_irq/x86.rs"]
mod platform;

mod pci;
pub(super) use pci::adopt;
#[cfg(all(target_arch = "x86_64", feature = "vtd-lab"))]
pub(super) use pci::{dma_fenced, dma_master};

const REGISTERS: u64 = memory::IO_BASE + 323 * memory::PAGE;
const CAUSE: u32 = 1;
#[derive(Default)]
struct Binding {
    epoch: u64,
    sequence: u64,
    armed: bool,
    pending: bool,
    retiring: bool,
}
struct Device {
    physical: u64,
    route: platform::Route,
    binding: Option<Binding>,
    route_restored: bool,
    probing: bool,
    unbound_entries: u64,
    notifications: u64,
}
struct Slot(UnsafeCell<Option<Device>>);
// Sûreté : tous les accès ont lieu sous la garde inter-CPU du scheduler, IRQ
// locales masquées. Aucun emprunt de l'objet ne traverse un callback ou un switch.
unsafe impl Sync for Slot {}
static SLOT: Slot = Slot(UnsafeCell::new(None));
// L'époque survit aussi à la restitution de la route de laboratoire.
static LAST_EPOCH: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);
fn state<T>(_: &Guard, f: impl FnOnce(&mut Option<Device>) -> T) -> T {
    unsafe { f(&mut *SLOT.0.get()) }
}

fn io_read(base: u64, off: u64) -> u32 {
    // Sûreté : les appelants utilisent exclusivement les registres alignés de la
    // page superviseur Device/UC vérifiée ; le slot conserve le mapping sous Guard.
    unsafe { ((base + off) as *const u32).read_volatile() }
}
fn io_write(base: u64, off: u64, value: u32) {
    unsafe { ((base + off) as *mut u32).write_volatile(value) }
    #[cfg(target_arch = "aarch64")]
    unsafe {
        core::arch::asm!("dsb sy", options(nostack, preserves_flags))
    };
}
fn read(off: u64) -> u32 {
    io_read(REGISTERS, off)
}
fn write(off: u64, value: u32) {
    io_write(REGISTERS, off, value);
}
fn idle_device() -> bool {
    read(0x24) == 0 && read(0x98) == 0 && read(0x20) == 0
}

/// Appelé seulement après qualification PCI du BDF, du BAR et de l'absence
/// de propriétaire concurrent. Le profil est borné par les ouvertures PCI QEMU.
pub(crate) fn prepare(bar_pa: u64) -> bool {
    let guard = Guard::new();
    state(&guard, |slot| {
        if slot.is_some() || crate::smp::index() != 0 || !pci::ready(&guard, bar_pa) {
            return false;
        }
        if !platform::contains_bar(bar_pa)
            || !memory::firmware::unlisted_extent(bar_pa, 1 << 20)
            || memory::map_io(bar_pa, REGISTERS).is_err()
        {
            return false;
        }
        // Les registres de DMA sont également lisibles dans la page utilisateur
        // RO. Sur ce profil neuf, aucune ancienne adresse ne doit y subsister.
        let known = read(0) == 0x0100_00ed
            && idle_device()
            && [0x80, 0x88, 0x90]
                .into_iter()
                .all(|off| unsafe { ((REGISTERS + off) as *const u64).read_volatile() == 0 });
        if known && let Some(route) = platform::Route::new() {
            *slot = Some(Device {
                physical: bar_pa,
                route,
                binding: None,
                route_restored: false,
                probing: false,
                unbound_entries: 0,
                notifications: 0,
            });
            return true;
        }
        memory::unmap_lab_io(bar_pa, REGISTERS).expect("retrait page EDU non publiée");
        false
    })
}

pub(crate) fn bind(epoch: u64) -> bool {
    let guard = Guard::new();
    state(&guard, |slot| {
        let Some(d) = slot else { return false };
        if !pci::ready(&guard, d.physical)
            || d.probing
            || d.binding.is_some()
            || epoch <= LAST_EPOCH.load(core::sync::atomic::Ordering::Relaxed)
            || !idle_device()
            || !d.route.drained()
        {
            return false;
        }
        LAST_EPOCH.store(epoch, core::sync::atomic::Ordering::Relaxed);
        d.binding = Some(Binding {
            epoch,
            ..Binding::default()
        });
        true
    })
}

fn active<T>(epoch: u64, f: impl FnOnce(&mut Binding, &mut platform::Route) -> T) -> Option<T> {
    let guard = Guard::new();
    state(&guard, |slot| {
        let d = slot.as_mut()?;
        if !pci::ready(&guard, d.physical) {
            return None;
        }
        let b = d.binding.as_mut()?;
        (b.epoch == epoch && !b.retiring).then(|| f(b, &mut d.route))
    })
}
pub(crate) fn live(epoch: u64, value: u32) -> Option<u32> {
    active(epoch, |_, _| {
        write(4, value);
        read(4)
    })
}
pub(crate) fn sequence(epoch: u64) -> Option<u64> {
    active(epoch, |b, _| b.pending.then_some(b.sequence)).flatten()
}
pub(crate) fn unmask(epoch: u64) -> bool {
    active(epoch, |b, route| {
        if b.pending || read(0x24) & !CAUSE != 0 {
            return false;
        }
        if !route.mask(false) {
            b.retiring = true;
            return false;
        }
        b.armed = true;
        true
    })
    .unwrap_or(false)
}
pub(crate) fn mask(epoch: u64) -> bool {
    active(epoch, |b, route| {
        b.armed = false;
        let confirmed = route.mask(true);
        b.retiring |= !confirmed;
        confirmed
    })
    .unwrap_or(false)
}
/// Une écriture masquée reste mémorisée dans le registre d'interruption EDU.
pub(crate) fn raise(epoch: u64) -> bool {
    active(epoch, |_, _| {
        write(0x60, CAUSE);
        read(0x24) == CAUSE
    })
    .unwrap_or(false)
}
pub(crate) fn acknowledge(epoch: u64, sequence: u64) -> bool {
    active(epoch, |b, _| {
        if !b.pending || b.sequence != sequence || read(0x24) != CAUSE {
            return false;
        }
        write(0x64, CAUSE);
        if read(0x24) != 0 {
            b.retiring = true;
            return false;
        }
        b.pending = false;
        true
    })
    .unwrap_or(false)
}

/// L'IRQ ne prend aucun verrou bloquant. Le contrôleur global reste masqué
/// jusqu'à l'acquittement applicatif suivi d'un démasquage explicite.
pub(crate) fn interrupt() -> bool {
    let guard = Guard::new();
    let (handled, notice) = state(&guard, |slot| {
        let Some(d) = slot else { return (false, None) };
        if d.route_restored {
            return (false, None);
        }
        let masked = d.route.mask(true);
        let Some(b) = &mut d.binding else {
            if masked && read(0x24) == CAUSE {
                d.unbound_entries = d.unbound_entries.saturating_add(1);
            }
            return (true, None);
        };
        b.retiring |= !masked || !pci::ready(&guard, d.physical);
        let armed = core::mem::take(&mut b.armed);
        let cause = read(0x24);
        if cause & !CAUSE != 0 {
            b.retiring = true;
        }
        if !armed || b.retiring || b.pending || cause != CAUSE {
            return (true, None);
        }
        let Some(next) = b.sequence.checked_add(1) else {
            b.retiring = true;
            return (true, None);
        };
        b.sequence = next;
        b.pending = true;
        d.notifications = d.notifications.saturating_add(1);
        (true, Some((b.epoch, next)))
    });
    if let Some((epoch, seq)) = notice {
        if !crate::sched::process::lab_irq_notice(epoch, seq) {
            state(&guard, |slot| {
                if let Some(b) = slot.as_mut().and_then(|d| d.binding.as_mut()) {
                    b.retiring = true;
                }
            });
        }
    }
    handled
}

/// Une route en vol ou une cause inconnue garde le droit fermé et le slot
/// possédé. L'appelant peut reprendre sur une autre CPU après l'EOI réel.
pub(crate) fn retire(epoch: u64) -> bool {
    let guard = Guard::new();
    state(&guard, |slot| {
        let Some(d) = slot else { return false };
        let Some(b) = &mut d.binding else {
            return false;
        };
        if b.epoch != epoch {
            return false;
        }
        b.retiring = true;
        b.armed = false;
        if !d.route.mask(true) {
            return false;
        }
        if read(0x24) == CAUSE {
            write(0x64, CAUSE);
        }
        if !idle_device() || !d.route.drained() {
            return false;
        }
        // Un pilote ne transmet pas sa dernière valeur de vivacité au suivant.
        write(4, !0);
        if read(4) != 0 {
            return false;
        }
        d.binding = None;
        true
    })
}

/// Déclenche une vraie INTA pendant l'intervalle entre deux incarnations. Le
/// gestionnaire du banc appelle cette sonde après la collecte du premier pilote.
pub(crate) fn probe_unbound() -> bool {
    use crate::sched::{HZ, sleep, ticks};
    let baseline = {
        let guard = Guard::new();
        state(&guard, |slot| {
            let d = slot.as_mut()?;
            if crate::smp::index() != 0
                || d.probing
                || d.binding.is_some()
                || !pci::ready(&guard, d.physical)
                || !idle_device()
                || !d.route.drained()
            {
                return None;
            }
            d.probing = true;
            let start = (d.unbound_entries, d.notifications);
            write(0x60, CAUSE);
            let armed = read(0x24) == CAUSE && d.route.mask(false);
            Some((start, armed))
        })
    };
    let Some(((entries, notices), armed)) = baseline else {
        return false;
    };
    let start = ticks();
    while armed && ticks().wrapping_sub(start) < u64::from(HZ) {
        let seen = {
            let guard = Guard::new();
            state(&guard, |slot| {
                slot.as_ref().is_some_and(|d| d.unbound_entries > entries)
            })
        };
        if seen {
            break;
        }
        sleep(1);
    }
    let guard = Guard::new();
    state(&guard, |slot| {
        let Some(d) = slot else { return false };
        let masked = d.route.mask(true);
        let cause = read(0x24);
        if cause == CAUSE {
            write(0x64, CAUSE);
        }
        // Le thread revient sur le BSP après la sortie du vrai handler et son EOI.
        let drained = masked && idle_device() && d.route.drained();
        let n = d.unbound_entries - entries;
        let sent = d.notifications - notices;
        let ok = armed
            && d.probing
            && d.binding.is_none()
            && cause == CAUSE
            && n == 1
            && sent == 0
            && drained;
        crate::hal::log(format_args!(
            "PILOTE IRQ sans bail entrees={n} notifications={sent} drain={drained}"
        ));
        if !ok {
            pci::stop_admissions(&guard);
        }
        d.probing = false;
        ok
    })
}

pub(crate) fn shutdown() -> bool {
    let guard = Guard::new();
    state(&guard, |slot| {
        // Après le début de fermeture, aucun nouvel acquéreur n’est admis.
        // Les contrôleurs sont encore accessibles si le dernier retrait doit être repris.
        pci::stop_admissions(&guard);
        if let Some(d) = slot {
            if crate::smp::index() != 0 || d.binding.is_some() || d.probing {
                return false;
            }
            if !d.route_restored {
                if !idle_device() || !d.route.drained() || !d.route.restore() {
                    return false;
                }
                d.route_restored = true;
            }
            if memory::unmap_lab_io(d.physical, REGISTERS).is_err() {
                return false;
            }
            *slot = None;
        }
        pci::close(&guard)
    })
}

#[cfg(target_arch = "x86_64")]
pub(crate) fn x86_entry() -> bool {
    let guard = Guard::new();
    #[cfg(all(feature="net-q35-lab",feature="net-ethernet-lab"))]
    if net_intx::interrompre() {
        platform::eoi();
        return true;
    }
    if !interrupt() {
        return false;
    }
    platform::eoi();
    drop(guard);
    true
}

#[cfg(all(target_arch="x86_64",feature="net-q35-lab",feature="net-ethernet-lab"))]
pub(super) mod net_intx {
    use super::{platform,Guard,state};
    use core::sync::atomic::{AtomicUsize,Ordering};
    static ISR:AtomicUsize=AtomicUsize::new(0);
    static CAUSE:AtomicUsize=AtomicUsize::new(0);
    pub(in crate::hardware) struct Ligne(platform::Route<23>);
    pub(super) fn interrompre()->bool{
        let base=ISR.load(Ordering::Acquire);
        if base==0 {return false}
        // Registre ISR protégé au superviseur. Le BSP conserve le mapping jusqu'au drainage.
        #[cfg(feature="net-q35-duo")]
        if !super::super::vtd_lab::irq_delivery_ok() {
            platform::Route::<23>::masquer_nic();
            CAUSE.store(0x200, Ordering::Release);
            return true;
        }
        let raison=unsafe{(base as *const u8).read_volatile()};
        let masque=platform::Route::<23>::masquer_nic();
        CAUSE.fetch_or((if masque {0x100} else {0x200})|usize::from(raison),Ordering::Release);
        true
    }
    impl Ligne {
        pub(in crate::hardware) fn ouvrir(isr:usize)->Option<Self>{
            let g=Guard::new();
            if state(&g,|slot|slot.is_some()) || ISR.load(Ordering::Acquire)!=0 {return None}
            let ligne=platform::Route::<23>::new()?;
            CAUSE.store(0,Ordering::Release);
            ISR.store(isr,Ordering::Release);
            if !ligne.mask(false) {
                if ligne.mask(true) && ligne.drained() && ligne.restore() {ISR.store(0,Ordering::Release);}
                return None
            }
            Some(Self(ligne))
        }
        pub(in crate::hardware) fn cause(&self)->usize{CAUSE.load(Ordering::Acquire)}
        pub(in crate::hardware) fn rearmer(&self)->bool{
            let _g=Guard::new();
            CAUSE.swap(0,Ordering::AcqRel)==0x101 && self.0.mask(false)
        }
        pub(in crate::hardware) fn fermer(self)->bool{
            let _g=Guard::new();
            if !self.0.mask(true) || !self.0.drained() {return false}
            ISR.store(0,Ordering::Release);
            self.0.restore()
        }
    }
}
