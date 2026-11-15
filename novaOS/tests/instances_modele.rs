#[allow(dead_code)]
#[path = "../abi/instances.rs"]
mod instance_abi;
#[allow(dead_code)]
#[path = "../kernel/src/sched/process/instances.rs"]
mod instances;

#[path = "../abi/installation.rs"]
mod installation;

#[test]
fn catalogue_unique_et_droits_bornes() {
    use installation::{COMMANDS, grant};
    let mut ids = std::collections::BTreeSet::new();
    for &(name, id) in COMMANDS {
        assert!(ids.insert(id), "identité réutilisée {name}");
        if name != "nsh" {
            assert_eq!(grant(id, 7, 1), Some(false));
        }
        assert_eq!(grant(id, 8, 0), None);
    }
    assert_eq!(grant(0, 0, 0), None);
    assert_eq!(grant(instance_abi::SAMPLE, 0, 0), Some(true));
    assert_eq!(grant(instance_abi::SAMPLE, 0, 1), None);
    assert_eq!(grant(instance_abi::SHELL, 0, 0), None);
}

#[allow(dead_code)]
#[path = "../kernel/src/boot_id.rs"]
mod boot_id;

#[test]
fn source_rejouee_et_recyclage_dans_le_meme_demarrage() {
    const BOOT: [u8; 16] = *b"boot-test-rejoue";
    let mut registre = instances::Registry::<2>::new();
    let ancienne = registre.create(instance_abi::SAMPLE).unwrap();
    let identite = ancienne.identity(Some(BOOT)).unwrap();
    registre.activate(ancienne);
    assert!(registre.stop(ancienne));
    registre.exited(ancienne);
    registre.released(ancienne);

    let courante = registre.create(instance_abi::SAMPLE).unwrap();
    let suivante = courante.identity(Some(BOOT)).unwrap();
    assert_eq!(identite.boot, suivante.boot);
    assert_ne!(identite.instance, suivante.instance);
    assert_eq!(
        identite.generation.checked_add(1),
        Some(suivante.generation)
    );
    registre.activate(courante);
    assert!(!registre.attach(ancienne));
    assert!(registre.attach(courante));

    // Restaurer le registre vide puis rejouer le même lancement avec la même
    // source externe reproduit la trame entière. Ce cas est exclu du profil qualifié.
    let restauree = instances::Registry::<2>::new()
        .create(instance_abi::SAMPLE)
        .unwrap();
    assert_eq!(
        restauree.identity(Some(BOOT)).unwrap().encode(),
        identite.encode()
    );
    assert_ne!(
        restauree.identity(Some(*b"boot-test-frais!")).unwrap(),
        identite
    );
}
