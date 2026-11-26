//! Propriété des seize descripteurs d'une file et de leurs pages de données.
pub(super) fn restants(
    lus: u32,
    used_idx: u16,
    dus: u16,
    rx: bool,
    entree: impl Fn(usize) -> (u32, u32),
) -> Option<u16> {
    let delta = used_idx.wrapping_sub(lus as u16);
    if u32::from(delta) > dus.count_ones() {
        return None;
    }
    let mut restants = dus;
    for n in 0..delta {
        let slot = (lus as usize + usize::from(n)) & 15;
        let (id, longueur) = entree(slot);
        if id >= 16
            || restants & (1 << id) == 0
            || (rx && !(72..=1526).contains(&longueur))
            || (!rx && longueur != 0)
        {
            return None;
        }
        restants &= !(1 << id);
    }
    Some(restants)
}

#[cfg(any(target_arch = "aarch64", test))]
pub(super) fn restituable(rx_rendus: u16, tx_en_vol: u16, page: usize) -> bool {
    match page {
        0..=5 => rx_rendus == u16::MAX && tx_en_vol == 0,
        6..=13 => rx_rendus >> (2 * (page - 6)) & 3 == 3,
        14..=21 => tx_en_vol >> (2 * (page - 14)) & 3 == 0,
        _ => false,
    }
}
