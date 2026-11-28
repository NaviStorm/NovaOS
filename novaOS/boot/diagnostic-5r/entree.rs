#![no_std]
#![no_main]
extern crate alloc;
use core::{
    arch::asm,
    ptr, slice,
    sync::atomic::{AtomicBool, Ordering},
};
use ed25519_dalek::{Signature, VerifyingKey};
use r_efi::{
    efi,
    protocols::{device_path, loaded_image},
};
use sha2::{Digest, Sha256};
#[allow(dead_code, unexpected_cfgs)]
#[path = "../../kernel/src/hardware/acpi.rs"]
mod acpi;
#[allow(dead_code)]
#[path = "../diagnostic-5e/catalogue.rs"]
mod catalogue;
#[allow(dead_code, unexpected_cfgs)]
#[path = "../diagnostic-5e/chemins.rs"]
mod chemins;
mod contrat;
mod ecran;
#[allow(dead_code, unexpected_cfgs)]
#[path = "../diagnostic-5e/firmware.rs"]
mod firmware;
mod inventaire;
#[path = "../diagnostic-5e/memoire.rs"]
mod memoire;
#[global_allocator]
static HEAP: memoire::Heap = memoire::Heap;
static UTILISE: AtomicBool = AtomicBool::new(false);
const ORIGINE: &[u8] = b"NovaOS/5M/OVMF-5J/ramdisk/EFI/BOOT/BOOTX64.EFI\0";
const TAILLE: usize = 176;
const AUTORITE: &[u8; 32] = include_bytes!("../diagnostic-5m/autorite-laboratoire.pub");
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
    usb: u64,
    rid: u64,
    port: u64,
    pages: u64,
    tables: u64,
    pci: u64,
    ioapic: u64,
    hpet: u64,
    drhd: u64,
    compatibles: u64,
    conflits: u64,
    activation: u64,
    plan: [u8; 32],
}
static mut SORTIE: Resultat = unsafe { core::mem::zeroed() };
include!(concat!(env!("OUT_DIR"), "/reception5m.rs"));
fn check(ok: bool, n: u64) -> Result<(), u64> {
    if ok { Ok(()) } else { Err(n) }
}
fn hash(x: &[u8]) -> [u8; 32] {
    Sha256::digest(x).into()
}
fn mot(x: &[u8]) -> u64 {
    u64::from_le_bytes(x.try_into().unwrap())
}
fn signature(b: &[u8], err: u64) -> Result<&[u8], u64> {
    let (body, sig) = b
        .split_at_checked(b.len().checked_sub(64).ok_or(err)?)
        .ok_or(err)?;
    VerifyingKey::from_bytes(AUTORITE)
        .map_err(|_| err)?
        .verify_strict(body, &Signature::from_slice(sig).map_err(|_| err)?)
        .map_err(|_| err)?;
    Ok(body)
}
fn diagnostiquer(
    st: &efi::SystemTable,
    map: &firmware::Map,
    img: &loaded_image::Protocol,
    ram: &[u8],
    off: usize,
    pe: &[u8],
    r: &mut Resultat,
) -> Result<(), u64> {
    let len = ram.len().checked_sub(16384).ok_or(50u64)?;
    check(
        len >= 1 << 20 && off.checked_add(pe.len()).is_some_and(|v| v <= len),
        53,
    )?;
    check(
        map.covers(img.image_base as u64, img.image_size as usize, |t| {
            donnees(t) || t == efi::LOADER_CODE
        }),
        53,
    )?;
    // Copie privée unique des contrats. Aucun pointeur reçu ne devient une adresse de DMA ou de périphérique.
    let suffix = ram[len..].to_vec();
    let old = &suffix[16384 - 344..16384 - 176];
    let bloc = &suffix[8192..16384 - 344];
    let lease = bloc.get(..232).ok_or(44u64)?;
    let surface = ecran::autoriser(st, map, r, old, lease)?;
    let mut info = &bloc[232..];
    let mut champ = |max: usize| {
        let n = u32le(info.get(..4).ok_or(50u64)?);
        if n > max || info.len() < 4 + n {
            return Err(50u64);
        }
        let value = &info[4..4 + n];
        info = &info[4 + n..];
        Ok(value)
    };
    let result = (|| {
        let rec = champ(6144)?;
        let path = champ(128)?;
        let manifest = champ(1024)?;
        let sig = champ(64)?;
        let elf = champ(512)?;
        let a = contrat::Attendu {
            r,
            esp: &ram[..len],
            base: img.image_base as u64,
            taille: img.image_size,
            offset: off,
            pe,
            surface: surface.faits(),
            chemin: path,
            media: &lease[136..168],
        };
        let (rid, port, pages) = contrat::verifier(rec, &a)?;
        check(
            suffix.get(..rec.len()) == Some(rec) && hash(rec) == lease[104..136],
            57,
        )?;
        r.usb = 1;
        r.rid = rid;
        r.port = port;
        r.pages = pages;
        inventaire::examiner(st, map, manifest, sig, elf, r)?;
        Ok(())
    })();
    let status = result.as_ref().err().copied().unwrap_or(0);
    surface.afficher(r, status);
    result
}
#[unsafe(no_mangle)]
pub extern "efiapi" fn efi_main(h: efi::Handle, st: *mut efi::SystemTable) -> ! {
    let r = unsafe { &mut *(&raw mut SORTIE) };
    r.adresse = (&raw mut SORTIE) as u64;
    r.confiance = 2;
    r.statut = unsafe { st.as_ref() }
        .ok_or(2u64)
        .and_then(|s| examiner(h, s, r))
        .err()
        .unwrap_or(0);
    (r.memoire, r.vivants) = {
        let (m, v) = memoire::close();
        (m as u64, v as u64)
    };
    if r.vivants != 0 {
        r.statut = 8;
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
