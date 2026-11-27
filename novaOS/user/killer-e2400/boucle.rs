//! Pair de simulation en mémoire privée. Aucune réussite ici ne confirme une
//! livraison sur câble. Les adresses locales et l'EtherType sont ceux du banc v1.
use super::adaptateur::{Adaptateur, Erreur, Filtre, Reception};
use ethernet_v1::{
    Binding,
    wire::{FRAME, LIMIT},
};

pub const STATION: [u8; 6] = [0x52, 0x54, 0, 0x12, 0x34, 0x56];
pub const PAIR: [u8; 6] = [2, 0x4e, 0x56, 0, 0, 1];

#[derive(Clone, Copy)]
struct Copie {
    len: usize,
    bytes: [u8; FRAME],
}
impl Copie {
    const VIDE: Self = Self {
        len: 0,
        bytes: [0; FRAME],
    };
}

pub struct Tampons {
    tx: [Copie; LIMIT],
    rx: [Copie; LIMIT],
}
impl Tampons {
    pub const VIDES: Self = Self {
        tx: [Copie::VIDE; LIMIT],
        rx: [Copie::VIDE; LIMIT],
    };
}

pub struct Boucle<'a> {
    liaison: Binding,
    adaptation: Adaptateur,
    tampons: &'a mut Tampons,
    a_notifier: u8,
    index_rx: u8,
    emises: u64,
    en_attente: usize,
}

impl<'a> Boucle<'a> {
    pub fn nouvelle(liaison: Binding, tampons: &'a mut Tampons) -> Result<Self, Erreur> {
        let mut adaptation = Adaptateur::nouveau(liaison, Filtre::laboratoire(STATION, PAIR)?)?;
        adaptation.preposter_rx(liaison, LIMIT)?;
        tampons.tx.fill(Copie::VIDE);
        tampons.rx.fill(Copie::VIDE);
        Ok(Self {
            liaison,
            adaptation,
            tampons,
            a_notifier: 0,
            index_rx: 0,
            emises: 0,
            en_attente: 0,
        })
    }

    pub fn preparer(&mut self, coupon: usize, source: &[u8]) -> Result<(), Erreur> {
        let slot = self.tampons.tx.get_mut(coupon).ok_or(Erreur::Etat)?;
        if slot.len != 0 || self.a_notifier & (1 << coupon) != 0 {
            return Err(Erreur::Etat);
        }
        slot.len = self
            .adaptation
            .preparer(self.liaison, coupon, source, &mut slot.bytes)?;
        Ok(())
    }

    pub fn publier(&mut self, n: usize) -> Result<(), Erreur> {
        let lot = self.adaptation.publier(self.liaison, n)?;
        for &coupon in &lot.coupons[..lot.nombre] {
            let tx = &mut self.tampons.tx[usize::from(coupon)];
            if self.en_attente < LIMIT {
                let pos = (usize::from(self.index_rx) + self.en_attente) % LIMIT;
                let rx = &mut self.tampons.rx[pos];
                rx.bytes[..tx.len].copy_from_slice(&tx.bytes[..tx.len]);
                rx.bytes[..6].copy_from_slice(&STATION);
                rx.bytes[6..12].copy_from_slice(&PAIR);
                // Convention du pair de laboratoire Ethernet 3B. Le numéro de
                // séquence reste intact. Le motif de retour distingue TX de RX.
                let debut = if self.emises < 16 { 15 } else { 18 };
                for octet in &mut rx.bytes[debut..tx.len] {
                    *octet ^= 0xa5;
                }
                rx.len = tx.len;
                self.en_attente += 1;
            }
            self.emises = self.emises.saturating_add(1);
            tx.bytes[..tx.len].fill(0);
            tx.len = 0;
        }
        // Seul ce pair CPU simule un consommateur immédiat.
        self.a_notifier |= self
            .adaptation
            .completions(self.liaison, u16::from(lot.producteur))?;
        Ok(())
    }

    pub fn perdre_recu(mut self, n: usize) -> Result<super::adaptateur::Retrait, Erreur> {
        self.adaptation.publication_incertaine(self.liaison, n)
    }

    pub fn completions(&mut self, max: usize) -> u8 {
        let mut restants = self.a_notifier;
        for _ in 0..max.min(LIMIT) {
            if restants == 0 {
                break;
            }
            restants &= restants - 1;
        }
        let resultat = self.a_notifier ^ restants;
        self.a_notifier = restants;
        resultat
    }

    pub fn recevoir(&mut self, sortie: &mut [u8; FRAME]) -> Result<Option<usize>, Erreur> {
        if self.en_attente == 0 {
            return Ok(None);
        }
        let rx = &mut self.tampons.rx[usize::from(self.index_rx) % LIMIT];
        let mut rrd = [0u8; 16];
        rrd[..4].copy_from_slice(&((u32::from(self.index_rx) << 20) | (1 << 16)).to_le_bytes());
        rrd[12..].copy_from_slice(&((1 << 31) | (rx.len as u32 + 4)).to_le_bytes());
        let Reception::Trame(frame) =
            self.adaptation
                .reception(self.liaison, &rrd, &rx.bytes[..rx.len])?
        else {
            return Err(Erreur::Etat);
        };
        let len = frame.len();
        sortie[..len].copy_from_slice(frame);
        rx.bytes[..len].fill(0);
        rx.len = 0;
        self.index_rx = self.index_rx.wrapping_add(1);
        self.en_attente -= 1;
        self.adaptation.preposter_rx(self.liaison, 1)?;
        Ok(Some(len))
    }

    pub fn fermer(mut self) -> Result<(), Erreur> {
        self.adaptation.retirer(self.liaison)?;
        Ok(())
    }
}
