//! Autorité de laboratoire sur périphérique dédié à écrivain exclusif.
//! Les SHA-256 détectent les erreurs accidentelles, pas un hôte malveillant.
//! La restauration simultanée de ce support et du P9 peut restaurer un droit.
use super::*;
use novafs::Block;

pub struct Ledger<D>(D);
impl<D: Device> Ledger<D> {
    pub fn new(device: D) -> Self {
        Self(device)
    }
    pub fn into_device(self) -> D {
        self.0
    }

    fn scan(&self, request: Option<Id>) -> Result<(Option<Head>, Option<Head>, u64)> {
        if self.0.blocks() == 0 || self.0.blocks() > 1024 {
            return Err(Fault::Limit);
        }
        let mut last: Option<Head> = None;
        let mut found = None;
        let mut chain = [0; 32];
        let mut next = 0;
        for index in 0..self.0.blocks() {
            let b = self.0.read(index).map_err(|_| Fault::Unavailable)?;
            if b.iter().all(|&v| v == 0) {
                continue;
            }
            if index != next || &b[..8] != b"NVAUTHL1" || b[8..40] != chain {
                return Err(Fault::Corrupt);
            }
            let end = b.len() - 32;
            chain = Sha256::digest(&b[..end]).into();
            if b[end..] != chain {
                return Err(Fault::Corrupt);
            }
            let size = u32::from_le_bytes(b[40..44].try_into().unwrap()) as usize;
            let bytes = b[44..end].get(..size).ok_or(Fault::Corrupt)?;
            let head: Head = serde_json::from_slice(bytes).map_err(|_| Fault::Corrupt)?;
            if head.generation != index
                || last.is_some_and(|h| {
                    h.domain != head.domain
                        || h.volume != head.volume
                        || h.objects != head.objects
                        || h.recovery != head.recovery
                })
            {
                return Err(Fault::Corrupt);
            }
            if Some(head.request) == request {
                if found.is_some() {
                    return Err(Fault::Corrupt);
                }
                found = Some(head);
            }
            last = Some(head);
            next += 1;
        }
        Ok((last, found, next))
    }
    fn append(&mut self, h: Head) -> Result<()> {
        let i = h.generation;
        if i >= self.0.blocks() {
            return Err(Fault::Limit);
        }
        let mut b: Block = [0; 4096];
        b[..8].copy_from_slice(b"NVAUTHL1");
        if i > 0 {
            b[8..40].copy_from_slice(&self.0.read(i - 1)?[4064..]);
        }
        let bytes = serde_json::to_vec(&h).map_err(|_| Fault::Limit)?;
        b[40..44].copy_from_slice(&(bytes.len() as u32).to_le_bytes());
        b[44..44 + bytes.len()].copy_from_slice(&bytes);
        let hash = Sha256::digest(&b[..4064]);
        b[4064..].copy_from_slice(&hash);
        self.0.write(i, &b).map_err(|_| Fault::Uncertain)?;
        self.0.flush().map_err(|_| Fault::Uncertain)?;
        Ok(())
    }
}
impl<D: Device> Authority for Ledger<D> {
    fn current(&self) -> Result<Head> {
        self.scan(None)?.0.ok_or(Fault::NotEnrolled)
    }
    fn outcome(&self, request: Id) -> Result<Option<Head>> {
        Ok(self.scan(Some(request))?.1)
    }
    fn decide(&mut self, previous: Head, next: Head) -> Result<()> {
        let (last, found, index) = self.scan(Some(next.request))?;
        if last != Some(previous)
            || found.is_some()
            || next.generation != index
            || next.objects != previous.objects
            || next.domain != previous.domain
            || next.volume != previous.volume
            || next.recovery != previous.recovery
        {
            return Err(Fault::Conflict);
        }
        self.append(next)
    }
}
impl<D: Device> Provisioner for Ledger<D> {
    fn ensure_unused(&self) -> Result<()> {
        if self.scan(None)?.0.is_some() {
            Err(Fault::NotNew)
        } else {
            Ok(())
        }
    }
    fn bind_new(&mut self, genesis: Head) -> Result<()> {
        self.ensure_unused()?;
        if genesis.generation != 0 || genesis.request != [0; 16] || genesis.recovery == [0; 16] {
            return Err(Fault::BadInput);
        }
        self.append(genesis)
    }
}
