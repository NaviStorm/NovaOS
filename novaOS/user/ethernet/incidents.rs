//! Client de panne, inclus uniquement dans les deux images de non-régression.
use super::*;

impl Client {
    pub(super) fn interrompre_segment(&mut self) -> Result<()> {
        require(self.groupes)?;
        let mut liste = [0; 96];
        let mut trames = Trames::vide();
        for i in 0..8 {
            let dst = trames.ajouter(FRAME)?;
            require(paquet(i as u32, false, dst) == FRAME)?;
            liste[12 * i..12 * i + 12].copy_from_slice(&wire::descriptor(
                i as u16,
                FRAME as u16,
                1,
            ));
        }
        self.envoyer(wire::TX, 1, 8, &liste)?;
        // Le premier TAKE a réussi. Le deuxième segment n'est jamais offert.
        self.c.envoyer_lot(&trames.bytes[..4096], || {
            self.c.envoyer(BYTES, &1u64.to_le_bytes())
        })?;
        let ack = self.c.recevoir(BYTES)?;
        require(ack.len == 8 && ack.payload[..8] == canal::PREMIER_SEGMENT.to_le_bytes())?;
        Err(CANCELLED)
    }
    pub(super) fn verifier_recu_incertain(&mut self) -> Result<()> {
        require(self.groupes)?;
        self.tx(1, 8, 0, None)?;
        require(self.recu(wire::TX, 1)?.payload[6] == 1)?;
        let mut inconnus = false;
        for _ in 0..1000 {
            let r = self.ctrl(wire::QUERY, 0, &1u64.to_le_bytes())?;
            require(r.len == 128 && r.payload[6] == 1)?;
            if r.payload[32..].chunks_exact(12).all(|d| d[2] == 5) {
                inconnus = true;
                break;
            }
        }
        require(inconnus && self.ctrl(wire::ACK, 0, &1u64.to_le_bytes())?.payload[6] == 3)?;
        // Une seconde lecture retrouve exactement les huit reçus incertains.
        let r = self.ctrl(wire::QUERY, 0, &1u64.to_le_bytes())?;
        require(r.len == 128 && r.payload[32..].chunks_exact(12).all(|d| d[2] == 5))?;
        require(self.ctrl(wire::CLOSE, 0, &[])?.len == 32)?;
        self.c.envoyer(EVENT, &self.corr.to_le_bytes())
    }
}
