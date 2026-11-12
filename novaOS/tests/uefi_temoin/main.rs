//! Témoin de banc sur PL011. Charge le Limine original du même support via UEFI.
#![no_std]
#![no_main]

use core::{ffi::c_void, ptr};
use r_efi::{
    efi,
    protocols::{device_path, device_path_utilities, loaded_image},
};

// GUID EDK II MdeModulePkg/Include/Protocol/IoMmu.h, distinct du contrôleur PCI.
const IO_MMU: efi::Guid = efi::Guid::from_fields(
    0x4e939de9,
    0xd948,
    0x4b0f,
    0x88,
    0xed,
    &[0xe6, 0xe1, 0xce, 0x51, 0x7c, 0x1e],
);

fn serie(message: &[u8]) {
    // Sûreté : le témoin vise uniquement le PL011 de virt sous EDK2 ArmVirt.
    // Accès MMIO alignés à FR et DR, identité UEFI, sans allocation ni appel EFI
    // dans la notification de sortie. Une UART saturée ne bloque pas le firmware.
    unsafe {
        for &octet in message {
            if !(0..65536).any(|_| (0x09000018 as *const u32).read_volatile() & 32 == 0) {
                return;
            }
            (0x09000000 as *mut u32).write_volatile(u32::from(octet));
        }
    }
}

extern "efiapi" fn sortie(_: efi::Event, _: *mut c_void) {
    serie(b"OBS EFI notification ExitBootServices\r\n");
}

#[repr(C)]
struct Chemin<const N: usize> {
    en_tete: device_path::Protocol,
    nom: [u16; N],
}

// Témoin indépendant du noyau. Après une première sortie refusée, limiter
// les appels aux services mémoire et à la nouvelle tentative de sortie.
fn verifier_mapkey(h: efi::Handle, bs: &efi::BootServices) -> ! {
    #[repr(align(8))]
    struct Carte([u8; 32768]);
    static mut CARTE: Carte = Carte([0; 32768]);
    // Sûreté : appel unique sur le BSP. La carte statique est alignée et reste
    // vivante après la sortie. Les marqueurs série ne dépendent pas de l’UEFI.
    unsafe {
        assert_eq!(
            (bs.exit_boot_services)(h, usize::MAX),
            efi::Status::INVALID_PARAMETER
        );
        serie(b"OBS EFI MapKey refusee\r\n");
        let mut taille = 32768;
        let (mut cle, mut pas, mut version) = (0, 0, 0);
        assert_eq!(
            (bs.get_memory_map)(
                &mut taille,
                (&raw mut CARTE.0).cast(),
                &mut cle,
                &mut pas,
                &mut version
            ),
            efi::Status::SUCCESS
        );
        assert_eq!((bs.exit_boot_services)(h, cle), efi::Status::SUCCESS);
    }
    serie(b"OBS EFI seconde sortie reussie\r\n");
    loop {
        core::hint::spin_loop();
    }
}

fn lancer(h: efi::Handle, bs: &efi::BootServices) -> efi::Status {
    let mut interface = ptr::null_mut();
    let mut guid = IO_MMU;
    // Sûreté : EFI initialise uniquement le paramètre de sortie ; la sonde
    // n'appelle aucune méthode Map/Unmap et ne revendique aucun périphérique.
    let s = unsafe { (bs.locate_protocol)(&mut guid, ptr::null_mut(), &mut interface) };
    match s {
        efi::Status::SUCCESS if !interface.is_null() => serie(b"OBS EFI IoMmu present\r\n"),
        efi::Status::NOT_FOUND => serie(b"OBS EFI IoMmu absent\r\n"),
        _ => return efi::Status::DEVICE_ERROR,
    }

    if cfg!(feature = "mapkey-lab") {
        verifier_mapkey(h, bs);
    }

    let mut image = ptr::null_mut();
    let mut guid = loaded_image::PROTOCOL_GUID;
    // Sûreté : h est le handle de notre image fourni par le point d'entrée EFI.
    let s = unsafe { (bs.handle_protocol)(h, &mut guid, &mut image) };
    if s.is_error() {
        return s;
    }
    // Sûreté : SUCCESS et le GUID LoadedImage garantissent ce type et sa durée de vie.
    let disque = unsafe { (*image.cast::<loaded_image::Protocol>()).device_handle };
    let mut base = ptr::null_mut();
    let mut guid = device_path::PROTOCOL_GUID;
    // Sûreté : le firmware conserve le DevicePath du support de l'image.
    let s = unsafe { (bs.handle_protocol)(disque, &mut guid, &mut base) };
    if s.is_error() {
        return s;
    }
    let mut guid = device_path_utilities::PROTOCOL_GUID;
    // Sûreté : LocateProtocol renvoie l'interface correspondant au GUID demandé.
    let s = unsafe { (bs.locate_protocol)(&mut guid, ptr::null_mut(), &mut interface) };
    if s.is_error() {
        return s;
    }
    let nom = (*b"\\EFI\\BOOT\\NOVALDR.EFI\0").map(u16::from);
    let node = Chemin {
        en_tete: device_path::Protocol {
            r#type: device_path::TYPE_MEDIA,
            sub_type: device_path::Media::SUBTYPE_FILE_PATH,
            length: (4u16 + 2 * nom.len() as u16).to_le_bytes(),
        },
        nom,
    };
    // Sûreté : le protocole est validé, le nœud FILEPATH et son NUL sont
    // contigus. Le chemin alloué est libéré après le LoadImage synchrone.
    let path = unsafe {
        ((*interface.cast::<device_path_utilities::Protocol>()).append_device_node)(
            base.cast(),
            &node.en_tete,
        )
    };
    if path.is_null() {
        return efi::Status::OUT_OF_RESOURCES;
    }
    let mut enfant = ptr::null_mut();
    // Sûreté : le chemin appartient au pool EFI. Le firmware lit et vérifie le
    // PE via son chemin normal sur le support ; aucun tampon d'image substitué.
    let s = unsafe {
        let s = (bs.load_image)(false.into(), h, path, ptr::null_mut(), 0, &mut enfant);
        (bs.free_pool)(path.cast());
        s
    };
    if s.is_error() {
        return s;
    }

    let mut evt = ptr::null_mut();
    let group = efi::EVENT_GROUP_EXIT_BOOT_SERVICES;
    // Sûreté : la fonction de notification reste chargée pendant StartImage.
    // Elle ne touche qu'à la série, même si le firmware répète une tentative.
    let s = unsafe {
        (bs.create_event_ex)(
            efi::EVT_NOTIFY_SIGNAL,
            efi::TPL_NOTIFY,
            Some(sortie),
            ptr::null(),
            &group,
            &mut evt,
        )
    };
    if s.is_error() {
        // Sûreté : l'enfant a été chargé et n'a pas encore démarré.
        unsafe { (bs.unload_image)(enfant) };
        return s;
    }
    serie(b"OBS EFI StartImage Limine\r\n");
    // Sûreté : en cas de retour, les services sont encore actifs. Fermer
    // l'événement évite un rappel sur une image déchargée.
    unsafe {
        let s = (bs.start_image)(enfant, ptr::null_mut(), ptr::null_mut());
        (bs.close_event)(evt);
        s
    }
}

// Sûreté : symbole attendu par le chargeur PE/COFF AArch64, ABI UEFI.
#[unsafe(no_mangle)]
extern "efiapi" fn efi_main(h: efi::Handle, st: *mut efi::SystemTable) -> efi::Status {
    // Sûreté : EDK2 appelle cette image avant ExitBootServices, avec une
    // table système et une table de services valides pour la durée de l'appel.
    let s = lancer(h, unsafe { &*(*st).boot_services });
    serie(b"OBS EFI retour inattendu\r\n");
    s
}

#[panic_handler]
fn panique(_: &core::panic::PanicInfo) -> ! {
    serie(b"OBS EFI panique\r\n");
    loop {
        core::hint::spin_loop();
    }
}
