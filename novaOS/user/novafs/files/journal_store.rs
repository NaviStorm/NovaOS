//! Un objet P9 privé. Sa publication CoW porte ensemble la table des segments,
//! les événements et les deux compteurs qui survivent à une purge.
use super::config_store::crc;
use alloc::vec::Vec;
use novafs::{Device, Engine, Error, Kind, Result, View};

#[path = "../../services/journal_wire.rs"]
mod schema;
#[allow(dead_code)]
#[path = "../../services/journal_storage_wire.rs"]
pub(super) mod wire;
use wire::{Event, RECORD, Status, get, put};

pub const NAME: &str = "\x1fNovaOS-journal";
const STAGE: &str = "\x1fNovaOS-journal-new";
const MAGIC: &[u8; 8] = b"NVJSEG01";
pub const SEGMENTS: usize = 8;
pub const PER_SEGMENT: usize = 16;
const HEADER: usize = 96;
const LIMIT: usize = HEADER + SEGMENTS * PER_SEGMENT * RECORD;
const ESSENTIAL: u64 = 16;
// Un plan P9 de modification de catalogue réserve au plus 64 blocs.
const CATALOG_MAX: u64 = 64;

pub fn hidden(name: &str) -> bool {
    matches!(name, NAME | STAGE)
}

pub struct Journal {
    revision: u64,
    next: u64,
    retention: usize,
    lengths: [u8; SEGMENTS],
    count: usize,
    events: Vec<u8>,
    // L'objet est exclusivement administré par le serveur NovaFS. La révision
    // P9 interdit d'écrire une décision préparée sur une vue déjà dépassée.
    object: Option<(novafs::Stat, usize)>,
}

impl Journal {
    pub fn load<D: Device>(fs: &Engine<D>) -> Result<Self> {
        if fs.atime_policy().is_none() {
            return Err(Error::Invalid("journal réservé à P9"));
        }
        let mut j = Self {
            next: 1,
            revision: 0,
            retention: SEGMENTS,
            lengths: [0; SEGMENTS],
            count: 1,
            events: Vec::new(),
            object: None,
        };
        let st = match fs.lookup_at(fs.root_id(), NAME) {
            Ok(st) => st,
            Err(Error::NotFound) => return Ok(j),
            Err(e) => return Err(e),
        };
        let bad = || Error::Corrupt("journal P9");
        let size = usize::try_from(st.size).map_err(|_| bad())?;
        if st.kind != Kind::File || !(HEADER..=LIMIT).contains(&size) {
            return Err(bad());
        }
        let mut bytes = fs.inspect_id(st.id, 0, size)?;
        if bytes.len() != size
            || &bytes[..8] != MAGIC
            || bytes[8..24] != fs.ids().fs
            || bytes[24..40] != fs.ids().lv
            || bytes[66..72]
                .iter()
                .chain(&bytes[76..HEADER])
                .any(|&v| v != 0)
        {
            return Err(bad());
        }
        j.revision = get(&bytes, 5);
        j.next = get(&bytes, 6);
        j.retention = usize::from(bytes[56]);
        j.count = usize::from(bytes[57]);
        j.lengths.copy_from_slice(&bytes[58..66]);
        if j.revision == 0
            || j.next == 0
            || !(1..=SEGMENTS).contains(&j.retention)
            || !(1..=j.retention).contains(&j.count)
            || j.lengths[j.count..].iter().any(|&n| n != 0)
            || j.lengths[..j.count - 1].contains(&0)
            || j.lengths[..j.count]
                .iter()
                .any(|&n| usize::from(n) > PER_SEGMENT)
        {
            return Err(bad());
        }
        let events: usize = j.lengths.iter().map(|&n| usize::from(n)).sum();
        let used = HEADER + RECORD * events;
        let stored_crc = u32::from_le_bytes(bytes[72..76].try_into().unwrap());
        bytes[72..76].fill(0);
        if used > size
            || events as u64 >= j.next
            || bytes[used..].iter().any(|&b| b != 0)
            || crc(&bytes) != stored_crc
        {
            return Err(bad());
        }
        let first = j.next - events as u64;
        for (i, e) in bytes[HEADER..used].chunks_exact(RECORD).enumerate() {
            if get(e, 1) != first + i as u64
                || !schema::valid(&schema::from_bytes(e.try_into().unwrap()))
            {
                return Err(bad());
            }
        }
        j.events
            .try_reserve_exact(used - HEADER)
            .map_err(|_| Error::NoMemory)?;
        j.events.extend_from_slice(&bytes[HEADER..used]);
        j.object = Some((st, size));
        Ok(j)
    }

    pub fn status(&self) -> Status {
        Status {
            next: self.next,
            first: self.next - (self.events.len() / RECORD) as u64,
            revision: self.revision,
            retention: self.retention as u64,
            segments: self.count as u64,
            quota: (self.retention * PER_SEGMENT * RECORD) as u64,
            bytes: self.events.len() as u64,
        }
    }

    pub fn read(&self, seq: u64) -> Result<Event> {
        let first = self.status().first;
        if seq < first || seq >= self.next {
            return Err(Error::NotFound);
        }
        let start = (seq - first) as usize * RECORD;
        Ok(self.events[start..start + RECORD].try_into().unwrap())
    }

    fn drop_oldest(&mut self) {
        let len = usize::from(self.lengths[0]) * RECORD;
        self.events.drain(..len);
        self.lengths.copy_within(1..self.count, 0);
        self.count -= 1;
        self.lengths[self.count] = 0;
    }

    fn rotate(&mut self) {
        if self.count == self.retention {
            self.drop_oldest();
        }
        self.count += 1;
    }

    /// La mutation consomme la vue privée. Un échec ne laisse aucun état modifié en cache.
    pub fn append<D: Device>(
        mut self,
        fs: &mut Engine<D>,
        expected: u64,
        event: &Event,
    ) -> Result<u64> {
        if get(event, 1) != expected || !schema::valid(&schema::from_bytes(event)) {
            return Err(Error::Invalid("identité ou séquence du journal"));
        }
        // EID = ordinal, BootID et instance/génération authentifiée de l'émetteur.
        for e in self.events.chunks_exact(RECORD) {
            if e[..8] == event[..8] && e[16..32] == event[16..32] && e[48..64] == event[48..64] {
                if e[16..] != event[16..] {
                    return Err(Error::Busy);
                }
                return Ok(get(e, 1));
            }
        }
        if self.next != expected {
            return Err(Error::Busy);
        }
        self.next = self
            .next
            .checked_add(1)
            .ok_or(Error::Limit("séquence journal"))?;
        if usize::from(self.lengths[self.count - 1]) == PER_SEGMENT {
            self.rotate();
        }
        self.events
            .try_reserve_exact(RECORD)
            .map_err(|_| Error::NoMemory)?;
        self.events.extend_from_slice(event);
        self.lengths[self.count - 1] += 1;
        self.save(fs)?;
        Ok(expected)
    }

    pub fn administer<D: Device>(
        mut self,
        fs: &mut Engine<D>,
        op: u64,
        rev: u64,
        count: u64,
    ) -> Result<u64> {
        if self.revision != rev {
            return Err(Error::Busy);
        }
        match op {
            wire::ROTATE if count == 0 => {
                if self.lengths[self.count - 1] == 0 {
                    return Ok(rev);
                }
                self.rotate();
            }
            wire::PURGE if count == 0 => {
                if self.events.is_empty() {
                    return Ok(rev);
                }
                self.events.clear();
                self.lengths = [0; SEGMENTS];
                self.count = 1;
            }
            wire::RETAIN if (1..=SEGMENTS as u64).contains(&count) => {
                if self.retention == count as usize {
                    return Ok(rev);
                }
                self.retention = count as usize;
                while self.count > self.retention {
                    self.drop_oldest();
                }
            }
            _ => return Err(Error::Invalid("politique du journal")),
        }
        self.save(fs)
    }

    fn save<D: Device>(&self, fs: &mut Engine<D>) -> Result<u64> {
        let rev = self
            .revision
            .checked_add(1)
            .ok_or(Error::Limit("révision journal"))?;
        let root = fs.root_id();
        let (id, init, size) = if let Some((st, size)) = &self.object {
            let actual = fs.lookup_at(root, NAME)?;
            if st.id != actual.id || st.revision != actual.revision {
                return Err(Error::Busy);
            }
            (st.id, false, *size)
        } else {
            match fs.lookup_at(root, NAME) {
                Err(Error::NotFound) => (),
                Ok(_) => return Err(Error::Busy),
                Err(e) => return Err(e),
            }
            // La préparation n'est jamais sélectionnée. Ses erreurs de publication
            // n'ont pas pu publier l'événement et nécessitent un remontage du moteur.
            let mut prepare = || -> Result<novafs::Stat> {
                match fs.lookup_at(root, STAGE) {
                    Ok(st) if st.kind == Kind::File && st.size <= LIMIT as u64 => Ok(st),
                    Ok(_) => Err(Error::Corrupt("préparation journal")),
                    Err(Error::NotFound) => {
                        if fs.accounting()?.available_blocks < ESSENTIAL + CATALOG_MAX {
                            return Err(Error::NoSpace);
                        }
                        fs.create_at(root, STAGE, Kind::File)
                    }
                    Err(e) => Err(e),
                }
            };
            let st = (prepare)().map_err(unpublished)?;
            (st.id, true, st.size as usize)
        };
        let size = size.max(HEADER + self.events.len());
        let mut buf = Vec::new();
        buf.try_reserve_exact(size).map_err(|_| Error::NoMemory)?;
        buf.resize(size, 0);
        buf[..8].copy_from_slice(MAGIC);
        buf[8..24].copy_from_slice(&fs.ids().fs);
        buf[24..40].copy_from_slice(&fs.ids().lv);
        put(&mut buf, 5, rev);
        put(&mut buf, 6, self.next);
        buf[56] = self.retention as u8;
        buf[57] = self.count as u8;
        buf[58..66].copy_from_slice(&self.lengths);
        buf[HEADER..HEADER + self.events.len()].copy_from_slice(&self.events);
        let sum = crc(&buf);
        buf[72..76].copy_from_slice(&sum.to_le_bytes());
        let plan = fs.prepare_write_id(id, 0, &buf)?;
        // Le plan réel comptabilise données et métadonnées. Garder une marge pour
        // le renommage inaugural en plus de la réserve P9 et des services essentiels.
        let reserve = ESSENTIAL + if init { CATALOG_MAX } else { 0 };
        if plan.accounting().available_blocks < reserve {
            return Err(Error::NoSpace);
        }
        if init {
            plan.commit().map_err(unpublished)?;
            fs.rename_at(root, STAGE, root, NAME)?;
        } else {
            plan.commit()?;
        }
        Ok(rev)
    }
}

fn unpublished(e: Error) -> Error {
    match e {
        Error::UnknownOutcome | Error::Publication { .. } => Error::Unavailable,
        other => other,
    }
}
