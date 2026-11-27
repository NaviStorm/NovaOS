extern crate alloc;
#[cfg(feature = "diagnostic-5i")]
#[path = "../../../kernel/src/hardware/acpi.rs"]
#[allow(dead_code, unexpected_cfgs)]
mod acpi;
#[path = "../catalogue.rs"]
mod catalogue;
#[path = "../firmware.rs"]
#[allow(dead_code)]
mod firmware;
#[cfg(feature = "diagnostic-5i")]
#[path = "../journal5i.rs"]
#[allow(dead_code)]
mod journal5i;
#[cfg(feature = "diagnostic-5h")]
#[path = "../phase5h.rs"]
#[allow(dead_code)]
mod phase5h;
#[cfg(feature = "diagnostic-5i")]
#[path = "../phase5i.rs"]
#[allow(dead_code)]
mod phase5i;
#[cfg(feature = "diagnostic-5i")]
#[path = "../topologie5i.rs"]
mod topologie5i;

#[cfg(test)]
mod tests {
    use super::catalogue;
    use novaos_catalogue::Error;
    #[test]
    fn plan_ambigu_inconnu_et_plancher() {
        let policy = catalogue::policy(3).unwrap();
        let c = catalogue::catalog(&policy).unwrap();
        assert!(
            catalogue::select(&c, "acpi", "ioapic", "acpi.0")
                .unwrap()
                .is_some()
        );
        assert_eq!(catalogue::select(&c, "acpi", "ecam", "acpi.1"), Ok(None));
        assert_eq!(
            catalogue::select(&c, "acpi", "hpet", "acpi.2"),
            Err(Error::Conflict)
        );
        let newer = catalogue::policy(4).unwrap();
        assert!(matches!(catalogue::catalog(&newer), Err(Error::Floor)));
    }
}
