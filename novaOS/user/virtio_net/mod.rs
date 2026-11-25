//! Partie non privilégiée du pilote Ethernet moderne. Aucune adresse DMA.
pub mod init;
#[cfg(virtio_net_probe)]
pub mod probe;

pub const SLOTS: u16 = 16;
pub const SLOT_BYTES: usize = 2048;
const HEADER: usize = 12;
const MIN_ETH: usize = 14;
const MAX_ETH: usize = 1514;
const MIN_WIRE: usize = 60;
const VERSION_1: u64 = 1 << 32;
const ACCESS_PLATFORM: u64 = 1 << 33;
const MAC: u64 = 1 << 5;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum Error {
    Features,
    Frame,
    Full,
    Quarantined,
}

pub fn features(offered: u64) -> Result<u64, Error> {
    let required = VERSION_1 | ACCESS_PLATFORM | MAC;
    if offered & required != required {
        return Err(Error::Features);
    }
    Ok(required)
}

/// Copie une trame dans la borne Ethernet du profil (hors FCS).
/// Le remplissage Ethernet court et les champs d'offload transmis sont nuls.
pub fn encode_tx(frame: &[u8], slot: &mut [u8]) -> Result<usize, Error> {
    if !(MIN_ETH..=MAX_ETH).contains(&frame.len()) {
        return Err(Error::Frame);
    }
    let bytes = HEADER + frame.len().max(MIN_WIRE);
    let out = slot.get_mut(..bytes).ok_or(Error::Frame)?;
    out[..HEADER].fill(0);
    out[HEADER..HEADER + frame.len()].copy_from_slice(frame);
    out[HEADER + frame.len()..].fill(0);
    Ok(bytes)
}

/// Le courtier remet une copie limitée à used.len après reprise du descripteur.
/// Ne jamais livrer la fin inutilisée du tampon au consommateur.
pub fn decode_rx(used: &[u8]) -> Result<&[u8], Error> {
    if !(HEADER + MIN_ETH..=HEADER + MAX_ETH).contains(&used.len())
        || used[0] != 0
        || used[1] != 0
        || used[10..12] != [1, 0]
    {
        return Err(Error::Frame);
    }
    Ok(&used[HEADER..])
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Receive,
    Transmit,
}

/// Une instance par file, propriétaire unique dans la boucle d'événements.
/// Le courtier reste responsable des barrières DMA et de l'authentification
/// des complétions. Une incertitude de publication garde le slot réservé.
pub struct Queue {
    direction: Direction,
    busy: u16,
    reaped: u16,
    quarantined: bool,
}

impl Queue {
    pub const fn new(direction: Direction) -> Self {
        Self {
            direction,
            busy: 0,
            reaped: 0,
            quarantined: false,
        }
    }

    pub fn reserve(&mut self) -> Result<u16, Error> {
        if self.quarantined {
            return Err(Error::Quarantined);
        }
        let slot = self.busy.trailing_ones() as u16;
        if slot == SLOTS {
            return Err(Error::Full);
        }
        self.busy |= 1 << slot;
        Ok(slot)
    }

    pub fn pending(&self) -> u16 {
        self.busy.count_ones() as u16
    }

    /// Un réveil peut être partagé, coalescé ou surnuméraire.
    pub fn available_completions(&mut self, used_idx: u16) -> Result<u16, Error> {
        let count = used_idx.wrapping_sub(self.reaped);
        if self.quarantined || count > self.pending() {
            return self.quarantine();
        }
        Ok(count)
    }

    /// Appeler pour chaque entrée du lot effectivement lu dans le used ring.
    pub fn complete(&mut self, id: u32, written: u32) -> Result<(), Error> {
        if self.quarantined || id >= u32::from(SLOTS) || self.busy & (1 << id) == 0 {
            return self.quarantine();
        }
        let max = match self.direction {
            Direction::Receive => SLOT_BYTES as u32,
            Direction::Transmit => 0,
        };
        if written > max {
            return self.quarantine();
        }
        self.busy &= !(1 << id);
        self.reaped = self.reaped.wrapping_add(1);
        Ok(())
    }

    fn quarantine<T>(&mut self) -> Result<T, Error> {
        self.quarantined = true;
        Err(Error::Quarantined)
    }
}

#[cfg(test)]
mod tests;
