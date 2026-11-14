//! Ticket privé du seul BDF EDU adopté par le recensement de confiance.
//! Le registre PCI_COMMAND est le seul registre de configuration modifiable ici.
use super::{Guard, platform};
use crate::memory;
use core::cell::UnsafeCell;

const ID: u32 = 0x11e8_1234;
const BDF: u16 = 6 << 3;

#[cfg(target_arch = "aarch64")]
const CONFIG_PA: u64 = 0x0040_1000_0000 + ((BDF as u64) << 12);
#[cfg(target_arch = "aarch64")]
const CONFIG_VA: u64 = memory::IO_BASE + 326 * memory::PAGE;

#[derive(Clone, Copy, Eq, PartialEq)]
enum Phase {
    Adoption,
    Open,
    Closing,
}

#[derive(Clone, Copy)]
struct Ticket {
    bar: u32,
    initial: u16,
    phase: Phase,
}
impl Ticket {
    fn adopted(self) -> u16 {
        (self.initial & !7) | 2
    }
    fn closed(self) -> u16 {
        self.initial & !7
    }
}
struct Owner(UnsafeCell<Option<Ticket>>);
// Sûreté : comme le slot IRQ, cet état est privé et sous Guard sur tous les CPU.
unsafe impl Sync for Owner {}
static OWNER: Owner = Owner(UnsafeCell::new(None));

fn owner<T>(_: &Guard, f: impl FnOnce(&mut Option<Ticket>) -> T) -> T {
    unsafe { f(&mut *OWNER.0.get()) }
}

// Sous la garde, personne ne peut changer le sélecteur CF8 ou la page ECAM.
// Sur ARM la page fixe est commune aux tables superviseur, Device/NX, et sa
// suppression utilise l’invalidation TLB SMP, aussi après restitution de Limine.
fn with_config(_: &Guard, operation: impl FnOnce() -> bool) -> bool {
    #[cfg(target_arch = "aarch64")]
    {
        if !memory::firmware::unlisted_extent(CONFIG_PA, memory::PAGE)
            || memory::map_io(CONFIG_PA, CONFIG_VA).is_err()
        {
            return false;
        }
        let result = operation();
        memory::unmap_lab_io(CONFIG_PA, CONFIG_VA).is_ok() && result
    }
    #[cfg(target_arch = "x86_64")]
    operation()
}
fn read_config(offset: u16) -> u32 {
    debug_assert!(matches!(offset, 0 | 4 | 0x10));
    #[cfg(target_arch = "x86_64")]
    {
        crate::hardware::pci_ports::port(BDF, offset.into(), 4, None)
    }
    #[cfg(target_arch = "aarch64")]
    {
        // Sûreté : fonction privée utilisée exclusivement sous with_config,
        // dword aligné compris dans la page du BDF fixe.
        unsafe { ((CONFIG_VA + u64::from(offset)) as *const u32).read_volatile() }
    }
}
fn command(value: u16) {
    #[cfg(target_arch = "x86_64")]
    crate::hardware::pci_ports::port(BDF, 4, 2, Some(u32::from(value)));
    #[cfg(target_arch = "aarch64")]
    unsafe {
        // Sûreté : PCI_COMMAND a 16 bits. PCI_STATUS adjacent (W1C) est préservé.
        ((CONFIG_VA + 4) as *mut u16).write_volatile(value);
        core::arch::asm!("dsb sy", options(nostack, preserves_flags));
    }
}
fn identity(bar: u32) -> bool {
    read_config(0) == ID && read_config(0x10) == bar
}

pub(in crate::hardware) fn adopt(base: u64, initial: u16) -> Option<u16> {
    let guard = Guard::new();
    if owner(&guard, |slot| slot.is_some())
        || base & 0xfffff != 0
        || !platform::contains_bar(base)
        || !memory::firmware::unlisted_extent(base, 1 << 20)
    {
        return None;
    }
    let t = Ticket {
        bar: u32::try_from(base).ok()?,
        initial,
        phase: Phase::Adoption,
    };
    let ok = with_config(&guard, || {
        if !identity(t.bar) || read_config(4) as u16 != initial {
            return false;
        }
        // Conserver la responsabilité dès la première écriture, même si sa
        // relecture ou le retrait de la page ECAM échoue.
        owner(&guard, |slot| *slot = Some(t));
        if t.adopted() != initial {
            command(t.adopted());
        }
        identity(t.bar) && read_config(4) as u16 == t.adopted()
    });
    if !ok {
        if !close(&guard) {
            crate::hal::log(format_args!(
                "EDU PCI fermeture non confirmee apres adoption"
            ));
        }
        return None;
    }
    owner(&guard, |slot| slot.as_mut().unwrap().phase = Phase::Open);
    Some(t.adopted())
}

pub(super) fn ready(guard: &Guard, base: u64) -> bool {
    owner(guard, |slot| {
        slot.is_some_and(|t| t.phase == Phase::Open && u64::from(t.bar) == base)
    })
}
pub(super) fn stop_admissions(guard: &Guard) {
    owner(guard, |slot| {
        if let Some(t) = slot
            && t.phase == Phase::Open
        {
            t.phase = Phase::Closing;
        }
    });
}
// Une identité inattendue, un bit étranger ou un retrait ECAM incertain
// conservent le ticket fermé. Une reprise idempotente relit réellement PCI.
pub(super) fn close(guard: &Guard) -> bool {
    let Some(t) = owner(guard, |slot| *slot) else {
        return true;
    };
    if t.phase == Phase::Open {
        return false;
    }
    let confirmed = with_config(guard, || {
        if !identity(t.bar) {
            return false;
        }
        let old = read_config(4) as u16;
        if old != t.closed() {
            if old != t.adopted() && !(t.phase == Phase::Adoption && old == t.initial) {
                return false;
            }
            command(t.closed());
        }
        identity(t.bar) && read_config(4) as u16 == t.closed()
    });
    if confirmed {
        owner(guard, |slot| *slot = None);
        crate::hal::log(format_args!(
            "EDU PCI retrait 0000:00:06.0 BAR0={:#x} commande={:#06x}",
            t.bar,
            t.closed()
        ));
    }
    confirmed
}
