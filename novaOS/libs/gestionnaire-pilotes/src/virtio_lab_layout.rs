//! Géométrie des deux anciens profils de laboratoire 4D/4F. Les références de
//! profil sont épinglées comme dans user/gestionnaire-4d/preparer.py. Aucun accès HAL.
use crate::{
    catalogue::{Kind, Plan},
    dma::Region,
};
const ARM: [u8; 32] = [
    0x39, 0x5b, 0xdf, 0xcc, 0x84, 0x51, 0x25, 0xee, 0xcf, 0xd9, 0x6b, 0x6e, 0x99, 0xe1, 0xcf, 0x46,
    0x4e, 0x05, 0x1b, 0x84, 0xe0, 0x1a, 0x0f, 0xa3, 0xbc, 0x7d, 0xdc, 0xe2, 0xf5, 0x3a, 0x76, 0x2c,
];
const Q35: [u8; 32] = [
    0x3a, 0xdd, 0xcc, 0xb2, 0xd6, 0xfa, 0xa7, 0x2a, 0x19, 0x4a, 0x27, 0x8c, 0x33, 0x9e, 0xb5, 0xe0,
    0xdf, 0x76, 0x30, 0x71, 0xb7, 0x7b, 0xed, 0xd9, 0x0f, 0x3d, 0xdc, 0xd5, 0xca, 0x9c, 0xcb, 0xd7,
];
pub(crate) const REGIONS: [Region; 8] = [
    Region {
        name: "descripteurs:r",
        frames: 1,
    },
    Region {
        name: "disponibles:r",
        frames: 1,
    },
    Region {
        name: "utilises:w",
        frames: 1,
    },
    Region {
        name: "descripteurs:r",
        frames: 1,
    },
    Region {
        name: "disponibles:r",
        frames: 1,
    },
    Region {
        name: "utilises:w",
        frames: 1,
    },
    Region {
        name: "rx:w",
        frames: 8,
    },
    Region {
        name: "tx:r",
        frames: 8,
    },
];
fn broker(driver: &str, profile: &[u8; 32]) -> Option<&'static str> {
    Some(match (driver, *profile) {
        ("org.novaos.lab.virtio-net-arm", ARM) => "org.novaos.lab.courtier-arm",
        ("org.novaos.lab.virtio-net-q35", Q35) => "org.novaos.lab.courtier-q35",
        _ => return None,
    })
}

pub(crate) fn lookup(p: &Plan, profile: &[u8; 32]) -> Option<&'static [Region]> {
    let id = broker(p.driver(), profile)?;
    let hw = p.hardware();
    (hw.dma_pages == 22
        && hw.queues == 2
        && hw.entries_per_queue == 16
        && p.dependencies()
            .iter()
            .any(|d| d.id == id && d.kind == Kind::SystemProfile && d.sha256 == *profile))
    .then_some(&REGIONS)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Error,
        dma::{Direction, Ledger},
    };
    use alloc::string::String;
    use sha2::{Digest, Sha256};

    #[test]
    fn profils_historiques_et_22_directions() {
        let arm: [u8; 32] = Sha256::digest(
            b"QEMU virt-10.0 PCI 00:07.0 SID56; IRQ38; 2x16; 22 pages; reset 3E; broker only\n",
        )
        .into();
        let q35: [u8; 32] = Sha256::digest(
            b"QEMU Q35 PCI 00:07.0 SID56; VT-d; INTx GSI23; 2x16; 22 pages; broker only\n",
        )
        .into();
        for (driver, digest, other, dependency) in [
            (
                "org.novaos.lab.virtio-net-arm",
                arm,
                q35,
                "org.novaos.lab.courtier-arm",
            ),
            (
                "org.novaos.lab.virtio-net-q35",
                q35,
                arm,
                "org.novaos.lab.courtier-q35",
            ),
        ] {
            assert_eq!(broker(driver, &digest), Some(dependency));
            assert_eq!(broker(driver, &other), None);
            assert_eq!(broker("org.novaos.lab.autre", &digest), None);
        }
        let roles = [
            "descripteurs:r",
            "disponibles:r",
            "utilises:w",
            "tx:r",
            "rx:w",
        ]
        .map(String::from);
        let ledger = Ledger::reserve(22, &roles, Some(&REGIONS)).unwrap();
        assert_eq!(ledger.bytes(), 66);
        for i in 0..22 {
            let expected = if matches!(i, 2 | 5..=13) {
                Direction::Write
            } else {
                Direction::Read
            };
            assert_eq!(ledger.page(i, false).unwrap().direction, expected);
        }
        assert_eq!(
            Ledger::reserve(22, &roles[..4], Some(&REGIONS)).err(),
            Some(Error::Denied)
        );
        assert_eq!(Ledger::reserve(22, &roles, None).err(), Some(Error::Full));
    }
}
