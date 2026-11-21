//! Conversion privée du profil QEMU : NOW est en ms, IPC en ticks à 250 Hz.
//! Arrondir vers le bas n’allonge jamais le délai absolu. Aucun sélecteur changé.
pub fn ticks_until(until_ms: u64, now_ms: u64) -> Option<u64> {
    let remaining = until_ms.checked_sub(now_ms)?;
    let ticks = remaining / 4;
    (ticks != 0 && ticks <= 15_000).then_some(ticks)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn absolute_deadline_is_not_renewed() {
        assert_eq!(ticks_until(10_100, 100), Some(2500));
        assert_eq!(ticks_until(10_100, 6100), Some(1000));
        assert_eq!(ticks_until(10_100, 10_096), Some(1));
        for now in 10_097..=10_104 { assert_eq!(ticks_until(10_100, now), None); }
    }
    #[test]
    fn bounds_and_rounding_never_extend_the_deadline() {
        for remaining in 0..=60_004 {
            let result=ticks_until(remaining, 0);
            match result {
                Some(ticks)=>{assert!((1..=15_000).contains(&ticks));assert!(ticks*4<=remaining);assert!(remaining-ticks*4<4);},
                None=>assert!(remaining<4 || remaining>=60_004),
            }
        }
        assert_eq!(ticks_until(0, u64::MAX), None);
        assert_eq!(ticks_until(u64::MAX, u64::MAX-4), Some(1));
    }
}
