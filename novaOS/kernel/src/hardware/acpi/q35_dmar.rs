//! Politique du seul laboratoire Q35. Une DMAR syntaxiquement valide mais hors
//! profil reste visible dans l'inventaire générique et n'accorde aucun accès.
use super::{u16_at, u64_at};

#[derive(Debug)]
pub(crate) struct Q35Dmar {
    pub(crate) base: u64,
    pub(crate) host_bits: u8,
    pub(crate) explicit: bool,
}

pub(super) fn recognize(table: &[u8]) -> Option<Q35Dmar> {
    // discover() a déjà figé les octets, vérifié le checksum et l'unicité de DMAR.
    let record = table.get(48..)?;
    if table[8] != 1
        || table[36] != 47
        || table[37] & !1 != 0
        || !table[38..48].iter().all(|v| *v == 0)
        || u16_at(record, 0).ok()? != 0
        || usize::from(u16_at(record, 2).ok()?) != record.len()
        || *record.get(4)? > 1
        || *record.get(5)? != 0
        || u16_at(record, 6).ok()? != 0
        || u64_at(record, 8).ok()? != 0xfed9_0000
    {
        return None;
    }
    let include_all = record[4] == 1;
    let scopes = record.get(16..)?;
    if !scopes.len().is_multiple_of(8) {
        return None;
    }
    let mut endpoints = 0u8;
    let mut ioapic = false;
    // Pas de pont ni d'alias dans cette VM. On rejette aussi toute seconde
    // unité, réservation RMRR ou nouvelle portée dont l'effet est inconnu.
    for s in scopes.chunks_exact(8) {
        match s {
            [3, 8, 0, 0, 0, 255, 0, 0] if !ioapic => ioapic = true,
            [1, 8, 0, 0, 0, 0, dev, fun] if !include_all => {
                let index = [(0, 0), (6, 0), (31, 0), (31, 2), (31, 3)]
                    .iter()
                    .position(|pair| pair == &(*dev, *fun))?;
                if endpoints & (1 << index) != 0 {
                    return None;
                }
                endpoints |= 1 << index;
            }
            _ => return None,
        }
    }
    // Le générateur QEMU courant énumère tous les endpoints explicitement.
    // La forme historique INCLUDE_PCI_ALL est sûre uniquement pour cet unique
    // DRHD sans réservation et le même inventaire PCI fermé revérifié par l'appelant.
    if !ioapic || (!include_all && endpoints != 0x1f) {
        return None;
    }
    Some(Q35Dmar {
        base: u64_at(record, 8).ok()?,
        host_bits: table[36] + 1,
        explicit: !include_all,
    })
}
