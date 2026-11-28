#![no_std]
#![no_main]
#![allow(dead_code, unexpected_cfgs)]
extern crate alloc;

use alloc::{format, vec::Vec};
use core::{arch::asm, ffi::c_void, panic::PanicInfo, ptr};
use r_efi::{
    efi,
    protocols::{device_path, device_path_utilities, file, simple_file_system},
};
use sha2::{Digest, Sha256};

#[path = "../diagnostic-5e/catalogue.rs"]
mod catalogue;
#[path = "../diagnostic-5e/chemins.rs"]
mod chemins;
#[path = "../diagnostic-5e/firmware.rs"]
mod firmware;
#[path = "../diagnostic-5e/memoire.rs"]
mod memoire;
#[path = "../diagnostic-5e/phase5h.rs"]
mod phase5h;

#[global_allocator]
static HEAP: memoire::Heap = memoire::Heap;

// Trois rôles fixes dans cet ordre. Longueur LE32 puis SHA-256 pour chaque rôle.
const PLAN: &[u8; 116] = include_bytes!(env!("NOVA5Z_PLAN"));
type Resultat<T> = Result<T, &'static str>;

fn protocole(bs: &efi::BootServices, h: efi::Handle, mut guid: efi::Guid) -> Resultat<*mut c_void> {
    let mut p = ptr::null_mut();
    let s = unsafe { (bs.handle_protocol)(h, &mut guid, &mut p) };
    if s == efi::Status::SUCCESS && !p.is_null() {
        Ok(p)
    } else {
        Err("PROTOCOLE")
    }
}

struct Fichier(*mut file::Protocol);
impl Fichier {
    fn ouvrir(&self, nom: &str) -> Resultat<Self> {
        let mut chemin = [0u16; 32];
        if nom.len() >= chemin.len() || !nom.is_ascii() || nom.as_bytes().contains(&0) {
            return Err("NOM");
        }
        for (slot, b) in chemin.iter_mut().zip(nom.bytes()) {
            *slot = b as u16;
        }
        let mut p = ptr::null_mut();
        let s =
            unsafe { ((*self.0).open)(self.0, &mut p, chemin.as_mut_ptr(), file::MODE_READ, 0) };
        if s == efi::Status::SUCCESS && !p.is_null() {
            Ok(Self(p))
        } else {
            Err("OUVERTURE")
        }
    }
    fn lire(&self, dst: &mut [u8]) -> Resultat<usize> {
        let mut n = dst.len();
        let s = unsafe { ((*self.0).read)(self.0, &mut n, dst.as_mut_ptr().cast()) };
        if s == efi::Status::SUCCESS && n <= dst.len() {
            Ok(n)
        } else {
            Err("LECTURE")
        }
    }
    fn exact(&self, mut dst: &mut [u8]) -> Resultat<()> {
        while !dst.is_empty() {
            let n = self.lire(dst)?;
            if n == 0 {
                return Err("TRONQUE");
            }
            dst = &mut dst[n..];
        }
        Ok(())
    }
    fn fin(&self) -> Resultat<()> {
        if self.lire(&mut [0])? == 0 {
            Ok(())
        } else {
            Err("LONGUEUR")
        }
    }
}
impl Drop for Fichier {
    fn drop(&mut self) {
        unsafe {
            ((*self.0).close)(self.0);
        }
    }
}

fn verifier(volume: &Fichier) -> Resultat<Vec<u8>> {
    let plan = volume.ouvrir("\\PLAN.BIN")?;
    let mut lu = [0; PLAN.len()];
    plan.exact(&mut lu)?;
    plan.fin()?;
    if &lu != PLAN || &lu[..8] != b"NOVA5Z01" {
        return Err("MANIFESTE");
    }
    let mut limine = Vec::new();
    for ((nom, borne, motif), ligne) in [
        ("\\LIMINE.EFI", 1 << 20, "LIMINE"),
        ("\\LIMINE.CONF", 4096, "CONFIG"),
        ("\\NOVAOS.ELF", 24 << 20, "NOYAU"),
    ]
    .into_iter()
    .zip(lu[8..].chunks_exact(36))
    {
        let taille = u32::from_le_bytes(ligne[..4].try_into().unwrap()) as usize;
        if taille == 0 || taille > borne {
            return Err("BORNE");
        }
        let f = volume.ouvrir(nom)?;
        let mut reste = taille;
        let mut digest = Sha256::new();
        let mut bloc = [0u8; 4096];
        if motif == "LIMINE" {
            limine.try_reserve_exact(taille).map_err(|_| "MEMOIRE")?;
        }
        while reste != 0 {
            let n = reste.min(bloc.len());
            f.exact(&mut bloc[..n])?;
            digest.update(&bloc[..n]);
            if motif == "LIMINE" {
                limine.extend_from_slice(&bloc[..n]);
            }
            reste -= n;
        }
        f.fin()?;
        if digest.finalize()[..] != ligne[4..] {
            return Err(motif);
        }
    }
    Ok(limine)
}

#[repr(C)]
struct Noeud<const N: usize> {
    header: device_path::Protocol,
    nom: [u16; N],
}

fn transfert(
    bs: &efi::BootServices,
    h: efi::Handle,
    disque: efi::Handle,
    mut pe: Vec<u8>,
) -> Resultat<()> {
    let base = protocole(bs, disque, device_path::PROTOCOL_GUID)?;
    let mut u = ptr::null_mut();
    let s = unsafe {
        (bs.locate_protocol)(
            &mut { device_path_utilities::PROTOCOL_GUID },
            ptr::null_mut(),
            &mut u,
        )
    };
    if s != efi::Status::SUCCESS || u.is_null() {
        return Err("CHEMIN");
    }
    let nom = (*b"\\LIMINE.EFI\0").map(u16::from);
    let node = Noeud {
        header: device_path::Protocol {
            r#type: device_path::TYPE_MEDIA,
            sub_type: device_path::Media::SUBTYPE_FILE_PATH,
            length: (4 + 2 * nom.len() as u16).to_le_bytes(),
        },
        nom,
    };
    let path = unsafe {
        ((*u.cast::<device_path_utilities::Protocol>()).append_device_node)(
            base.cast(),
            &node.header,
        )
    };
    if path.is_null() {
        return Err("CHEMIN");
    }
    let mut enfant = ptr::null_mut();
    let s = unsafe {
        (bs.load_image)(
            false.into(),
            h,
            path,
            pe.as_mut_ptr().cast(),
            pe.len(),
            &mut enfant,
        )
    };
    unsafe {
        (bs.free_pool)(path.cast());
    }
    if s != efi::Status::SUCCESS || enfant.is_null() {
        return Err("LOADIMAGE");
    }
    let child = unsafe { firmware::protocole_image(bs, enfant) }.map_err(|_| "IMAGE")?;
    if child.device_handle != disque || child.parent_handle != h {
        return Err("ORIGINE-LIMINE");
    }
    // StartImage transfère au Limine vérifié. Tout retour est terminal.
    unsafe {
        (bs.start_image)(enfant, ptr::null_mut(), ptr::null_mut());
    }
    Err("RETOUR-LIMINE")
}

fn lancer(st: &efi::SystemTable, h: efi::Handle) -> Resultat<()> {
    let bs = unsafe { st.boot_services.as_ref() }.ok_or("SERVICES")?;
    if unsafe { (bs.set_watchdog_timer)(0, 0, 0, ptr::null_mut()) } != efi::Status::SUCCESS {
        return Err("WATCHDOG");
    }
    let dire = |s: &str| phase5h::display(st, s).map_err(|_| "CONSOLE");
    dire(&format!(
        "NOVA5Z EFI entree={:x}",
        efi_main as *const () as usize
    ))?;
    let rt = unsafe { st.runtime_services.as_ref() }.ok_or("CONFIANCE")?;
    if firmware::trust(rt) != phase5h::Trust::Unverified {
        return Err("MODE-UEFI");
    }
    let im = unsafe { firmware::image(bs, h) }.map_err(|_| "IMAGE")?;
    let map = firmware::map(bs).map_err(|_| "CARTE")?;
    if !chemins::usb(bs, im, &map) {
        return Err("ORIGINE-USB");
    }
    let disque = im.device_handle;
    let fs = protocole(bs, disque, simple_file_system::PROTOCOL_GUID)?
        .cast::<simple_file_system::Protocol>();
    let mut p = ptr::null_mut();
    if unsafe { ((*fs).open_volume)(fs, &mut p) } != efi::Status::SUCCESS || p.is_null() {
        return Err("VOLUME");
    }
    let pe = verifier(&Fichier(p))?;
    let politique = catalogue::policy(3).map_err(|_| "CATALOGUE")?;
    catalogue::catalog(&politique).map_err(|_| "CATALOGUE")?;
    dire("NOVA5Z USB verifie | Secure Boot inactif | catalogue verifie | lancement Limine")?;
    transfert(bs, h, disque, pe)
}

#[unsafe(no_mangle)]
pub extern "efiapi" fn efi_main(h: efi::Handle, table: *mut efi::SystemTable) -> efi::Status {
    if let Some(st) = unsafe { table.as_ref() } {
        if let Err(raison) = lancer(st, h) {
            let _ = phase5h::display(st, &format!("NOVA5Z REFUS {raison}"));
        }
    }
    arret()
}
fn arret() -> ! {
    unsafe {
        asm!("cli", "2:", "hlt", "jmp 2b", options(noreturn, nostack));
    }
}
#[panic_handler]
fn panique(_: &PanicInfo) -> ! {
    arret()
}
