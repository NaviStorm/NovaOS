use alloc::vec;
use sha2::Digest;
extern crate alloc;
#[cfg(test)]
use ed25519_dalek::{Signer as _, SigningKey};
use novaos_catalogue::*;
pub const RAW: &str = include_str!("../../vecteurs/pilote.toml");
pub const SIG: &[u8] = include_bytes!("../../vecteurs/pilote.sig");
pub const ARM: &[u8] = include_bytes!("../../vecteurs/aarch64.pkg01");
pub const X86: &[u8] = include_bytes!("../../vecteurs/x86_64.pkg01");
pub const SYS: [u8; 32] = *include_bytes!("../../vecteurs/systeme.pub");
pub fn objects() -> [Blob<'static>; 2] {
    [
        Blob {
            isa: "aarch64",
            bytes: ARM,
        },
        Blob {
            isa: "x86_64",
            bytes: X86,
        },
    ]
}
pub fn signer() -> novaos_catalogue::Signer {
    novaos_catalogue::Signer {
        publisher: "org.novaos.lab".into(),
        key_id: "lab-v1".into(),
        origin: Origin::Laboratoire,
        manifest_key: *include_bytes!("../../vecteurs/pilote.pub"),
        elf_key: *include_bytes!("../../vecteurs/elf.pub"),
        first: 1,
        last: 4,
        priority: 1,
        revoked: false,
    }
}
pub fn policy() -> Policy {
    let mut p = Policy::new(1, Origin::Laboratoire, 1, vec![signer()], SYS).unwrap();
    p.add_resource(
        include_bytes!("../../vecteurs/profil.toml"),
        include_bytes!("../../vecteurs/profil.bin"),
        include_bytes!("../../vecteurs/profil.sig"),
    )
    .unwrap();
    p
}
#[cfg(test)]
pub fn sign(raw: &[u8], seed: u8) -> [u8; 64] {
    let mut msg = b"NovaOS/driver-package/v1\0".to_vec();
    msg.extend_from_slice(&(raw.len() as u32).to_le_bytes());
    msg.extend_from_slice(raw);
    SigningKey::from_bytes(&[seed; 32]).sign(&msg).to_bytes()
}
pub fn target(isa: &str) -> Target<'_> {
    Target {
        revision: 9,
        profile: Some(sha2::Sha256::digest(include_bytes!("../../vecteurs/profil.bin")).into()),
        attachment: "port-01",
        class: "ethernet",
        isa,
        os: [0, 20, 0],
        abi: 1,
        bus: Bus::Pci {
            vendor: Some(0x1af4),
            device: Some(0x1041),
            subsystem_vendor: None,
            subsystem_device: None,
            revision: None,
            class_base: None,
            subclass: None,
        },
        caps: &["rx"],
        features: &["courtier"],
        dma: &["rx:w"],
        mmio: &["notification"],
        irq: &["intx"],
        allow_class_match: false,
        limits: Limits {
            private_pages: 32,
            roots: 1,
            handles: 6,
            boxes: 0,
            pending: 8,
            dma_pages: 22,
            queues: 2,
            entries: 16,
            irq_routes: 1,
        },
    }
}
