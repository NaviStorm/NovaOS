//! Une seule fonction EDU dans le profil de laboratoire. Le courtier PCI doit avoir
//! identifié cette fonction et attribué sa BAR. La MMU vérifie à nouveau l'aperture,
//! toute la carte mémoire et l'exclusivité, sous son propre verrou.
pub(super) const BAR_SIZE: u64 = 1 << 20;

pub(super) fn valid_bar(bar: u64, size: u64, regions: impl Iterator<Item = (u64, u64)>) -> bool {
    #[cfg(target_arch = "aarch64")]
    let aperture = 0x1000_0000..0x3000_0000;
    #[cfg(target_arch = "x86_64")]
    let aperture = 0x8000_0000..0xb000_0000;
    let Some(end) = bar.checked_add(size) else {
        return false;
    };
    if size != BAR_SIZE || bar % BAR_SIZE != 0 || bar < aperture.start || end > aperture.end {
        return false;
    }
    for (base, length) in regions {
        let Some(limit) = base.checked_add(length) else {
            return false;
        };
        if length != 0 && base < end && bar < limit {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fenetre_taille_et_alignement_selon_architecture() {
        let (low, high) = if cfg!(target_arch = "aarch64") {
            (0x1000_0000, 0x3000_0000)
        } else {
            (0x8000_0000, 0xb000_0000)
        };
        let valid = |bar, size| valid_bar(bar, size, core::iter::empty());
        assert!(valid(low, BAR_SIZE));
        assert!(valid(high - BAR_SIZE, BAR_SIZE));
        assert!(!valid(low - BAR_SIZE, BAR_SIZE));
        assert!(!valid(high, BAR_SIZE));
        assert!(!valid(low + 4096, BAR_SIZE));
        assert!(!valid(low, 0));
        assert!(!valid(low, 4096));
        assert!(!valid(u64::MAX - BAR_SIZE + 1, BAR_SIZE));
    }

    #[test]
    fn toute_la_bar_exclut_ram_et_firmware_y_compris_sur_un_octet() {
        let bar = if cfg!(target_arch = "aarch64") {
            0x1000_0000
        } else {
            0x8000_0000
        };
        let last = bar + BAR_SIZE - 1;
        let overlaps = (last, 1);
        assert!(!valid_bar(bar, BAR_SIZE, [overlaps].into_iter()));
        assert!(!valid_bar(bar, BAR_SIZE, [(bar, BAR_SIZE)].into_iter()));
        assert!(valid_bar(
            bar,
            BAR_SIZE,
            [(last + 1, 1), (bar, 0)].into_iter()
        ));
        assert!(!valid_bar(bar, BAR_SIZE, [(1, u64::MAX)].into_iter()));
    }
}
