use super::*;
/// Un objet possède le TOML exact suivi, si nécessaire, de l'ID déséchappé.
pub(crate) struct Entry {
    object: Vec<u8>,
    end: u16,
    id_offset: u16,
    id_len: u8,
    issuer: u8,
    pub priority: u16,
    pub digest: [u8; 32],
    version: Version,
    pci: Option<(u16, u16)>,
}
const _: () = assert!(size_of::<Entry>() <= 128);
impl Entry {
    pub(crate) fn new(
        raw: &[u8],
        m: &Manifest,
        digest: [u8; 32],
        issuer: u8,
        priority: u16,
    ) -> Result<Self> {
        let id = m.id.as_bytes();
        let pos = raw.windows(id.len()).position(|w| w == id);
        let mut object = Vec::new();
        object
            .try_reserve_exact(raw.len() + if pos.is_none() { id.len() } else { 0 })
            .map_err(|_| Error::NoMemory)?;
        object.extend_from_slice(raw);
        if pos.is_none() {
            object.extend_from_slice(id);
        }
        let pci = match m.matches.as_slice() {
            [
                Bus::Pci {
                    vendor: Some(v),
                    device: Some(d),
                    ..
                },
            ] => Some((*v, *d)),
            _ => None,
        };
        Ok(Self {
            object,
            end: raw.len() as u16,
            id_offset: pos.unwrap_or(raw.len()) as u16,
            id_len: id.len() as u8,
            issuer,
            priority,
            digest,
            version: m.version,
            pci,
        })
    }
    pub fn raw(&self) -> &[u8] {
        &self.object[..usize::from(self.end)]
    }
    pub fn id(&self) -> &[u8] {
        let start = usize::from(self.id_offset);
        &self.object[start..start + usize::from(self.id_len)]
    }
    pub fn possible(&self, target: &Bus) -> bool {
        match self.pci {
            Some((v, d)) => {
                matches!(target,Bus::Pci{vendor:Some(a),device:Some(b),..} if *a==v && *b==d)
            }
            None => true,
        }
    }
}
impl Catalog<'_> {
    pub fn memory_bytes(&self) -> (usize, usize) {
        (
            self.entries.capacity() * size_of::<Entry>(),
            self.entries.iter().map(|e| e.object.capacity()).sum(),
        )
    }
    pub(crate) fn publish(
        &mut self,
        raw: &[u8],
        m: &Manifest,
        hash: [u8; 32],
        issuer: u8,
        priority: u16,
    ) -> Result<[u8; 32]> {
        let publisher = &self.policy.signers[issuer as usize].publisher;
        if self.entries.iter().any(|e| {
            e.id() == m.id.as_bytes()
                && self.policy.signers[usize::from(e.issuer)].publisher != *publisher
        }) {
            return Err(Error::Conflict);
        }
        let slot = match self
            .entries
            .binary_search_by(|e| e.id().cmp(m.id.as_bytes()).then(e.version.cmp(&m.version)))
        {
            Ok(i) => {
                return if self.entries[i].digest == hash {
                    Ok(hash)
                } else {
                    Err(Error::Conflict)
                };
            }
            Err(i) => i,
        };
        if self.entries.len() >= 64 {
            return Err(Error::Limit);
        }
        let item = Entry::new(raw, m, hash, issuer, priority)?;
        self.entries
            .try_reserve_exact(1)
            .map_err(|_| Error::NoMemory)?;
        self.entries.insert(slot, item);
        Ok(hash)
    }
}
