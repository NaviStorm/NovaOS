use super::*;
use ed25519_dalek::Signature;
use novaos_identity::distribution::{self, Machine};
use sha2::{Digest, Sha256};

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Origin {
    Laboratoire,
    Distribution,
}
/// Une ligne du trousseau injecté. Les époques sont inclusives.
pub struct Signer {
    pub publisher: String,
    pub key_id: String,
    pub origin: Origin,
    pub manifest_key: [u8; 32],
    pub elf_key: [u8; 32],
    pub first: u32,
    pub last: u32,
    pub revoked: bool,
    pub priority: u16,
}
pub struct Blob<'a> {
    pub isa: &'a str,
    pub bytes: &'a [u8],
}
fn digest(b: &[u8]) -> [u8; 32] {
    Sha256::digest(b).into()
}
pub(crate) fn verify(
    key: &[u8; 32],
    domain: &[u8],
    payload: &[u8],
    signature: &[u8],
) -> Result<()> {
    if payload.len() > MAX_MANIFEST {
        return Err(Error::Limit);
    }
    let k = distribution::public_key(key).map_err(|_| Error::Signature)?;
    let sig = Signature::from_slice(signature).map_err(|_| Error::Signature)?;
    let mut msg = Vec::with_capacity(domain.len() + 4 + payload.len());
    msg.extend_from_slice(domain);
    msg.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    msg.extend_from_slice(payload);
    k.verify_strict(&msg, &sig).map_err(|_| Error::Signature)
}
impl Policy {
    pub fn new(
        revision: u64,
        origin: Origin,
        floor: u32,
        signers: Vec<Signer>,
        system_key: [u8; 32],
    ) -> Result<Self> {
        if revision == 0 || floor == 0 || signers.len() > 64 {
            return Err(Error::Limit);
        }
        distribution::public_key(&system_key).map_err(|_| Error::Signature)?;
        for (i, k) in signers.iter().enumerate() {
            if !name(&k.publisher)
                || !name(&k.key_id)
                || k.first == 0
                || k.first > k.last
                || signers[..i].iter().any(|p| p.key_id == k.key_id)
                || k.manifest_key == k.elf_key
                || [k.manifest_key, k.elf_key].contains(&system_key)
            {
                return Err(Error::Format);
            }
            for key in [k.manifest_key, k.elf_key] {
                distribution::public_key(&key).map_err(|_| Error::Signature)?;
            }
            if signers[..i].iter().any(|p| {
                p.origin != k.origin
                    && [p.elf_key, p.manifest_key]
                        .iter()
                        .any(|v| [k.elf_key, k.manifest_key].contains(v))
            }) {
                return Err(Error::Denied);
            }
        }
        Ok(Self {
            blocked: Vec::new(),
            revision,
            origin,
            floor,
            signers,
            system_key,
            resources: Vec::new(),
        })
    }
    /// Liste négative de la politique hors ligne, construite avant le catalogue.
    pub fn block_digest(&mut self, digest: [u8; 32]) -> Result<()> {
        if !self.blocked.contains(&digest) {
            if self.blocked.len() >= 64 {
                return Err(Error::Limit);
            }
            self.blocked.push(digest);
        }
        self.resources
            .retain(|r| r.digest != digest && r.record_digest != digest);
        Ok(())
    }
    #[cfg(test)]
    pub(crate) fn empty_test(revision: u64) -> Self {
        Self {
            blocked: Vec::new(),
            revision,
            origin: Origin::Laboratoire,
            floor: 1,
            signers: Vec::new(),
            system_key: [0; 32],
            resources: alloc::vec![Resource {
                id: "profil".into(),
                kind: Kind::SystemProfile,
                version: [1, 0, 0],
                digest: [7; 32],
                record_digest: [9; 32],
                dependencies: Vec::new()
            }],
        }
    }
    /// Le profil et son graphe sont attestés par l'autorité système indépendante.
    pub fn add_resource(&mut self, record: &[u8], artifact: &[u8], sig: &[u8]) -> Result<()> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Record {
            origin: Origin,
            policy_revision: u64,
            security_epoch: u32,
            id: String,
            kind: Kind,
            version: Version,
            #[serde(deserialize_with = "schema::hexbytes")]
            sha256: [u8; 32],
            dependencies: Vec<Dependency>,
        }
        if self.resources.len() >= 64 || artifact.len() > 4 * 1024 * 1024 {
            return Err(Error::Limit);
        }
        verify(
            &self.system_key,
            b"NovaOS/driver-resource/v1\0",
            record,
            sig,
        )?;
        let r: Record = schema::decode(record)?;
        if r.origin != self.origin || r.policy_revision != self.revision {
            return Err(Error::Denied);
        }
        if r.security_epoch < self.floor {
            return Err(Error::Floor);
        }
        if self.blocked.contains(&r.sha256) || self.blocked.contains(&digest(record)) {
            return Err(Error::Revoked);
        }
        if !name(&r.id) || !schema::deps(&r.dependencies) {
            return Err(Error::Format);
        }
        if digest(artifact) != r.sha256 {
            return Err(Error::Digest);
        }
        if self
            .resources
            .iter()
            .any(|n| n.id == r.id && n.version == r.version)
        {
            return Err(Error::Conflict);
        }
        self.resources.push(Resource {
            id: r.id,
            kind: r.kind,
            version: r.version,
            digest: r.sha256,
            record_digest: digest(record),
            dependencies: r.dependencies,
        });
        Ok(())
    }
}
impl<'a> Catalog<'a> {
    pub fn new(policy: &'a Policy) -> Self {
        Self {
            policy,
            entries: Vec::new(),
        }
    }
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    /// Vérifie toutes les ISA annoncées avant l'unique insertion en mémoire.
    pub fn admit(&mut self, raw: &[u8], sig: &[u8], blobs: &[Blob<'_>]) -> Result<[u8; 32]> {
        if blobs.len() > 8 {
            return Err(Error::Limit);
        }
        let m: Manifest = schema::decode(raw)?;
        m.validate()?;
        let policy = self.policy;
        let hash = digest(raw);
        if policy.blocked.contains(&hash)
            || m.image.iter().any(|i| {
                policy.blocked.contains(&i.package_sha256) || policy.blocked.contains(&i.elf_sha256)
            })
        {
            return Err(Error::Revoked);
        }
        let k = policy
            .signers
            .iter()
            .find(|k| {
                k.key_id == m.signing_key_id && k.publisher == m.publisher && k.origin == m.origin
            })
            .ok_or(Error::Denied)?;
        if m.origin != policy.origin {
            return Err(Error::Denied);
        }
        if k.revoked
            || policy
                .signers
                .iter()
                .any(|s| s.revoked && s.manifest_key == k.manifest_key)
        {
            return Err(Error::Revoked);
        }
        if m.security_epoch < policy.floor
            || m.security_epoch < k.first
            || m.security_epoch > k.last
        {
            return Err(Error::Floor);
        }
        verify(&k.manifest_key, b"NovaOS/driver-package/v1\0", raw, sig)?;
        if blobs.len() != m.image.len() {
            return Err(Error::Elf);
        }
        for image in &m.image {
            let blob = blobs
                .iter()
                .find(|b| b.isa == image.isa)
                .ok_or(Error::Elf)?
                .bytes;
            if blob.len() > distribution::MAX_WIRE {
                return Err(Error::Limit);
            }
            if digest(blob) != image.package_sha256 {
                return Err(Error::Digest);
            }
            let machine = match image.isa.as_str() {
                "aarch64" => Machine::Aarch64,
                "x86_64" => Machine::X86_64,
                _ => return Err(Error::Elf),
            };
            let pk = distribution::public_key(&k.elf_key).map_err(|_| Error::Signature)?;
            let pkg = distribution::verify(&pk, machine, blob).map_err(|_| Error::Elf)?;
            if pkg.id() != m.elf_package_id || pkg.version() != m.elf_package_revision {
                return Err(Error::Elf);
            }
            let member = pkg
                .executables()
                .iter()
                .find(|e| e.name == image.member)
                .ok_or(Error::Elf)?;
            if member.sha256 != image.elf_sha256 {
                return Err(Error::Digest);
            }
        }

        if self
            .entries
            .iter()
            .any(|e| e.manifest.id == m.id && e.manifest.publisher != m.publisher)
        {
            return Err(Error::Conflict);
        }
        let slot = match self.entries.binary_search_by(|e| {
            e.manifest
                .id
                .cmp(&m.id)
                .then(e.manifest.version.cmp(&m.version))
        }) {
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
        self.entries
            .try_reserve_exact(1)
            .map_err(|_| Error::Limit)?;
        self.entries.insert(
            slot,
            Entry {
                manifest: m,
                digest: hash,
                priority: k.priority,
            },
        );
        Ok(hash)
    }
}
