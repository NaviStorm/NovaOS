//! Relais du profil QEMU ArmVirt de confiance, avant Limine et ExitBootServices.
//! La graine appartient au firmware ; seule une identité publique dérivée sort du relais.
#![cfg_attr(not(test), no_std)]
#![cfg_attr(not(test), no_main)]

use core::{ffi::c_void, mem::size_of, ptr};
use r_efi::{
    efi,
    protocols::{device_path, device_path_utilities, loaded_image},
};
use sha2::{Digest, Sha256};

const BOOT_ID: efi::Guid = efi::Guid::from_fields(
    0xe8fd92d6,
    0xfb62,
    0x4aa3,
    0xa6,
    0x67,
    &[0x03, 0x74, 0xba, 0xa5, 0x01, 0xd5],
);
const FDT_CLIENT: efi::Guid = efi::Guid::from_fields(
    0xe11faca0,
    0x4710,
    0x4c8e,
    0xa7,
    0xa2,
    &[0x01, 0xba, 0xa2, 0x59, 0x1b, 0x4c],
);

// ABI privée d’EDK II EmbeddedPkg/Include/Protocol/FdtClient.h, profil ArmVirt
// qualifié seulement. Les sept méthodes intermédiaires ne sont jamais appelées.
#[repr(C)]
struct FdtClient {
    property: unsafe extern "efiapi" fn(
        *const Self,
        i32,
        *const u8,
        *mut *const u8,
        *mut u32,
    ) -> efi::Status,
    unused: [usize; 7],
    chosen: unsafe extern "efiapi" fn(*const Self, *mut i32) -> efi::Status,
}

impl FdtClient {
    fn exact<const N: usize>(&self, node: i32, name: &core::ffi::CStr) -> Option<[u8; N]> {
        let mut data = ptr::null();
        let mut size = 0;
        // Le protocole fournit la plage lisible jusqu’au prochain changement du DT.
        let status =
            unsafe { (self.property)(self, node, name.as_ptr().cast(), &mut data, &mut size) };
        if status != efi::Status::SUCCESS || data.is_null() || size as usize != N {
            return None;
        }
        Some(unsafe { data.cast::<[u8; N]>().read() })
    }

    fn identifier(&self) -> Option<[u8; 16]> {
        // Le profil autorise le DT généré par QEMU virt, pas un DTB fourni par l’opérateur.
        // Les propriétés identifient le profil ; elles n’authentifient pas l’hyperviseur.
        let expected = *b"linux,dummy-virt\0";
        if self.exact(0, c"compatible")? != expected || self.exact(0, c"model")? != expected {
            return None;
        }
        let mut chosen = -1;
        // Si le firmware doit créer /chosen, le nœud vide ne fournit aucune graine.
        if unsafe { (self.chosen)(self, &mut chosen) } != efi::Status::SUCCESS || chosen <= 0 {
            return None;
        }
        let seed: [u8; 32] = self.exact(chosen, c"rng-seed")?;
        if seed.iter().all(|byte| *byte == seed[0]) {
            return None;
        }
        let mut hash = Sha256::new();
        hash.update(b"NovaOS/BootID/v1/QEMU-ArmVirt/chosen/rng-seed\0");
        hash.update(seed);
        let value: [u8; 16] = hash.finalize()[..16].try_into().ok()?;
        (value != [0; 16]).then_some(value)
    }
}

#[repr(C)]
struct BootTable {
    magic: [u8; 8],
    version: u32,
    size: u32,
    id: [u8; 16],
}
const _: () = assert!(size_of::<BootTable>() == 32);

fn protocol(
    bs: &efi::BootServices,
    handle: Option<efi::Handle>,
    mut guid: efi::Guid,
) -> Result<*mut c_void, efi::Status> {
    let mut ptr = ptr::null_mut();
    let status = unsafe {
        if let Some(h) = handle {
            (bs.handle_protocol)(h, &mut guid, &mut ptr)
        } else {
            (bs.locate_protocol)(&mut guid, ptr::null_mut(), &mut ptr)
        }
    };
    if status.is_error() {
        Err(status)
    } else if ptr.is_null() {
        Err(efi::Status::DEVICE_ERROR)
    } else {
        Ok(ptr)
    }
}

fn publish(bs: &efi::BootServices, id: [u8; 16]) -> Option<*mut c_void> {
    let mut buffer = ptr::null_mut();
    // Donnée physique réservée jusqu’à sa lecture par le noyau ; ni le code
    // de cette image, ni une allocation BootServicesData récupérable par Limine.
    let status = unsafe {
        (bs.allocate_pool)(
            efi::RUNTIME_SERVICES_DATA,
            size_of::<BootTable>(),
            &mut buffer,
        )
    };
    if status.is_error() || buffer.is_null() {
        return None;
    }
    let mut guid = BOOT_ID;
    unsafe {
        buffer.cast::<BootTable>().write(BootTable {
            magic: *b"NVBOOT01",
            version: 1u32.to_le(),
            size: 32u32.to_le(),
            id,
        });
        if (bs.install_configuration_table)(&mut guid, buffer).is_error() {
            (bs.free_pool)(buffer);
            return None;
        }
    }
    Some(buffer)
}

#[repr(C)]
struct FileNode<const N: usize> {
    header: device_path::Protocol,
    file: [u16; N],
}

fn limine(h: efi::Handle, bs: &efi::BootServices) -> Result<efi::Status, efi::Status> {
    let image = protocol(bs, Some(h), loaded_image::PROTOCOL_GUID)?;
    let disk = unsafe { (*image.cast::<loaded_image::Protocol>()).device_handle };
    let base = protocol(bs, Some(disk), device_path::PROTOCOL_GUID)?;
    let utilities = protocol(bs, None, device_path_utilities::PROTOCOL_GUID)?;
    let file = (*b"\\EFI\\BOOT\\LIMINE.EFI\0").map(u16::from);
    let node = FileNode {
        header: device_path::Protocol {
            r#type: device_path::TYPE_MEDIA,
            sub_type: device_path::Media::SUBTYPE_FILE_PATH,
            length: (4 + 2 * file.len() as u16).to_le_bytes(),
        },
        file,
    };
    // Même support, chemin absolu fixe, vérification PE habituelle du firmware.
    let path = unsafe {
        ((*utilities.cast::<device_path_utilities::Protocol>()).append_device_node)(
            base.cast(),
            &node.header,
        )
    };
    if path.is_null() {
        return Err(efi::Status::OUT_OF_RESOURCES);
    }
    let mut child = ptr::null_mut();
    let status = unsafe {
        let status = (bs.load_image)(false.into(), h, path, ptr::null_mut(), 0, &mut child);
        (bs.free_pool)(path.cast());
        status
    };
    if status.is_error() {
        // EFI_SECURITY_VIOLATION peut fournir un handle chargé mais non démarrable.
        if !child.is_null() {
            unsafe {
                (bs.unload_image)(child);
            }
        }
        return Err(status);
    }
    Ok(unsafe { (bs.start_image)(child, ptr::null_mut(), ptr::null_mut()) })
}

#[unsafe(no_mangle)]
extern "efiapi" fn efi_main(h: efi::Handle, st: *mut efi::SystemTable) -> efi::Status {
    // Le firmware est de confiance, le relais s’exécute avant ExitBootServices.
    let bs = unsafe { &*(*st).boot_services };
    let mut guid = BOOT_ID;
    // Une table d’une invocation antérieure ne doit jamais servir de repli.
    let cleared = unsafe { (bs.install_configuration_table)(&mut guid, ptr::null_mut()) };
    if cleared.is_error() && cleared != efi::Status::NOT_FOUND {
        return cleared;
    }
    let table = protocol(bs, None, FDT_CLIENT)
        .ok()
        .and_then(|fdt| unsafe { &*fdt.cast::<FdtClient>() }.identifier())
        .and_then(|id| publish(bs, id));
    // Diagnostic sans valeur de la graine ou de l’identité, via la console EFI.
    let console = unsafe { (*st).con_out };
    if !console.is_null() {
        let message: &[u8] = if table.is_some() {
            b"[BootID EFI] QEMU ArmVirt /chosen/rng-seed\r\n"
        } else {
            b"[BootID EFI] source ArmVirt indisponible\r\n"
        };
        let mut text = [0u16; 64];
        for (dest, src) in text.iter_mut().zip(message) {
            *dest = u16::from(*src);
        }
        unsafe {
            ((*console).output_string)(console, text.as_mut_ptr());
        }
    }
    // Même sans source, NovaOS démarre ; le noyau refuse les opérations exigeant un BootID.
    let status = limine(h, bs).unwrap_or_else(|error| error);
    if let Some(buffer) = table {
        // Un retour de StartImage implique des services actifs. Ne pas restituer
        // la mémoire si le retrait de son pointeur n’est pas confirmé.
        if !unsafe { (bs.install_configuration_table)(&mut guid, ptr::null_mut()) }.is_error() {
            unsafe {
                (bs.free_pool)(buffer);
            }
        }
    }
    status
}

#[cfg(not(test))]
#[panic_handler]
fn panique(_: &core::panic::PanicInfo) -> ! {
    loop {
        core::hint::spin_loop();
    }
}

#[cfg(test)]
mod tests;
