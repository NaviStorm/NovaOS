//! Preuve de propriété des buffers à la clôture du seul port Ethernet v1.
use super::*;
mod propriete;
const TOUS: u16 = u16::MAX;

/// Photographié après le retrait de l'autorité du pilote et le reset acquitté.
/// Les bits nomment des descripteurs de l'incarnation courante.
#[derive(Clone, Copy)]
pub(in crate::hardware::block) struct Arret {
    pub(in crate::hardware::block) rx_rendus: u16,
    pub(in crate::hardware::block) tx_en_vol: u16,
}

impl Arret {
    #[cfg(target_arch = "aarch64")]
    pub(in crate::hardware::block) fn page_restituable(self, n: usize) -> bool {
        propriete::restituable(self.rx_rendus, self.tx_en_vol, n)
    }
}

/// Valide les nouvelles entrées used, y compris celles que le pilote n'a pas lues.
/// Un identifiant, une longueur ou une distance incohérents invalident toute la preuve.
fn rendus(used: Region, lus: u32, dus: u16, rx: bool) -> Result<u16> {
    let index = used.r16(2);
    barrier();
    propriete::restants(lus, index, dus, rx, |slot| {
        let pos = 4 + 8 * slot;
        (used.r32(pos), used.r32(pos + 4))
    })
    .ok_or(Error::Quarantine)
}

impl Port<'_> {
    /// Le courtier ne traite plus aucune requête de cette génération.
    pub(in crate::hardware::block) fn sceller(&self, generation: u64) -> Result<Arret> {
        if option_env!("NOVAOS_ETH3_CASE") == Some("rattrapage") {
            hal::log(format_args!("E3C dernier lot RX generation={generation}"));
            // Rendez-vous du banc avec le pair. La restitution dépend uniquement
            // des descripteurs réellement revenus lors de la vérification ci-dessous.
            let _ = crate::hardware::block::io::wait(|| self.cadres[2].mem.r16(2) == 16);
        }
        self.nic.cfg.intx(false);
        self.nic.cfg.master(false);
        barrier();
        if self.nic.cfg.get(0, 4) != 0x1041_1af4 || self.nic.cfg.get(4, 2) & 0x406 != 0x402 {
            return Err(Error::Quarantine);
        }
        self.nic.reset()?;
        for file in 0..2 {
            self.nic.common.w16(22, file);
            if self.nic.common.r16(22) != file || self.nic.common.r16(28) != 0 {
                return Err(Error::Quarantine);
            }
        }
        if self.nic.common.r8(20) != 0 || self.nic.cfg.get(4, 2) & 0x406 != 0x402 {
            return Err(Error::Quarantine);
        }
        if option_env!("NOVAOS_ETH3_CASE") == Some("preuve-absente") {
            hal::log(format_args!(
                "E3C preuve de reset retenue generation={generation}"
            ));
            return Err(Error::Quarantine);
        }
        let s = &self.flux;
        let preuve = if self.ouvert {
            if s.publiees.checked_sub(s.completees) != Some(s.en_vol.count_ones())
                || s.rx_lues.checked_sub(s.rx_base) != Some(s.rx_rendues.count_ones())
                || self.cadres[4].mem.r16(2) != s.publiees as u16
                || self.cadres[1].mem.r16(2) != (s.rx_base as u16).wrapping_add(16)
            {
                return Err(Error::Quarantine);
            }
            Arret {
                rx_rendus: !rendus(self.cadres[2].mem, s.rx_lues, !s.rx_rendues, true)?,
                tx_en_vol: rendus(self.cadres[5].mem, s.completees, s.en_vol, false)?,
            }
        } else {
            Arret {
                rx_rendus: TOUS,
                tx_en_vol: 0,
            }
        };
        hal::log(format_args!(
            "E3C arret generation={generation} reset=0 queues=0,0 bme=0 ouverture={} rx_lus={} tx_lus={} tx_publies={} rx_rendus={:#06x} tx_en_vol={:#06x}",
            self.ouvert, s.rx_lues, s.completees, s.publiees, preuve.rx_rendus, preuve.tx_en_vol
        ));
        Ok(preuve)
    }
}
