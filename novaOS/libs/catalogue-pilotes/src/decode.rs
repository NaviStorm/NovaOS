use super::*;
use crate::{
    bounded::Budget,
    document::{self, Value},
};
use alloc::borrow::Cow;
use toml_parser::decoder::ScalarKind;

pub(crate) trait Read: Sized {
    fn read(v: Value<'_>, b: &mut Budget) -> Result<Self>;
}
pub(crate) fn decode<T: Read>(raw: &[u8], limit: usize) -> Result<T> {
    let mut b = Budget::new(limit);
    T::read(Value::Table(document::parse(raw, &mut b)?), &mut b)
}
fn text(v: Value<'_>) -> Result<Cow<'_, str>> {
    if let Value::Scalar(ScalarKind::String, s) = v {
        Ok(s)
    } else {
        Err(Error::Format)
    }
}
impl Read for String {
    fn read(v: Value<'_>, b: &mut Budget) -> Result<Self> {
        match text(v)? {
            Cow::Borrowed(s) => b.text(s),
            Cow::Owned(s) if s.capacity() == s.len() => Ok(s),
            Cow::Owned(s) => b.text(&s),
        }
    }
}
impl Read for bool {
    fn read(v: Value<'_>, _: &mut Budget) -> Result<Self> {
        if let Value::Scalar(ScalarKind::Boolean(x), _) = v {
            Ok(x)
        } else {
            Err(Error::Format)
        }
    }
}
macro_rules! number {
    ($t:ty) => {
        impl Read for $t {
            fn read(v: Value<'_>, _: &mut Budget) -> Result<Self> {
                if let Value::Scalar(ScalarKind::Integer(r), s) = v {
                    if s.as_ref() == "-0" {
                        return Ok(0);
                    }
                    <$t>::from_str_radix(&s, r.value()).map_err(|_| Error::Format)
                } else {
                    Err(Error::Format)
                }
            }
        }
    };
}
number!(u8);
number!(u16);
number!(u32);
number!(u64);
impl<const N: usize> Read for [u8; N] {
    fn read(v: Value<'_>, _: &mut Budget) -> Result<Self> {
        let s = text(v)?;
        let mut bytes = [0; N];
        if s.len() != 2 * N
            || !s
                .bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
        {
            return Err(Error::Format);
        }
        for (i, out) in bytes.iter_mut().enumerate() {
            *out = u8::from_str_radix(&s[2 * i..2 * i + 2], 16).map_err(|_| Error::Format)?;
        }
        if bytes == [0; N] {
            return Err(Error::Format);
        }
        Ok(bytes)
    }
}
impl Read for Version {
    fn read(v: Value<'_>, b: &mut Budget) -> Result<Self> {
        let Value::Array(values) = v else {
            return Err(Error::Format);
        };
        if values.len() != 3 {
            return Err(Error::Format);
        }
        let mut out = [0; 3];
        for (slot, v) in out.iter_mut().zip(values) {
            *slot = u32::read(v, b)?;
        }
        Ok(out)
    }
}
impl<T: Read> Read for Vec<T> {
    fn read(v: Value<'_>, b: &mut Budget) -> Result<Self> {
        let (Value::Array(values) | Value::Tables(values)) = v else {
            return Err(Error::Format);
        };
        if values.len() > 32 {
            return Err(Error::Limit);
        }
        let mut out = Vec::new();
        b.reserve(&mut out, values.len())?;
        for v in values {
            out.push(T::read(v, b)?);
        }
        Ok(out)
    }
}
// Le schéma énumère les champs obligatoires. finish refuse tout champ restant.
macro_rules! record {
    ($ty:ident { $($field:ident $(=> $key:literal)?),+ $(,)? }) => {
        impl Read for $ty {
            fn read(value: Value<'_>, budget: &mut Budget) -> Result<Self> {
                let Value::Table(mut table) = value else { return Err(Error::Format); };
                let record = Self {
                    $( $field: Read::read(table.take(record!(@key $field $($key)?))?, budget)?, )+
                };
                table.finish()?;
                Ok(record)
            }
        }
    };
    (@key $field:ident $key:literal) => { $key };
    (@key $field:ident) => { stringify!($field) };
}
record!(Manifest {
    schema, origin, publisher, signing_key_id, security_epoch,
    elf_package_id, elf_package_revision, provides, id, version, class, compat,
    image, matches => "match", required_caps, optional_caps, dependency, hardware, quotas
});
record!(Compat {
    architectures,
    abi_major,
    os_min,
    os_before,
    required_features
});
record!(Image {
    member,
    isa,
    elf_sha256,
    package_sha256
});
record!(Hardware {
    dma,
    mmio,
    irq,
    max_irq_routes,
    dma_pages,
    queues,
    entries_per_queue
});
record!(Quotas {
    private_pages,
    ipc_roots_created,
    ipc_boxes_created,
    ipc_handles,
    pending_operations
});
record!(Dependency {
    id,
    kind,
    min,
    before,
    sha256,
    required
});
use crate::admission::Record;
record!(Record {
    origin,
    policy_revision,
    security_epoch,
    id,
    kind,
    version,
    sha256,
    dependencies
});
impl Read for Origin {
    fn read(v: Value<'_>, _: &mut Budget) -> Result<Self> {
        match &*text(v)? {
            "laboratoire" => Ok(Self::Laboratoire),
            "distribution" => Ok(Self::Distribution),
            _ => Err(Error::Format),
        }
    }
}
impl Read for Kind {
    fn read(v: Value<'_>, _: &mut Budget) -> Result<Self> {
        match &*text(v)? {
            "system-profile" => Ok(Self::SystemProfile),
            "controller" => Ok(Self::Controller),
            "service" => Ok(Self::Service),
            _ => Err(Error::Format),
        }
    }
}
impl Read for Bus {
    fn read(v: Value<'_>, b: &mut Budget) -> Result<Self> {
        let Value::Table(mut t) = v else {
            return Err(Error::Format);
        };
        macro_rules! optional {
            ($variant:ident { $($field:ident),+ }) => {
                Self::$variant {
                    $( $field: t.optional(stringify!($field)).map(|v| Read::read(v, b)).transpose()?, )+
                }
            };
        }
        let bus = match &*text(t.take("bus")?)? {
            "pci" => optional!(Pci {
                vendor,
                device,
                subsystem_vendor,
                subsystem_device,
                revision,
                class_base,
                subclass
            }),
            "usb" => optional!(Usb {
                vendor,
                product,
                configuration,
                interface,
                class_base,
                subclass,
                protocol,
                parent
            }),
            "platform" => Self::Platform {
                provider: String::read(t.take("provider")?, b)?,
                firmware_id: String::read(t.take("firmware_id")?, b)?,
            },
            // Un bus futur reste inéligible, quels que soient ses critères.
            _ => {
                Value::Table(t).check_opaque()?;
                return Ok(Self::Unknown);
            }
        };
        t.finish()?;
        Ok(bus)
    }
}

#[cfg(test)]
mod tests;
