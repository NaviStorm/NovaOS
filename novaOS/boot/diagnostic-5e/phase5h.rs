//! Profil de diagnostic passif. Aucune opération de configuration PCI ou VT-d.
use r_efi::efi;

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Trust {
    Firmware,
    Unverified,
    Unknown,
}

impl Trust {
    pub(crate) fn from_flags(secure: Option<u8>, setup: Option<u8>, audit: Option<u8>) -> Self {
        match (secure, setup, audit) {
            (Some(1), Some(0), None | Some(0)) => Self::Firmware,
            (Some(0), Some(0 | 1), None | Some(0 | 1)) => Self::Unverified,
            _ => Self::Unknown,
        }
    }

    pub(crate) fn message(&self) -> &'static str {
        match self {
            Self::Firmware => "NovaOS: Secure Boot actif - EFI admis par la politique UEFI",
            Self::Unverified => {
                "NovaOS: Secure Boot inactif - chaine de demarrage non authentifiee"
            }
            Self::Unknown => "NovaOS: etat de confiance indetermine - refus",
        }
    }
}

#[cfg(feature = "diagnostic-5i")]
pub(crate) fn display(_: &efi::SystemTable, text: &str) -> Result<(), u64> {
    crate::journal5i::append(text)
}
#[cfg(not(feature = "diagnostic-5i"))]
pub(crate) fn display(st: &efi::SystemTable, text: &str) -> Result<(), u64> {
    let mut wide = [0u16; 128];
    if !text.is_ascii() || text.len() + 3 > wide.len() {
        return Err(17);
    }
    for (w, b) in wide.iter_mut().zip(text.bytes()) {
        *w = u16::from(b);
    }
    wide[text.len()] = 13;
    wide[text.len() + 1] = 10;
    let console = unsafe { st.con_out.as_ref() }.ok_or(17u64)?;
    let status = unsafe { (console.output_string)(st.con_out, wide.as_mut_ptr()) };
    if status == efi::Status::SUCCESS {
        Ok(())
    } else {
        Err(17)
    }
}

#[derive(Default)]
pub(crate) struct Remappers {
    // Données descriptives issues de la copie validée de DMAR, jamais déréférencées.
    units: [(u16, u64, bool); 16],
    count: usize,
}

impl Remappers {
    pub(crate) fn record(&mut self, item: &[u8]) -> Result<(), ()> {
        let base = u64::from_le_bytes(item.get(8..16).ok_or(())?.try_into().map_err(|_| ())?);
        let segment = u16::from_le_bytes([item[6], item[7]]);
        let all = item[4] & 1 != 0;
        if self.count == self.units.len()
            || base == 0
            || base & 4095 != 0
            || self.units[..self.count]
                .iter()
                .any(|&(s, b, a)| b == base || (a && all && s == segment))
        {
            return Err(());
        }
        self.units[self.count] = (segment, base, all);
        self.count += 1;
        Ok(())
    }

    pub(crate) fn report(&self, st: &efi::SystemTable) -> Result<(), u64> {
        for (segment, base, all) in &self.units[..self.count] {
            display(
                st,
                &alloc::format!("NovaOS: DRHD segment={segment} base={base:#x} include_all={all}"),
            )?;
        }
        display(
            st,
            "NovaOS: DMA non etabli - proprietaires et transferts en vol inconnus - arret",
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn trust_states() {
        for (s, m, a, expected) in [
            (Some(1), Some(0), Some(0), Trust::Firmware),
            (Some(1), Some(0), None, Trust::Firmware),
            (Some(0), Some(1), Some(1), Trust::Unverified),
            (Some(0), Some(0), Some(0), Trust::Unverified),
            (None, Some(0), None, Trust::Unknown),
            (Some(1), Some(1), None, Trust::Unknown),
            (Some(1), Some(0), Some(1), Trust::Unknown),
            (Some(0), None, None, Trust::Unknown),
            (Some(2), Some(0), None, Trust::Unknown),
        ] {
            assert_eq!(Trust::from_flags(s, m, a), expected);
        }
    }

    #[test]
    fn remapping_units_are_bounded_and_unambiguous() {
        let mut units = Remappers::default();
        let mut row = [0; 16];
        assert!(units.record(&row[..15]).is_err());
        assert!(units.record(&row).is_err());
        for i in 1..=16u64 {
            row[8..].copy_from_slice(&(i << 12).to_le_bytes());
            units.record(&row).unwrap();
            assert!(units.record(&row).is_err());
        }
        row[8..].copy_from_slice(&(17u64 << 12).to_le_bytes());
        assert!(units.record(&row).is_err());
    }
}
