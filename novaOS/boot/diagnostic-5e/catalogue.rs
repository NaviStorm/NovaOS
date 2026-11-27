use alloc::vec;
use novaos_catalogue::{Blob, Bus, Catalog, Error, Limits, Origin, Policy, Signer, Target};
macro_rules! public {
    ($file:literal) => {
        include_bytes!(concat!("../../libs/catalogue-pilotes/vecteurs/", $file))
    };
}
pub fn policy(floor: u32) -> Result<Policy, Error> {
    Policy::new(
        1,
        Origin::Laboratoire,
        floor,
        vec![Signer {
            publisher: "org.novaos.lab".into(),
            key_id: "lab-v1".into(),
            origin: Origin::Laboratoire,
            manifest_key: *public!("pilote.pub"),
            elf_key: *public!("elf.pub"),
            first: 3,
            last: 3,
            revoked: false,
            priority: 1,
        }],
        *public!("systeme.pub"),
    )
}
macro_rules! fixture {
    ($id:literal) => {
        (
            &include_bytes!(concat!("vecteurs/", $id, ".toml"))[..],
            &include_bytes!(concat!("vecteurs/", $id, ".sig"))[..],
        )
    };
}
pub fn catalog(policy: &Policy) -> Result<Catalog<'_>, Error> {
    let mut cat = Catalog::new(policy);
    for (raw, sig) in [fixture!("ioapic"), fixture!("hpet-a"), fixture!("hpet-b")] {
        cat.admit(
            raw,
            sig,
            &[Blob {
                isa: "x86_64",
                bytes: public!("x86_64.pkg01"),
            }],
        )?;
    }
    Ok(cat)
}
pub fn select(
    cat: &Catalog,
    provider: &str,
    device: &str,
    attachment: &str,
) -> Result<Option<[u8; 32]>, Error> {
    let target = Target {
        revision: 1,
        profile: None,
        attachment,
        bus: Bus::Platform {
            provider: provider.into(),
            firmware_id: device.into(),
        },
        class: "diagnostic",
        isa: "x86_64",
        os: [0, 20, 0],
        abi: 1,
        features: &[],
        caps: &[],
        dma: &[],
        mmio: &[],
        irq: &[],
        limits: Limits {
            private_pages: 0,
            roots: 0,
            handles: 0,
            boxes: 0,
            pending: 0,
            dma_pages: 0,
            queues: 0,
            entries: 0,
            irq_routes: 0,
        },
        allow_class_match: false,
    };
    match cat.select(&target, None) {
        Ok(plan) => Ok(Some(plan.manifest_digest())),
        Err(Error::NoMatch) => Ok(None),
        Err(e) => Err(e),
    }
}
