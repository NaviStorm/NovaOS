//! Longueurs et octets des huit crédits. Les segments IPC peuvent couper une
//! trame. L'admission et la publication attendent la réception du lot entier.
use super::*;
const PAGE: usize = 4096;
const TAG_LOT: u64 = 0x45544840;
pub struct Trames {
    pub bytes: [u8; 8 * FRAME],
    ends: [usize; 9],
    n: usize,
}
impl Trames {
    pub fn vide() -> Self {
        Self {
            bytes: [0; 8 * FRAME],
            ends: [0; 9],
            n: 0,
        }
    }
    pub fn ajouter(&mut self, len: usize) -> Result<&mut [u8]> {
        require(self.n < 8 && (14..=FRAME).contains(&len))?;
        let start = self.ends[self.n];
        self.n += 1;
        self.ends[self.n] = start + len;
        Ok(&mut self.bytes[start..start + len])
    }
    pub fn trame(&self, i: usize) -> &[u8] {
        &self.bytes[self.ends[i]..self.ends[i + 1]]
    }
    pub fn longueur(&self) -> usize {
        self.ends[self.n]
    }
    pub fn recevoir(&mut self, canal: &Canal, desc: &[u8], groupes: bool, tx: bool) -> Result<()> {
        require(desc.len().is_multiple_of(12) && desc.len() <= 96 && self.n == 0)?;
        for (i, d) in desc.chunks_exact(12).enumerate() {
            let word = |at| u16::from_le_bytes([d[at], d[at + 1]]);
            let dst = self.ajouter(usize::from(word(2)))?;
            if !groupes {
                let key = if tx {
                    i as u64 + 1
                } else {
                    u64::from(word(0)) + 1
                };
                canal.donnees(false, key, dst)?;
            }
        }
        if groupes {
            let len = self.longueur();
            canal.recevoir_lot(&mut self.bytes[..len])?;
        }
        Ok(())
    }
}
impl Canal {
    fn transfert(&self, op: u64, i: usize, addr: u64, len: usize) -> Result<()> {
        require(self.boite && (1..=PAGE).contains(&len))?;
        let desc = BulkV1 {
            version: VERSION,
            size: 32,
            tag: TAG_LOT + i as u64,
            address: addr,
            length: len as u64,
        };
        require(value(call(op, self.page, (&raw const desc) as u64, 32))? == len as u64)
    }
    // Le premier PUT précède le message de contrôle qui annonce le lot. Chaque
    // segment suivant attend le TAKE précédent. Le prochain échange de protocole
    // confirme la consommation finale avant réutilisation par le même producteur.
    pub fn envoyer_lot(&self, bytes: &[u8], controle: impl FnOnce() -> Result<()>) -> Result<()> {
        require(bytes.len() <= 8 * FRAME)?;
        let mut chunks = bytes.chunks(PAGE);
        if let Some(first) = chunks.next() {
            self.transfert(IPC_PUT, 0, first.as_ptr() as u64, first.len())?;
        }
        controle()?;
        for (i, chunk) in chunks.enumerate() {
            let key = TAG_LOT + i as u64;
            let ack = self.recevoir(BYTES)?;
            require(ack.len == 8 && ack.payload[..8] == key.to_le_bytes())?;
            self.transfert(IPC_PUT, i + 1, chunk.as_ptr() as u64, chunk.len())?;
            self.envoyer(BYTES, &(key + 1).to_le_bytes())?;
        }
        Ok(())
    }
    pub fn recevoir_lot(&self, bytes: &mut [u8]) -> Result<()> {
        require(bytes.len() <= 8 * FRAME)?;
        let n = bytes.len().div_ceil(PAGE);
        for (i, chunk) in bytes.chunks_mut(PAGE).enumerate() {
            let key = TAG_LOT + i as u64;
            if i != 0 {
                let msg = self.recevoir(BYTES)?;
                require(msg.len == 8 && msg.payload[..8] == key.to_le_bytes())?;
            }
            self.transfert(IPC_TAKE, i, chunk.as_mut_ptr() as u64, chunk.len())?;
            if i + 1 < n {
                self.envoyer(BYTES, &key.to_le_bytes())?;
            }
        }
        Ok(())
    }
}
