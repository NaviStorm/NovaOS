//! Double de l’interface DXE : erreurs des propriétés et vecteur indépendant avec graine fictive.
use super::*;
use core::ffi::CStr;

#[repr(C)]
struct Firmware {
    protocol: FdtClient,
    model: &'static [u8],
    compatible: &'static [u8],
    chosen_node: i32,
    seed: Option<&'static [u8]>,
}
impl Firmware {
    fn ordinary() -> Self {
        Self {
            protocol: FdtClient {
                property,
                unused: [0; 7],
                chosen,
            },
            model: b"linux,dummy-virt\0",
            compatible: b"linux,dummy-virt\0",
            chosen_node: 20,
            seed: Some(&[
                0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22,
                23, 24, 25, 26, 27, 28, 29, 30, 31,
            ]),
        }
    }
}

unsafe extern "efiapi" fn property(
    this: *const FdtClient,
    node: i32,
    name: *const u8,
    data: *mut *const u8,
    size: *mut u32,
) -> efi::Status {
    let firmware = unsafe { &*this.cast::<Firmware>() };
    let name = unsafe { CStr::from_ptr(name.cast()) }.to_bytes();
    let value = match (node, name) {
        (0, b"compatible") => Some(firmware.compatible),
        (0, b"model") => Some(firmware.model),
        (n, b"rng-seed") if n == firmware.chosen_node => firmware.seed,
        _ => None,
    };
    let Some(value) = value else {
        return efi::Status::NOT_FOUND;
    };
    unsafe {
        *data = value.as_ptr();
        *size = value.len() as u32;
    }
    efi::Status::SUCCESS
}
unsafe extern "efiapi" fn chosen(this: *const FdtClient, node: *mut i32) -> efi::Status {
    unsafe {
        *node = (*this.cast::<Firmware>()).chosen_node;
    }
    efi::Status::SUCCESS
}

#[test]
fn vecteur_sha256_avec_domaine_et_ordre_des_octets() {
    let expected = [
        0x07, 0x6f, 0x88, 0xf7, 0xe8, 0x9f, 0x0a, 0x0a, 0x9b, 0xa6, 0x67, 0x06, 0x70, 0xa8, 0x0a,
        0xb4,
    ];
    assert_eq!(Firmware::ordinary().protocol.identifier(), Some(expected));
}

#[test]
fn refus_source_absente_tronquee_uniforme_ou_profil_non_reconnu() {
    for seed in [
        None,
        Some(&[1; 31][..]),
        Some(&[2; 33][..]),
        Some(&[0xff; 32][..]),
        Some(&[0; 32][..]),
    ] {
        assert_eq!(
            Firmware {
                seed,
                ..Firmware::ordinary()
            }
            .protocol
            .identifier(),
            None
        );
    }
    assert_eq!(
        Firmware {
            model: b"modele-inconnu!!\0",
            ..Firmware::ordinary()
        }
        .protocol
        .identifier(),
        None
    );
    assert_eq!(
        Firmware {
            compatible: b"linux,dummy-virt",
            ..Firmware::ordinary()
        }
        .protocol
        .identifier(),
        None
    );
    assert_eq!(
        Firmware {
            chosen_node: 0,
            ..Firmware::ordinary()
        }
        .protocol
        .identifier(),
        None
    );
}
