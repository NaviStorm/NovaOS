use novaos_gestionnaire_sim::catalogue::{Bus, Limits, Origin, Policy, Signer, Target};
use sha2::{Digest, Sha256};

pub const MANIFESTE: &[u8] = include_bytes!("paquet/pilote.toml");
pub const SIG: &[u8] = include_bytes!("paquet/pilote.sig");
pub const PKG: &[u8] = include_bytes!("paquet/pilote.pkg01");
pub const PROFIL: &[u8] = include_bytes!("../../user/killer-e2400/profil-cpu.toml");
pub const RECU: &[u8] = include_bytes!("paquet/profil.toml");
pub const SIG_RECU: &[u8] = include_bytes!("paquet/profil.sig");
const DMA: &[&str] = &["tpd:r", "rfd:r", "rrd:w", "tx:r", "rx:w"];

pub fn politique(revoque: bool, plancher: u32) -> Policy {
    let editeur = Signer {
        publisher: "org.novaos.lab".into(),
        key_id: "lab-v1".into(),
        origin: Origin::Laboratoire,
        manifest_key: *include_bytes!("../../libs/catalogue-pilotes/vecteurs/pilote.pub"),
        elf_key: *include_bytes!("../../libs/catalogue-pilotes/vecteurs/elf.pub"),
        first: 1,
        last: 4,
        priority: 1,
        revoked: revoque,
    };
    Policy::new(
        1,
        Origin::Laboratoire,
        plancher,
        vec![editeur],
        *include_bytes!("../../libs/catalogue-pilotes/vecteurs/systeme.pub"),
    )
    .unwrap()
}
pub fn cible() -> Target<'static> {
    Target {
        revision: 1,
        profile: Some(Sha256::digest(PROFIL).into()),
        attachment: "dev-007",
        bus: Bus::Pci {
            vendor: Some(0x1969),
            device: Some(0xe0a1),
            subsystem_vendor: Some(0x1462),
            subsystem_device: Some(0x115a),
            revision: Some(0x10),
            class_base: Some(2),
            subclass: Some(0),
        },
        class: "ethernet",
        isa: "x86_64",
        os: [0, 20, 0],
        abi: 1,
        features: &["simulation-e2400-5a"],
        caps: &["tx", "rx"],
        dma: DMA,
        mmio: &[],
        irq: &[],
        allow_class_match: false,
        limits: Limits {
            private_pages: 64,
            roots: 0,
            handles: 2,
            boxes: 0,
            pending: 8,
            dma_pages: 259,
            queues: 2,
            entries: 256,
            irq_routes: 0,
        },
    }
}
