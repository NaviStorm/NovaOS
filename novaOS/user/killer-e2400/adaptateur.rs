//! Adaptation sur copies CPU des champs TPD/RFD/RRD et du contrat Ethernet v1.
//! Le propriétaire du port fournit la liaison authentifiée. Aucun descripteur
//! modifiable, index de registre ou IOVA ne traverse l'interface Ethernet.
use super::profil::{ENTREES, cadres_dma};
use ethernet_v1::{
    Binding,
    wire::{FRAME, LIMIT},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Erreur {
    Liaison,
    Filtre,
    Taille,
    Plein,
    Etat,
    Quarantaine,
}

#[derive(Clone, Copy)]
pub struct Filtre {
    station: [u8; 6],
    pair: [u8; 6],
}
impl Filtre {
    pub fn laboratoire(station: [u8; 6], pair: [u8; 6]) -> Result<Self, Erreur> {
        let unicast = |a: [u8; 6]| a != [0; 6] && a[0] & 1 == 0;
        if station == pair || !unicast(station) || !unicast(pair) {
            return Err(Erreur::Filtre);
        }
        Ok(Self { station, pair })
    }
    fn autorise(&self, trame: &[u8], tx: bool) -> bool {
        let (dst, src) = if tx {
            (self.pair, self.station)
        } else {
            (self.station, self.pair)
        };
        trame.len() >= 14
            && trame[..6] == dst
            && trame[6..12] == src
            && trame[12..14] == [0x88, 0xb5]
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Reception<'a> {
    Attente,
    Ecartee,
    Trame(&'a [u8]),
}

pub struct Publication {
    pub producteur: u8,
    pub coupons: [u8; LIMIT],
    pub nombre: usize,
}

pub struct Adaptateur {
    liaison: Binding,
    filtre: Filtre,
    gele: bool,
    // Les entrées logiques sont indexées modulo huit, les indices TPD modulo 256.
    coupons: [u8; LIMIT],
    cons_tx: u8,
    preparees: usize,
    publiees: usize,
    cons_rx: u8,
    pretees: usize,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Retrait {
    pub tx_incertains: usize,
    pub tx_annules: usize,
    pub rx_pretes: usize,
    pub cadres_a_conserver_si_exposes: usize,
}

impl Adaptateur {
    /// À créer exclusivement après authentification par le service. La liaison
    /// identifie une session. L'instance ne détient aucune preuve de retrait DMA.
    pub fn nouveau(liaison: Binding, filtre: Filtre) -> Result<Self, Erreur> {
        if liaison.boot == [0; 16]
            || [
                liaison.port,
                liaison.owner,
                liaison.incarnation,
                liaison.channel,
                liaison.epoch,
            ]
            .contains(&0)
        {
            return Err(Erreur::Liaison);
        }
        Ok(Self {
            liaison,
            filtre,
            gele: false,
            coupons: [0; LIMIT],
            cons_tx: 0,
            preparees: 0,
            publiees: 0,
            cons_rx: 0,
            pretees: 0,
        })
    }

    fn verifier(&self, origine: Binding) -> Result<(), Erreur> {
        if origine != self.liaison {
            return Err(Erreur::Liaison);
        }
        if self.gele {
            return Err(Erreur::Quarantaine);
        }
        Ok(())
    }

    fn incoherence<T>(&mut self) -> Result<T, Erreur> {
        self.gele = true;
        Err(Erreur::Quarantaine)
    }

    fn position(&self, decalage: usize) -> usize {
        (usize::from(self.cons_tx) + decalage) % LIMIT
    }

    /// La copie privée de sortie reçoit seulement la trame et le bourrage nul.
    pub fn preparer(
        &mut self,
        origine: Binding,
        coupon: usize,
        trame: &[u8],
        tampon: &mut [u8],
    ) -> Result<usize, Erreur> {
        self.verifier(origine)?;
        let n = trame.len().max(60);
        if !(14..=FRAME).contains(&trame.len()) || tampon.len() < n {
            return Err(Erreur::Taille);
        }
        if !self.filtre.autorise(trame, true) {
            return Err(Erreur::Filtre);
        }
        let occupes = self.preparees + self.publiees;
        if coupon >= LIMIT {
            return Err(Erreur::Etat);
        }
        if occupes == LIMIT {
            return Err(Erreur::Plein);
        }
        if (0..occupes).any(|i| self.coupons[self.position(i)] as usize == coupon) {
            return Err(Erreur::Etat);
        }
        let slot = self.position(occupes);
        tampon[..trame.len()].copy_from_slice(trame);
        tampon[trame.len()..n].fill(0);
        self.coupons[slot] = coupon as u8;
        self.preparees += 1;
        Ok(n)
    }

    /// Simule le producteur après publication d'un préfixe validé.
    pub fn publier(&mut self, origine: Binding, n: usize) -> Result<Publication, Erreur> {
        self.verifier(origine)?;
        if n == 0 || n > self.preparees {
            return Err(Erreur::Etat);
        }
        let mut coupons = [0; LIMIT];
        for (i, coupon) in coupons[..n].iter_mut().enumerate() {
            *coupon = self.coupons[self.position(self.publiees + i)];
        }
        self.preparees -= n;
        self.publiees += n;
        Ok(Publication {
            producteur: self.cons_tx.wrapping_add(self.publiees as u8),
            coupons,
            nombre: n,
        })
    }

    /// L'écriture du producteur a pu être observée avant la perte du reçu.
    /// Conserver tout le préfixe tenté, même si aucune complétion n'est arrivée.
    pub fn publication_incertaine(
        &mut self,
        origine: Binding,
        tentees: usize,
    ) -> Result<Retrait, Erreur> {
        self.verifier(origine)?;
        if tentees == 0 || tentees > self.preparees {
            return Err(Erreur::Etat);
        }
        self.preparees -= tentees;
        self.publiees += tentees;
        self.retirer(origine)
    }

    /// Instantané CPU du consommateur TPD. Un index illégal fige les deux sens.
    pub fn completions(&mut self, origine: Binding, index: u16) -> Result<u8, Erreur> {
        self.verifier(origine)?;
        let Ok(index) = u8::try_from(index) else {
            return self.incoherence();
        };
        let n = usize::from(index.wrapping_sub(self.cons_tx));
        if n > self.publiees {
            return self.incoherence();
        }
        let mut masque = 0;
        for i in 0..n {
            masque |= 1 << self.coupons[self.position(i)];
        }
        self.cons_tx = index;
        self.publiees -= n;
        Ok(masque)
    }

    pub fn preposter_rx(&mut self, origine: Binding, n: usize) -> Result<(), Erreur> {
        self.verifier(origine)?;
        if n == 0 || n > ENTREES - 1 - self.pretees {
            return Err(Erreur::Plein);
        }
        self.pretees += n;
        Ok(())
    }

    /// Le producteur du snapshot doit garantir sa cohérence et la propriété du
    /// tampon. Le statut UPDATE est lu sur la copie complète du RRD petit-boutiste.
    pub fn reception<'a>(
        &mut self,
        origine: Binding,
        rrd: &[u8; 16],
        copie: &'a [u8],
    ) -> Result<Reception<'a>, Erreur> {
        self.verifier(origine)?;
        let mot = |i: usize| u32::from_le_bytes(rrd[i..i + 4].try_into().unwrap());
        let etat = mot(12);
        if etat & (1 << 31) == 0 {
            return Ok(Reception::Attente);
        }
        let origine_rfd = mot(0);
        if self.pretees == 0
            || origine_rfd >> 20 != u32::from(self.cons_rx)
            || (origine_rfd >> 16) & 15 != 1
        {
            return self.incoherence();
        }
        let longueur = (etat & 0x3fff) as usize;
        // Un seul RFD a été prêté. Une longueur supérieure à sa capacité
        // ne permet plus de savoir quels emplacements ont été touchés.
        if longueur > super::profil::TAMPON {
            return self.incoherence();
        }
        let erreurs_l2 = (0x3f << 20) | (3 << 29) | (1 << 16);
        let retour = if etat & erreurs_l2 != 0 || !(64..=FRAME + 4).contains(&longueur) {
            Reception::Ecartee
        } else {
            let Some(trame) = copie.get(..longueur - 4) else {
                return self.incoherence();
            };
            if self.filtre.autorise(trame, false) {
                Reception::Trame(trame)
            } else {
                Reception::Ecartee
            }
        };
        self.pretees -= 1;
        self.cons_rx = self.cons_rx.wrapping_add(1);
        Ok(retour)
    }

    pub fn retirer(&mut self, origine: Binding) -> Result<Retrait, Erreur> {
        if origine != self.liaison {
            return Err(Erreur::Liaison);
        }
        self.gele = true;
        // Un crédit TX rendu ne démontre pas le retrait des traductions. Le
        // bilan est idempotent et n'offre aucune opération de libération.
        Ok(Retrait {
            tx_incertains: self.publiees,
            tx_annules: self.preparees,
            rx_pretes: self.pretees,
            cadres_a_conserver_si_exposes: cadres_dma(),
        })
    }
}
