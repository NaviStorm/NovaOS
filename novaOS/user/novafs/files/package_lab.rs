//! Provisionneur QEMU. Seule la voie privée du vérificateur atteint ces opérations.
use super::*;
use novaos_identity::{
    Authority, Fault, Head, Id, Registry,
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

// Le binaire nominal ne contient aucun point de coupure. Les deux images
// d'injection arrêtent le service à la frontière durable du registre.
struct Decision<A>(A);
impl<A: Authority> Authority for Decision<A> {
    fn current(&self) -> novaos_identity::Result<Head> {
        self.0.current()
    }
    fn outcome(&self, request: Id) -> novaos_identity::Result<Option<Head>> {
        self.0.outcome(request)
    }
    fn decide(&mut self, old: Head, new: Head) -> novaos_identity::Result<()> {
        fn stop(marker: &[u8]) -> ! {
            rt::call(
                rt::DEBUG_WRITE,
                marker.as_ptr() as u64,
                marker.len() as u64,
                0,
            );
            rt::exit(42)
        }
        if option_env!("NOVA_PACKAGE_CUT") == Some("avant") {
            stop(b"NXPK interruption avant decision");
        }
        self.0.decide(old, new)?;
        if option_env!("NOVA_PACKAGE_CUT") == Some("apres") {
            stop(b"NXPK interruption apres decision");
        }
        Ok(())
    }
}

impl Session<'_, '_> {
    pub(super) fn package_request(&mut self, q: &Frame, a: &mut Frame) -> rt::Result<()> {
        let mut stored;
        let name;
        let q = if &q[..8] == b"NXPKNAME" {
            let n = q[24] as usize;
            if n == 0 || n > 63 || q[25 + n..].iter().any(|b| *b != 0) {
                return Err(rt::BAD_ARGUMENT);
            }
            name = core::str::from_utf8(&q[25..25 + n]).map_err(|_| rt::BAD_ARGUMENT)?;
            stored = self
                .executable_request
                .take()
                .filter(|old| old[88..104] == q[8..24])
                .ok_or(rt::DENIED)?;
            stored[..8].copy_from_slice(b"NXPKLOAD");
            &stored
        } else {
            name = "";
            q
        };
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
                self.executable_request = None;
                return Ok(());
            }
            _ => (),
        }
        let owner = q[8..24].try_into().unwrap();
        let session: [u8; 32] = q[24..56].try_into().unwrap();
        if session == [0; 32] || self.failed.is_some() {
            return Err(rt::DENIED);
        }
        let mut ledger = Decision(Ledger::new(self.dev.identity_authority().map_err(error)?));
        let mut reg = Registry::open(self.fs.as_mut().ok_or(rt::CLOSED)?, &mut ledger, DOMAIN)
            .map_err(fault)?;
        let grant = reg.grant_attested(owner).map_err(fault)?;
        if q[56..88] != grant.generation_digest() {
            return Err(rt::REVOKED);
        }
        let key = distribution::public_key(PUB).map_err(|_| rt::DENIED)?;
        match &q[..8] {
            b"NXPKBEGX" => {
                if self.executable_request.is_some() || q[88..104] == [0; 16] || q[104..] != [0; 24]
                {
                    return Err(rt::REFUSED);
                }
                self.executable_request = Some(*q);
            }
            b"NXPKLOAD" => {
                let loaded = reg
                    .load_package(grant, &key, machine(), name)
                    .map_err(fault)?;
                let principal = loaded.principal.ok_or(rt::DENIED)?;
                let app = loaded.application.ok_or(rt::DENIED)?;
                let tx = rt::config(19);
                let frame = |op: &[u8; 8]| {
                    let mut f = [0; 128];
                    f[..8].copy_from_slice(op);
                    f[8..24].copy_from_slice(&q[88..104]);
                    f
                };
                let mut f = frame(b"NXLDMETA");
                f[24..40].copy_from_slice(&owner);
                f[40..72].copy_from_slice(&q[56..88]);
                f[72..88].copy_from_slice(&loaded.volume);
                f[88..104].copy_from_slice(&principal);
                f[104..112].copy_from_slice(&app.to_le_bytes());
                f[112..120].copy_from_slice(&loaded.object.to_le_bytes());
                f[120..128].copy_from_slice(&loaded.revision.to_le_bytes());
                rt::send_for(tx, 1, &f, 250)?;
                let mut f = frame(b"NXLDSIZE");
                f[24..32].copy_from_slice(&(loaded.bytes.len() as u64).to_le_bytes());
                f[32..64].copy_from_slice(&loaded.digest);
                f[64..72].copy_from_slice(&loaded.generation.to_le_bytes());
                rt::send_for(tx, 2, &f, 250)?;
                for (index, chunk) in loaded.bytes.chunks(4096).enumerate() {
                    let mut f = frame(b"NXLDDATA");
                    f[24..32].copy_from_slice(&((index * 4096) as u64).to_le_bytes());
                    f[32..40].copy_from_slice(&(chunk.as_ptr() as u64).to_le_bytes());
                    f[40..48].copy_from_slice(&(chunk.len() as u64).to_le_bytes());
                    rt::send_for(tx, 3, &f, 250)?;
                }
                rt::send_for(tx, 4, &frame(b"NXLDSEAL"), 250)?;
            }
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
