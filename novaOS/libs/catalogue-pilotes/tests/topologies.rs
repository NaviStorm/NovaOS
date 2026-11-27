use novaos_catalogue::Bus;

fn usb(parent: Option<&str>, interface: Option<u8>) -> Bus {
    Bus::Usb {
        vendor: Some(0x1234),
        product: Some(0x5678),
        configuration: Some(1),
        interface,
        class_base: Some(3),
        subclass: None,
        protocol: None,
        parent: parent.map(str::to_owned),
    }
}
#[test]
fn interface_usb_attachee_au_parent() {
    let rule = usb(Some("port-a"), Some(2));
    assert!(rule.specificity(&usb(Some("port-a"), Some(2))).is_some());
    assert_eq!(rule.specificity(&usb(Some("port-b"), Some(2))), None);
    assert_eq!(rule.specificity(&usb(None, Some(2))), None);
    assert_eq!(rule.specificity(&usb(Some("port-a"), Some(1))), None);
}
#[test]
fn plateforme_et_bus_distincts() {
    let a = Bus::Platform {
        provider: "acpi".into(),
        firmware_id: "nva01".into(),
    };
    assert!(a.specificity(&a).is_some());
    assert_eq!(
        a.specificity(&Bus::Platform {
            provider: "dt".into(),
            firmware_id: "nva01".into()
        }),
        None
    );
    assert_eq!(a.specificity(&usb(None, None)), None);
}
