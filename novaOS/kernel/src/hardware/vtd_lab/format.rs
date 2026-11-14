//! Contrat de registres et formats du seul profil Q35, mode root/context legacy.
pub(super) const PAGE: u64 = 4096;
pub(super) const SID: u16 = 6 << 3;
pub(super) const DID: u16 = 1;
pub(super) const BASE: u64 = 0x0200_0000;
pub(super) const STRIDE: u64 = 0x10000;
pub(super) const LAST_EPOCH: u64 = ((1 << 28) - BASE) / STRIDE;

pub(super) fn capabilities(version: u32, cap: u64, ecap: u64, host: u8) -> Option<usize> {
    let width = ((cap >> 16) & 63) + 1;
    let record = (((cap >> 24) & 1023) * 16) as usize;
    // Le banc vérifie le profil exact : quatre niveaux, invalidations en file,
    // drainage lecture/écriture, une seule entrée FRCD, sans ATS, IR ni scalable.
    let unsupported = (1 << 2) | (1 << 3) | (1 << 29) | (1 << 40) | (1 << 43);
    if version != 0x10
        || width != 48
        || host != 48
        || cap & (1 << 10) == 0
        || cap & (3 << 54) != 3 << 54
        || cap & (1 << 7) == 0
        || cap & (1 << 4) != 0
        || (cap >> 40) & 255 != 0
        || ecap & 2 == 0
        || ecap & unsupported != 0
        || !(0x100..=4080).contains(&record)
    {
        return None;
    }
    Some(record)
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(super) struct Record {
    pub(super) iova: u64,
    pub(super) sid: u16,
    pub(super) reason: u8,
    pub(super) read: bool,
}
pub(super) fn record(status: u32, low: u64, high: u64) -> Option<Record> {
    // Le banc n'admet qu'une transaction DMA non traduite, sans PASID, à la fois.
    let allowed = (3 << 62) | (255 << 32) | 0xffff;
    if status != 2 || high >> 63 != 1 || high & !allowed != 0 || low & 4095 != 0 {
        return None;
    }
    Some(Record {
        iova: low,
        sid: high as u16,
        reason: (high >> 32) as u8,
        read: high & (1 << 62) != 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    const CAP: u64 = 0x80d2008c222f0686;
    const ECAP: u64 = 0xf42;

    #[test]
    fn dimensions_et_capacites_lues_sur_q35() {
        assert_eq!(capabilities(0x10, CAP, ECAP, 48), Some(0x220));
        for bit in [7, 10, 54, 55] {
            assert_eq!(capabilities(0x10, CAP & !(1 << bit), ECAP, 48), None);
        }
        for bit in [2, 3, 29, 40, 43] {
            assert_eq!(capabilities(0x10, CAP, ECAP | 1 << bit, 48), None);
        }
        assert_ne!(DID, 0);
        assert!(capabilities(0x10, CAP | 1 << 4, ECAP, 48).is_none());
        assert_eq!(capabilities(0x11, CAP, ECAP, 48), None);
        assert_eq!(capabilities(0x10, CAP, ECAP & !2, 48), None);
        assert_eq!(capabilities(0x10, CAP | 1 << 40, ECAP, 48), None);
        assert_eq!(capabilities(0x10, CAP, ECAP, 39), None);
        assert_eq!(capabilities(0x10, CAP | (1023 << 24), ECAP, 48), None);
        assert_eq!(capabilities(0x10, CAP & !(1023 << 24), ECAP, 48), None);
    }

    #[test]
    fn champs_de_faute_et_perte_de_trace() {
        let hi = (1 << 63) | (5 << 32) | u64::from(SID);
        assert_eq!(
            record(2, BASE, hi),
            Some(Record {
                iova: BASE,
                sid: SID,
                reason: 5,
                read: false
            })
        );
        assert!(record(2, BASE, hi | (1 << 62)).unwrap().read);
        assert!(record(3, BASE, hi).is_none()); // overflow
        assert!(record(0x12, BASE, hi).is_none()); // faute de file
        assert!(record(2, BASE, hi & !(1 << 63)).is_none());
        assert!(record(2, BASE, hi | 1 << 31).is_none()); // PASID présent
        assert!(record(2, BASE + 1, hi).is_none());
    }

    #[test]
    fn fenetres_disjointes_sous_le_masque_edu() {
        assert!(2 * PAGE < STRIDE);
        assert!(BASE + (LAST_EPOCH - 1) * STRIDE + 3 * PAGE <= 1 << 28);
        assert_eq!(BASE + LAST_EPOCH * STRIDE, 1 << 28);
    }
}
