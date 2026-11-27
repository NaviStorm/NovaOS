use alloc::string::String;
use serde::Deserialize;

/// Un champ absent dans l'inventaire est inconnu. Un critère signé qui le nomme échoue.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(tag = "bus", rename_all = "lowercase", deny_unknown_fields)]
pub enum Bus {
    Pci {
        vendor: Option<u16>,
        device: Option<u16>,
        subsystem_vendor: Option<u16>,
        subsystem_device: Option<u16>,
        revision: Option<u8>,
        class_base: Option<u8>,
        subclass: Option<u8>,
    },
    Usb {
        vendor: Option<u16>,
        product: Option<u16>,
        configuration: Option<u8>,
        interface: Option<u8>,
        class_base: Option<u8>,
        subclass: Option<u8>,
        protocol: Option<u8>,
        parent: Option<String>,
    },
    Platform {
        provider: String,
        firmware_id: String,
    },
    #[serde(other)]
    Unknown,
}
fn fits<T: PartialEq>(want: &Option<T>, got: &Option<T>) -> bool {
    want.is_none() || want == got
}
impl Bus {
    pub(crate) fn valid(&self) -> bool {
        match self {
            Self::Unknown => false,
            Self::Pci {
                vendor,
                device,
                subsystem_vendor,
                subsystem_device,
                revision,
                class_base,
                subclass,
            } => {
                vendor.is_some() == device.is_some()
                    && subsystem_vendor.is_some() == subsystem_device.is_some()
                    && (vendor.is_some() || class_base.is_some())
                    && (vendor.is_some() || (revision.is_none() && subsystem_vendor.is_none()))
                    && (subclass.is_none() || class_base.is_some())
            }
            Self::Usb {
                vendor,
                product,
                class_base,
                subclass,
                protocol,
                configuration,
                interface,
                parent,
            } => {
                vendor.is_some() == product.is_some()
                    && (vendor.is_some() || class_base.is_some())
                    && (subclass.is_none() || class_base.is_some())
                    && (protocol.is_none() || subclass.is_some())
                    && (interface.is_none() || configuration.is_some())
                    && parent.as_ref().is_none_or(|s| crate::name(s))
            }
            Self::Platform {
                provider,
                firmware_id,
            } => crate::name(provider) && crate::name(firmware_id),
        }
    }
    pub fn specificity(&self, actual: &Self) -> Option<u8> {
        if !self.valid() || !actual.valid() {
            return None;
        }
        match (self, actual) {
            (
                Self::Pci {
                    vendor,
                    device,
                    subsystem_vendor,
                    subsystem_device,
                    revision,
                    class_base,
                    subclass,
                },
                Self::Pci {
                    vendor: v,
                    device: d,
                    subsystem_vendor: sv,
                    subsystem_device: sd,
                    revision: r,
                    class_base: c,
                    subclass: s,
                },
            ) if fits(vendor, v)
                && fits(device, d)
                && fits(subsystem_vendor, sv)
                && fits(subsystem_device, sd)
                && fits(revision, r)
                && fits(class_base, c)
                && fits(subclass, s) =>
            {
                Some(
                    8 * u8::from(vendor.is_some())
                        + 2 * u8::from(subsystem_device.is_some())
                        + u8::from(revision.is_some())
                        + u8::from(subclass.is_some()),
                )
            }
            (
                Self::Usb {
                    vendor,
                    product,
                    configuration,
                    interface,
                    class_base,
                    subclass,
                    protocol,
                    parent,
                },
                Self::Usb {
                    vendor: v,
                    product: p,
                    configuration: c,
                    interface: i,
                    class_base: b,
                    subclass: s,
                    protocol: t,
                    parent: a,
                },
            ) if fits(vendor, v)
                && fits(product, p)
                && fits(configuration, c)
                && fits(interface, i)
                && fits(class_base, b)
                && fits(subclass, s)
                && fits(protocol, t)
                && fits(parent, a) =>
            {
                Some(
                    8 * u8::from(vendor.is_some())
                        + [
                            configuration.is_some(),
                            interface.is_some(),
                            class_base.is_some(),
                            subclass.is_some(),
                            protocol.is_some(),
                            parent.is_some(),
                        ]
                        .into_iter()
                        .filter(|b| *b)
                        .count() as u8,
                )
            }
            (Self::Platform { .. }, Self::Platform { .. }) if self == actual => Some(8),
            _ => None,
        }
    }
}
