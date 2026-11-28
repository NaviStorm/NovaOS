extern crate alloc;

#[path = "../../diagnostic-5e/catalogue.rs"]
mod catalogue;
#[path = "../selection.rs"]
mod selection;
use novaos_catalogue::{Blob, Bus, Catalog, Error};

#[test]
fn classes_des_controleurs_et_fonction_inconnue() {
    for (code, groupe) in [
        (0x010601, 0),
        (0x020000, 1),
        (0x030000, 2),
        (0x0c0330, 3),
        (0x0c0500, 4),
        (0xff0000, 4),
    ] {
        assert_eq!(selection::categorie(code), groupe);
    }
}

#[test]
fn selection_du_catalogue_signe_et_refus_du_conflit() {
    let policy = catalogue::policy(3).unwrap();
    let cat = catalogue::catalog(&policy).unwrap();
    assert!(
        catalogue::select(&cat, "acpi", "ioapic", "acpi.ioapic.0")
            .unwrap()
            .is_some()
    );
    assert_eq!(
        catalogue::select(&cat, "acpi", "hpet", "acpi.hpet.0"),
        Err(Error::Conflict)
    );
    let inconnu = Bus::Pci {
        vendor: Some(0x1234),
        device: Some(0xfedc),
        subsystem_vendor: None,
        subsystem_device: None,
        revision: None,
        class_base: Some(0xff),
        subclass: Some(0),
    };
    assert!(
        selection::select_pci(&cat, inconnu, "pci.0.0.1.0")
            .unwrap()
            .is_none()
    );
}

#[test]
fn rejet_des_objets_internes_alteres() {
    let policy = catalogue::policy(3).unwrap();
    let m = include_bytes!("../../diagnostic-5e/vecteurs/ioapic.toml");
    let sig = include_bytes!("../../diagnostic-5e/vecteurs/ioapic.sig");
    let elf = include_bytes!("../../../libs/catalogue-pilotes/vecteurs/x86_64.pkg01");
    let mut faux_sig = *sig;
    faux_sig[0] ^= 1;
    let mut cat = Catalog::new(&policy);
    assert!(
        cat.admit(
            m,
            &faux_sig,
            &[Blob {
                isa: "x86_64",
                bytes: elf
            }]
        )
        .is_err()
    );
    let mut faux_elf = *elf;
    let dernier = faux_elf.len() - 1;
    faux_elf[dernier] ^= 1;
    assert!(
        cat.admit(
            m,
            sig,
            &[Blob {
                isa: "x86_64",
                bytes: &faux_elf
            }]
        )
        .is_err()
    );
    assert!(
        catalogue::select(&cat, "acpi", "ioapic", "acpi.ioapic.0")
            .unwrap()
            .is_none()
    );
}
