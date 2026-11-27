#![no_std]
#![no_main]
extern crate alloc;

use core::{arch::asm, ptr};
use r_efi::efi;

#[allow(dead_code)]
#[path = "../diagnostic-5e/catalogue.rs"]
mod catalogue;
#[allow(dead_code)]
#[path = "../diagnostic-5e/firmware.rs"]
mod firmware;
#[path = "../diagnostic-5e/memoire.rs"]
mod memoire;
#[allow(dead_code)]
#[path = "../diagnostic-5e/phase5h.rs"]
mod phase5h;

#[cfg(feature = "diagnostic-5i")]
#[allow(dead_code)]
#[path = "../diagnostic-5e/journal5i.rs"]
mod journal5i;

#[global_allocator]
static HEAP: memoire::Heap = memoire::Heap;
static CONFIG: [u8; 16] = *b"NOVA5E-CFG!\x01\x05\x01\x00\x03";

#[repr(C)]
struct Recu {
    marque: [u8; 16],
    adresse: u64,
    tsc_entree: u64,
    statut: u64,
    confiance: u64,
    temporaire: u64,
    vivants: u64,
    epoque: [u8; 16],
}

static mut RECU: Recu = unsafe { core::mem::zeroed() };

fn examiner(handle: efi::Handle, systeme: &efi::SystemTable, r: &mut Recu) -> Result<(), u64> {
    let bs = unsafe { systeme.boot_services.as_ref() }.ok_or(2u64)?;
    let rt = unsafe { systeme.runtime_services.as_ref() }.ok_or(2u64)?;
    r.confiance = match firmware::trust(rt) {
        phase5h::Trust::Firmware => 1,
        phase5h::Trust::Unverified => 0,
        _ => return Err(1),
    };
    let image = unsafe { firmware::protocole_image(bs, handle) }?;
    let carte = firmware::map(bs)?;
    if image.load_options_size != 32
        || !carte.covers(image.load_options as u64, 32, |t| {
            matches!(
                t,
                efi::BOOT_SERVICES_DATA | efi::BOOT_SERVICES_CODE | efi::LOADER_DATA
            )
        })
    {
        return Err(16);
    }
    let options = unsafe { ptr::read_unaligned(image.load_options.cast::<[u8; 32]>()) };
    if &options[..16] != b"NOVA5J-BOOT-EPOC" || options[16..] == [0; 16] {
        return Err(16);
    }
    let cfg = unsafe { ptr::read_volatile(&CONFIG) };
    if &cfg[..15] != b"NOVA5E-CFG!\x01\x05\x01\x00" || cfg[15] < 3 {
        return Err(11);
    }
    let politique = catalogue::policy(cfg[15].into()).map_err(|_| 6u64)?;
    let _catalogue = catalogue::catalog(&politique).map_err(|_| 14u64)?;
    // Le détenteur externe de la VM compare cet écho à son bail à usage unique.
    // Le PE ne délivre aucune capacité à partir de ces octets.
    r.epoque.copy_from_slice(&options[16..]);
    Ok(())
}

#[unsafe(no_mangle)]
pub extern "efiapi" fn efi_main(handle: efi::Handle, systeme: *mut efi::SystemTable) -> ! {
    let r = unsafe { &mut *(&raw mut RECU) };
    r.tsc_entree = unsafe { core::arch::x86_64::_rdtsc() };
    r.adresse = (&raw mut RECU) as u64;
    r.statut = unsafe { systeme.as_ref() }
        .ok_or(2u64)
        .and_then(|st| examiner(handle, st, r))
        .err()
        .unwrap_or(0);
    let (utilises, vivants) = memoire::close();
    r.temporaire = utilises as u64;
    r.vivants = vivants as u64;
    if vivants != 0 {
        r.statut = 8;
    }
    unsafe { ptr::write_volatile(&mut r.marque, *b"NOVA5J-RAM-FINAL") };
    terminer()
}

fn terminer() -> ! {
    // Conserver IF et la livraison des interruptions jusqu'à l'arrêt externe.
    unsafe { asm!("2:", "hlt", "jmp 2b", options(noreturn, nostack)) }
}

#[panic_handler]
fn panique(_: &core::panic::PanicInfo) -> ! {
    terminer()
}
