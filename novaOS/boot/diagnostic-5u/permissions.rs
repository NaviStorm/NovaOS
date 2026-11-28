use super::{Adresse, Inventaire, Refus, Resultat};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Plage {
    pub debut: u64,
    pub octets: u64,
}

impl Plage {
    fn fin(self) -> Resultat<u64> {
        if self.octets == 0 || (self.debut | self.octets) & 4095 != 0 {
            return Err(Refus::Plage);
        }
        self.debut.checked_add(self.octets - 1).ok_or(Refus::Plage)
    }
    fn contient(self, autre: Self) -> Resultat<bool> {
        Ok(self.debut <= autre.debut && self.fin()? >= autre.fin()?)
    }
    fn croise(self, autre: Self) -> Resultat<bool> {
        Ok(self.debut <= autre.fin()? && autre.debut <= self.fin()?)
    }
}

/// Droits vus du périphérique.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Droits {
    Lecture,
    Ecriture,
    LectureEcriture,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Traduction {
    pub iova: u64,
    pub ram: Plage,
    pub droits: Droits,
}

#[derive(Clone, Copy)]
pub struct Bail {
    pub titulaire: Adresse,
    pub traduction: Traduction,
}

pub struct Profil<'a> {
    pub xhci: Adresse,
    pub nic: Adresse,
    pub sata: Adresse,
    pub protegees: &'a [Plage],
    /// Baux attendus, établis indépendamment des clichés à comparer.
    pub baux: &'a [Bail],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Domaine {
    pub unite: u64,
    pub did: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EtatUnite {
    pub base: u64,
    pub racine: u64,
    pub tes: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Contexte {
    pub source: Adresse,
    pub domaine: Domaine,
    pub traduit: bool,
    pub ats_actif: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mapping {
    pub domaine: Domaine,
    pub traduction: Traduction,
}

/// Ressources et compteurs normalisés d'un observateur indépendant du contrôleur.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EtatSata {
    pub controleur: Adresse,
    pub generation: u64,
    pub commande_pci: u16,
    /// Aperture MMIO dédiée, en pages entières pour ce profil.
    pub registres: Plage,
    pub lectures: u64,
    pub ecritures: u64,
    pub flush: u64,
}

pub struct Cliche<'a> {
    pub sata: EtatSata,
    pub unites: &'a [EtatUnite],
    pub contextes: &'a [Contexte],
    pub mappings: &'a [Mapping],
}

impl Inventaire<'_> {
    fn verifier_profil(&self, p: &Profil<'_>) -> Resultat<()> {
        if p.protegees.is_empty() || p.protegees.len() > 128 || p.baux.len() > 256 {
            return Err(Refus::Limite);
        }
        for (id, classe, masque) in [
            (p.xhci, 0x0c0330, 0xffffff),
            (p.nic, 0x020000, 0xffff00),
            (p.sata, 0x010600, 0xffff00),
        ] {
            let f = self.fonctions[self.index(id)?];
            if f.titulaire != Some(id) || f.classe & masque != classe {
                return Err(Refus::Role);
            }
        }
        for &region in p.protegees {
            if !self.adressable(region.fin()?) {
                return Err(Refus::Plage);
            }
            for &base in &self.unites {
                if region.croise(Plage {
                    debut: base,
                    octets: 4096,
                })? {
                    return Err(Refus::Plage);
                }
            }
        }
        for (i, b) in p.baux.iter().enumerate() {
            let id = b.titulaire;
            let t = b.traduction;
            let virt = Plage {
                debut: t.iova,
                octets: t.ram.octets,
            };
            virt.fin()?;
            if self.fonctions[self.index(id)?].titulaire != Some(id)
                || !self.adressable(t.ram.fin()?)
            {
                return Err(Refus::Permission);
            }
            for &protegee in p.protegees {
                if t.ram.croise(protegee)? {
                    return Err(Refus::Permission);
                }
            }
            for &base in &self.unites {
                if t.ram.croise(Plage {
                    debut: base,
                    octets: 4096,
                })? {
                    return Err(Refus::Permission);
                }
            }
            for ancien in &p.baux[..i] {
                let a = ancien.traduction;
                if t.ram.croise(a.ram)?
                    || (id == ancien.titulaire
                        && virt.croise(Plage {
                            debut: a.iova,
                            octets: a.ram.octets,
                        })?)
                {
                    return Err(Refus::Permission);
                }
            }
        }
        for id in [p.xhci, p.sata] {
            if !p.baux.iter().any(|b| b.titulaire == id) {
                return Err(Refus::Permission);
            }
        }
        for &(masque, zone) in &self.reservations {
            // Vérifier chaque titulaire concerné, y compris via ses alias.
            for (i, f) in self.fonctions.iter().enumerate() {
                if masque & (1u64 << i) == 0 {
                    continue;
                }
                let mut couvert = false;
                for b in p.baux {
                    let t = b.traduction;
                    if Some(b.titulaire) == f.titulaire
                        && t.iova == t.ram.debut
                        && t.ram.contient(zone)?
                    {
                        couvert = true;
                        break;
                    }
                }
                if !couvert {
                    return Err(Refus::Reservation);
                }
            }
        }
        Ok(())
    }

    fn verifier_cliche(&self, p: &Profil<'_>, c: &Cliche<'_>) -> Resultat<()> {
        if c.unites.len() != self.unites.len()
            || c.contextes.len() != self.fonctions.len()
            || c.mappings.len() != p.baux.len()
        {
            return Err(Refus::Couverture);
        }
        let mmio = c.sata.registres;
        if c.sata.controleur != p.sata
            || c.sata.generation == 0
            || mmio.debut == 0
            || !self.adressable(mmio.fin()?)
        {
            return Err(Refus::Stockage);
        }
        for &z in p.protegees {
            if mmio.croise(z)? {
                return Err(Refus::Stockage);
            }
        }
        for b in p.baux {
            if mmio.croise(b.traduction.ram)? {
                return Err(Refus::Stockage);
            }
        }
        for (i, u) in c.unites.iter().enumerate() {
            let racine = Plage {
                debut: u.racine,
                octets: 4096,
            };
            racine.fin()?;
            if !self.unites.contains(&u.base)
                || !u.tes
                || u.racine == 0
                || self.unites.contains(&u.racine)
                || mmio.croise(Plage {
                    debut: u.base,
                    octets: 4096,
                })?
                || c.unites[..i]
                    .iter()
                    .any(|v| v.base == u.base || v.racine == u.racine)
            {
                return Err(Refus::Unite);
            }
            if !p.protegees.iter().any(|z| z.contient(racine) == Ok(true)) {
                return Err(Refus::Permission);
            }
        }
        for (i, ctx) in c.contextes.iter().enumerate() {
            if !ctx.traduit || ctx.ats_actif || self.unite(ctx.source)? != ctx.domaine.unite {
                return Err(Refus::Domaine);
            }
            let owner = self.fonctions[self.index(ctx.source)?].titulaire;
            for autre in &c.contextes[..i] {
                let meme_owner = owner == self.fonctions[self.index(autre.source)?].titulaire;
                if autre.source == ctx.source || meme_owner != (ctx.domaine == autre.domaine) {
                    return Err(Refus::Domaine);
                }
            }
        }
        for b in p.baux {
            let domaine = c
                .contextes
                .iter()
                .find(|ctx| ctx.source == b.titulaire)
                .ok_or(Refus::Couverture)?
                .domaine;
            let attendu = Mapping {
                domaine,
                traduction: b.traduction,
            };
            if !c.mappings.contains(&attendu) {
                return Err(Refus::Permission);
            }
        }
        Ok(())
    }

    /// Comparaison logique de deux descriptions complètes, sans lecture du bus.
    pub fn comparer(
        &self,
        p: &Profil<'_>,
        avant: &Cliche<'_>,
        remise: &Cliche<'_>,
    ) -> Resultat<()> {
        self.verifier_profil(p)?;
        self.verifier_cliche(p, avant)?;
        self.verifier_cliche(p, remise)?;
        // Les deux états satisfont déjà les mêmes baux, avec identité des plages et des droits.
        // Exiger aussi la continuité des identités de domaines et des racines, même à baux égaux.
        if avant.sata != remise.sata {
            return Err(Refus::Stockage);
        }
        if !avant.unites.iter().all(|u| remise.unites.contains(u))
            || !avant.contextes.iter().all(|c| remise.contextes.contains(c))
        {
            return Err(Refus::Continuite);
        }
        Ok(())
    }
}
