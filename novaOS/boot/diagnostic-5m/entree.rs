#![no_std]
#![no_main]
extern crate alloc;

use core::{
    arch::asm,
    ptr, slice,
    sync::atomic::{AtomicBool, Ordering},
};
use ed25519_dalek::{Signature, VerifyingKey};
use r_efi::{efi, protocols::device_path};
use sha2::{Digest, Sha256};

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

#[global_allocator]
static HEAP: memoire::Heap = memoire::Heap;
static CONFIG: [u8; 16] = *b"NOVA5E-CFG!\x01\x05\x01\x00\x03";
static UTILISE: AtomicBool = AtomicBool::new(false);
const ORIGINE: &[u8] = b"NovaOS/5M/OVMF-5J/ramdisk/EFI/BOOT/BOOTX64.EFI\0";
const TAILLE: usize = 176;
const AUTORITE: &[u8; 32] = include_bytes!("autorite-laboratoire.pub");

#[repr(C)]
struct Resultat {
    marque: [u8; 16],
    adresse: u64,
    statut: u64,
    confiance: u64,
    memoire: u64,
    vivants: u64,
    epoque: [u8; 16],
    image: [u8; 32],
}
static mut SORTIE: Resultat = unsafe { core::mem::zeroed() };

fn donnees(t: u32) -> bool {
    matches!(
        t,
        efi::LOADER_DATA | efi::BOOT_SERVICES_DATA | efi::BOOT_SERVICES_CODE
    )
}
fn u64le(b: &[u8]) -> u64 {
    u64::from_le_bytes(b.try_into().unwrap())
}
fn u32le(b: &[u8]) -> usize {
    u32::from_le_bytes(b.try_into().unwrap()) as usize
}

// Un seul nœud RAM Disk EFI suivi de End Entire. Aucun Block I/O.
fn disque_ram<'a>(
    bs: &efi::BootServices,
    handle: efi::Handle,
    carte: &'a firmware::Map,
) -> Result<&'a [u8], u64> {
    let mut p = ptr::null_mut();
    let s = unsafe { (bs.handle_protocol)(handle, &mut { device_path::PROTOCOL_GUID }, &mut p) };
    if s != efi::Status::SUCCESS || !carte.covers(p as u64, 42, donnees) {
        return Err(20);
    }
    let node = unsafe { ptr::read_unaligned(p.cast::<[u8; 42]>()) };
    const DISQUE_VIRTUEL: [u8; 16] = [
        0x5a, 0x53, 0xab, 0x77, 0xfc, 0x45, 0x4b, 0x62, 0x55, 0x60, 0xf7, 0xb2, 0x81, 0xd1, 0xf9,
        0x6e,
    ];
    if node[..4] != [4, 9, 38, 0]
        || node[20..36] != DISQUE_VIRTUEL
        || node[38..] != [0x7f, 0xff, 4, 0]
    {
        return Err(21);
    }
    let base = u64le(&node[4..12]);
    let end = u64le(&node[12..20]);
    let n = end
        .checked_sub(base)
        .and_then(|n| n.checked_add(1))
        .ok_or(22u64)?;
    if !(4096..=64 << 20).contains(&n) || !carte.covers(base, n as usize, donnees) {
        return Err(22);
    }
    Ok(unsafe { slice::from_raw_parts(base as *const u8, n as usize) })
}

fn examiner(h: efi::Handle, st: &efi::SystemTable, r: &mut Resultat) -> Result<(), u64> {
    let bs = unsafe { st.boot_services.as_ref() }.ok_or(2u64)?;
    let img = unsafe { firmware::protocole_image(bs, h) }?;
    let carte = firmware::map(bs)?;
    if img.load_options_size != 32 || !carte.covers(img.load_options as u64, 32, donnees) {
        return Err(16);
    }
    let ancre = unsafe { ptr::read_unaligned(img.load_options.cast::<[u8; 32]>()) };
    let ram = disque_ram(bs, img.device_handle, &carte)?;
    let fin = ram.len() - TAILLE;
    let rec = &ram[fin..];
    if &rec[..16] != b"NOVA5M-TRUST-V1!" {
        return Err(23);
    }
    let attendu: [u8; 32] = Sha256::new()
        .chain_update(ORIGINE)
        .chain_update(&rec[..112])
        .finalize()
        .into();
    if attendu != ancre || rec[32..48] == [0; 16] {
        return Err(24);
    }
    if rec[80..112] != AUTORITE[..] {
        return Err(25);
    }
    let cle = VerifyingKey::from_bytes(AUTORITE).map_err(|_| 25u64)?;
    cle.verify_strict(
        &rec[..112],
        &Signature::from_bytes(rec[112..].try_into().unwrap()),
    )
    .map_err(|_| 25u64)?;
    if rec[16] > 1
        || rec[17] != 0
        || !matches!(rec[18], 0 | 255)
        || rec[19] != 1
        || rec[28..32] != [0; 4]
    {
        return Err(26);
    }
    let off = u32le(&rec[20..24]);
    let taille = u32le(&rec[24..28]);
    if !(512..=8 << 20).contains(&taille) {
        return Err(27);
    }
    let pe = ram[..fin]
        .get(off..off.checked_add(taille).ok_or(27u64)?)
        .ok_or(27u64)?;
    if !pe.starts_with(b"MZ") || <[u8; 32]>::from(Sha256::digest(pe)) != rec[48..80] {
        return Err(27);
    }
    if UTILISE.swap(true, Ordering::AcqRel) {
        return Err(28);
    }
    r.epoque.copy_from_slice(&rec[32..48]);
    r.image.copy_from_slice(&rec[48..80]);
    r.confiance = rec[16].into();
    let cfg = unsafe { ptr::read_volatile(&CONFIG) };
    if cfg[..15] != *b"NOVA5E-CFG!\x01\x05\x01\x00" || cfg[15] < 3 {
        return Err(11);
    }
    let politique = catalogue::policy(cfg[15].into()).map_err(|_| 6u64)?;
    let _catalogue = catalogue::catalog(&politique).map_err(|e| match e {
        novaos_catalogue::Error::Signature => 6,
        novaos_catalogue::Error::Elf => 18,
        _ => 14u64,
    })?;
    Ok(())
}

#[unsafe(no_mangle)]
pub extern "efiapi" fn efi_main(h: efi::Handle, st: *mut efi::SystemTable) -> ! {
    let r = unsafe { &mut *(&raw mut SORTIE) };
    r.adresse = (&raw mut SORTIE) as u64;
    r.confiance = 2; // Indéterminée jusqu'à vérification du transfert.
    r.statut = unsafe { st.as_ref() }
        .ok_or(2u64)
        .and_then(|s| examiner(h, s, r))
        .err()
        .unwrap_or(0);
    let (used, live) = memoire::close();
    r.memoire = used as u64;
    r.vivants = live as u64;
    if live != 0 {
        r.statut = 8
    }
    unsafe { ptr::write_volatile(&mut r.marque, *b"NOVA5M-RAM-FINAL") };
    arret()
}
#[inline(never)]
fn arret() -> ! {
    unsafe { asm!("2:", "hlt", "jmp 2b", options(noreturn, nostack)) }
}
#[panic_handler]
fn panique(_: &core::panic::PanicInfo) -> ! {
    arret()
}
