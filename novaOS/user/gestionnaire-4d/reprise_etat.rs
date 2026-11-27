//! Vue locale reconstruite par un acteur EL0. Les adresses et les handles matériels
//! restent dans les tables de leurs propriétaires.
pub const WORDS: usize = 23;
pub const FULL: u64 = (1 << 22) - 1;
pub const RESERVED: u64 = 0;
pub const ENGAGED: u64 = 1;
pub const CONFIRMED: u64 = 2;
pub const DENIED_LAUNCH: u64 = 3;

#[derive(Clone, Copy)]
pub struct EtatRepris {
    mots: [u64; WORDS],
}

impl EtatRepris {
    pub fn lire(mots: [u64; WORDS], precedent: Option<&Self>) -> Option<Self> {
        let [
            boot_lo,
            boot_hi,
            controleur,
            groupe,
            inventaire,
            generation,
            issue,
            pid,
            incarnation,
            generation_instance,
            epingles,
            mappes,
            recu,
            publication,
            _,
            _,
            _,
            _,
            etat,
            intention,
            resultat_connu,
            resultat,
            remis,
        ] = mots;
        if (boot_lo | boot_hi) == 0
            || controleur != 40
            || groupe != 56
            || inventaire == 0
            || epingles & !FULL != 0
            || mappes & !epingles != 0
            || !matches!(etat, 0 | 1 | 2 | 3 | 5)
            || (etat == 5 && epingles != 0)
            || (generation == 0 && (epingles | intention | recu | publication) != 0)
        {
            return None;
        }
        let titulaire = [pid, incarnation, generation_instance];
        match issue {
            CONFIRMED if !titulaire.contains(&0) && intention != 0 => (),
            ENGAGED | DENIED_LAUNCH if titulaire == [0; 3] && intention != 0 => (),
            RESERVED if titulaire == [0; 3] => (),
            _ => return None,
        }
        if resultat_connu > 1
            || remis > resultat_connu
            || (resultat_connu == 0 && resultat != 0)
            || (publication == 0 && resultat_connu != 0)
            || (publication != 0 && issue != CONFIRMED)
        {
            return None;
        }
        if let Some(old) = precedent {
            let o = &old.mots;
            let conserve = |avant, apres| avant == 0 || avant == apres;
            if o[..5] != mots[..5]
                || o[14..18] != mots[14..18]
                || !conserve(o[5], generation)
                || !conserve(o[19], intention)
                || !conserve(o[12], recu)
                || !conserve(o[13], publication)
                || (o[6] == ENGAGED && issue == RESERVED)
                || (o[6] >= CONFIRMED && o[6] != issue)
                || (o[6] == CONFIRMED && o[7..10] != titulaire)
                || o[18] > etat
                || o[20] > resultat_connu
                || o[22] > remis
                || (o[20] != 0 && o[21] != resultat)
            {
                return None;
            }
        }
        Some(Self { mots })
    }
    pub fn intention(&self) -> u64 {
        self.mots[19]
    }
    pub fn plan(&self) -> [u8; 32] {
        core::array::from_fn(|i| self.mots[14 + i / 8].to_le_bytes()[i % 8])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn selection() -> [u64; WORDS] {
        let mut s = [0; WORDS];
        s[..5].copy_from_slice(&[11, 12, 40, 56, 1]);
        s
    }
    fn pilote() -> [u64; WORDS] {
        let mut s = selection();
        s[5..12].copy_from_slice(&[2, CONFIRMED, 7, 19, 1, FULL, FULL]);
        s[19] = 8;
        s
    }
    #[test]
    fn reconstituer_et_fermer() {
        let depart = EtatRepris::lire(selection(), None).unwrap();
        let actif = EtatRepris::lire(pilote(), Some(&depart)).unwrap();
        let mut fin = pilote();
        fin[10..12].fill(0);
        fin[12] = 82;
        fin[18] = 5;
        assert_eq!(EtatRepris::lire(fin, Some(&actif)).unwrap().intention(), 8);
    }
    #[test]
    fn refuser_transfert_et_rejeu() {
        let initial = pilote();
        let actif = EtatRepris::lire(initial, None).unwrap();
        for i in [0, 1, 2, 3, 4, 5, 7, 8, 9, 14, 15, 16, 17, 19] {
            let mut autre = initial;
            autre[i] += 1;
            assert!(EtatRepris::lire(autre, Some(&actif)).is_none(), "{i}");
        }
        let mut perime = initial;
        perime[6] = ENGAGED;
        perime[7..10].fill(0);
        assert!(EtatRepris::lire(perime, Some(&actif)).is_none());
    }
    #[test]
    fn refuser_traduction_sans_cadre_et_fausse_remise() {
        let mut s = pilote();
        s[10] = 0;
        assert!(EtatRepris::lire(s, None).is_none());
        let mut s = pilote();
        s[13] = 7;
        s[22] = 1;
        assert!(EtatRepris::lire(s, None).is_none());
        s[20] = 1;
        s[21] = 8;
        let acquit = EtatRepris::lire(s, None).unwrap();
        s[21] = 7;
        assert!(EtatRepris::lire(s, Some(&acquit)).is_none());
    }
}
