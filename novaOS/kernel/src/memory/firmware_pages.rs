//! Une lecture firmware n'utilise que des pages couvertes intégralement par une
//! région admise. Toute entrée contradictoire, même partielle, annule l’autorisation.
pub(crate) fn authorized(
    page: u64,
    size: u64,
    regions: impl Iterator<Item = (u64, u64, bool)>,
) -> bool {
    let Some(limit) = page.checked_add(size) else {
        return false;
    };
    if !size.is_power_of_two() || page % size != 0 {
        return false;
    }
    let mut covered = false;
    for (base, length, allowed) in regions {
        let Some(end) = base.checked_add(length) else {
            return false;
        };
        if length != 0 && base < limit && page < end {
            if !allowed {
                return false;
            }
            covered |= base <= page && limit <= end;
        }
    }
    covered
}

#[cfg(test)]
mod tests {
    use super::authorized;
    const PAGE: u64 = 4096;

    #[test]
    fn couverture_entiere_et_bornes_exclusives() {
        let regions = [(0, PAGE, false), (PAGE, PAGE, true), (2 * PAGE, 1, false)];
        assert!(authorized(PAGE, PAGE, regions.into_iter()));
        assert!(!authorized(2 * PAGE, PAGE, regions.into_iter()));
        assert!(!authorized(
            PAGE,
            PAGE,
            [(PAGE, PAGE - 1, true)].into_iter()
        ));
    }

    #[test]
    fn conflit_meme_sur_un_seul_octet_dans_les_deux_ordres() {
        let trusted = (PAGE, PAGE, true);
        let conflicting = (2 * PAGE - 1, 1, false);
        for regions in [[trusted, conflicting], [conflicting, trusted]] {
            assert!(!authorized(PAGE, PAGE, regions.into_iter()));
        }
        assert!(authorized(
            PAGE,
            PAGE,
            [trusted, (PAGE + 1, 0, false)].into_iter()
        ));
    }

    #[test]
    fn debordements_et_page_non_alignee() {
        assert!(!authorized(
            PAGE + 1,
            PAGE,
            [(0, 3 * PAGE, true)].into_iter()
        ));
        assert!(!authorized(
            PAGE,
            PAGE,
            [(PAGE, u64::MAX, true)].into_iter()
        ));
        assert!(!authorized(u64::MAX - PAGE + 1, PAGE, core::iter::empty()));
        assert!(!authorized(0, 0, core::iter::empty()));
    }
}
