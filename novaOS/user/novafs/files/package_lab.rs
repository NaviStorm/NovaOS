//! Provisionneur QEMU. Seule la voie privée du vérificateur atteint ces opérations.
use super::*;
use novaos_identity::{
    Fault, Registry,
    distribution::{self, Action, Machine},
    lab::Ledger,
};
const DOMAIN: [u8; 16] = [7; 16];
const PUB: &[u8; 32] = include_bytes!(env!("NOVA_PACKAGE_PUBLIC"));

pub(super) struct Upload {
    owner: [u8; 16],
    session: [u8; 32],
    request: [u8; 16],
    size: usize,
    bytes: Vec<u8>,
}

fn machine() -> Machine {
    #[cfg(target_arch = "aarch64")]
    {
        Machine::Aarch64
    }
    #[cfg(target_arch = "x86_64")]
    {
        Machine::X86_64
    }
}

fn fault(e: Fault) -> u64 {
    match e {
        Fault::Denied => rt::DENIED,
        Fault::BadInput => rt::BAD_ARGUMENT,
        Fault::Limit => rt::LIMIT,
        Fault::Uncertain | Fault::Storage(Error::Publication { .. } | Error::UnknownOutcome) => {
            w::UNCERTAIN
        }
        Fault::Storage(e) => error(e),
        _ => rt::REFUSED,
    }
}

impl Session<'_, '_> {
    pub(super) fn package_request(&mut self, q: &Frame, a: &mut Frame) -> rt::Result<()> {
        match &q[..8] {
            b"NXPKDATA" => {
                let u = self.upload.as_mut().ok_or(rt::DENIED)?;
                let at = u32::from_le_bytes(q[8..12].try_into().unwrap()) as usize;
                let n = u16::from_le_bytes(q[12..14].try_into().unwrap()) as usize;
                if n == 0
                    || n > 96
                    || u.bytes.len() != at
                    || at.checked_add(n).is_none_or(|end| end > u.size)
                    || q[14..16] != [0; 2]
                    || q[16..32] != u.request
                    || q[32 + n..].iter().any(|b| *b != 0)
                {
                    return Err(rt::BAD_ARGUMENT);
                }
                u.bytes.extend_from_slice(&q[32..32 + n]);
                return Ok(());
            }
            b"NXPKDROP" => {
                self.upload = None;
                return Ok(());
            }
            _ => (),
        }
        let owner = q[8..24].try_into().unwrap();
        let session: [u8; 32] = q[24..56].try_into().unwrap();
        if session == [0; 32] || self.failed.is_some() {
            return Err(rt::DENIED);
        }
        let mut ledger = Ledger::new(self.dev.identity_authority().map_err(error)?);
        let mut reg = Registry::open(self.fs.as_mut().ok_or(rt::CLOSED)?, &mut ledger, DOMAIN)
            .map_err(fault)?;
        let grant = reg.grant_attested(owner).map_err(fault)?;
        if q[56..88] != grant.generation_digest() {
            return Err(rt::REVOKED);
        }
        let key = distribution::public_key(PUB).map_err(|_| rt::DENIED)?;
        match &q[..8] {
            b"NXPKVIEW" => {
                let n = q[88] as usize;
                if n == 0 || n > 39 || q[89 + n..].iter().any(|b| *b != 0) {
                    return Err(rt::BAD_ARGUMENT);
                }
                let name = core::str::from_utf8(&q[89..89 + n]).map_err(|_| rt::BAD_ARGUMENT)?;
                let loaded = reg
                    .load_package(grant, &key, machine(), name)
                    .map_err(fault)?;
                a[8..24].copy_from_slice(&loaded.installation);
                a[24..56].copy_from_slice(&loaded.digest);
            }
            b"NXPKHIST" => {
                if !reg.inspect(grant, owner).map_err(fault)?.admin || q[104..] != [0; 24] {
                    return Err(rt::DENIED);
                }
                let request = q[88..104].try_into().unwrap();
                if request == [0; 16] {
                    return Err(rt::BAD_ARGUMENT);
                }
                if let Some(h) = reg.outcome(request).map_err(fault)? {
                    a[8..16].copy_from_slice(&h.generation.to_le_bytes());
                }
            }
            b"NXPKBEG1" => {
                if !self.writable || !reg.inspect(grant, owner).map_err(fault)?.admin {
                    return Err(rt::DENIED);
                }
                if self.upload.is_some() {
                    return Err(w::BUSY);
                }
                let request = q[88..104].try_into().unwrap();
                let size = u32::from_le_bytes(q[104..108].try_into().unwrap()) as usize;
                if request == [0; 16]
                    || size == 0
                    || size > distribution::MAX_WIRE
                    || q[108..] != [0; 20]
                {
                    return Err(rt::BAD_ARGUMENT);
                }
                if reg.outcome(request).map_err(fault)?.is_some() {
                    return Err(rt::REFUSED);
                }
                let mut bytes = Vec::new();
                bytes.try_reserve_exact(size).map_err(|_| rt::NO_MEMORY)?;
                self.upload = Some(Upload {
                    owner,
                    session,
                    request,
                    size,
                    bytes,
                });
            }
            b"NXPKEND1" => {
                if !reg.inspect(grant, owner).map_err(fault)?.admin {
                    return Err(rt::DENIED);
                }
                let u = self.upload.as_ref().ok_or(rt::DENIED)?;
                if u.owner != owner
                    || u.session != session
                    || u.bytes.len() != u.size
                    || q[88..] != [0; 40]
                {
                    return Err(rt::DENIED);
                }
                let u = self.upload.take().unwrap();
                let head = reg
                    .publish_package(grant, u.request, Action::Install, &key, machine(), &u.bytes)
                    .map_err(fault)?;
                a[8..16].copy_from_slice(&head.generation.to_le_bytes());
            }
            _ => return Err(rt::BAD_ARGUMENT),
        }
        Ok(())
    }
}
