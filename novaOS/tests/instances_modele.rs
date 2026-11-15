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
