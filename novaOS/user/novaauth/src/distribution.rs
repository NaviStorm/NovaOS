//! Contrat de paquet de qualification. Seule la clé publique vient de l'autorité.
//! Le provisionneur doit garder les octets en mémoire privée jusqu'à publication.
use super::{Hash, Id};
use alloc::vec::Vec;
use core::str;
use ed25519_dalek::{Signature, VerifyingKey};
use sha2::{Digest, Sha256};

pub const MAGIC: &[u8; 16] = b"NovaOS/ELF/pkg01";
pub const MAX_MEMBERS: usize = 8;
pub const MAX_NAME: usize = 63;
pub const MAX_IMAGE: usize = 4 * 1024 * 1024;
const HEADER: usize = 40;
const MAX_MANIFEST: usize = HEADER + MAX_MEMBERS * (5 + 32 + MAX_NAME);

pub const MAX_WIRE: usize = 4 + MAX_MANIFEST + 64 + MAX_IMAGE;

pub fn public_key(bytes: &[u8; 32]) -> core::result::Result<VerifyingKey, Error> {
    let key = VerifyingKey::from_bytes(bytes).map_err(|_| Error::Signature)?;
    if key.is_weak() {
        return Err(Error::Signature);
    }
    Ok(key)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    Signature,
    Architecture,
    Format,
    Digest,
    Limit,
}
type Result<T> = core::result::Result<T, Error>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u16)]
pub enum Machine {
    X86_64 = 62,
    Aarch64 = 183,
}

pub struct Executable<'a> {
    pub name: &'a str,
    pub image: &'a [u8],
    pub sha256: Hash,
}

pub struct VerifiedPackage<'a> {
    id: Id,
    version: u32,
    machine: Machine,
    manifest_hash: Hash,
    members: Vec<Executable<'a>>,
}

impl<'a> VerifiedPackage<'a> {
    pub fn id(&self) -> Id {
        self.id
    }
    pub fn version(&self) -> u32 {
        self.version
    }
    pub fn machine(&self) -> Machine {
        self.machine
    }
    pub fn manifest_hash(&self) -> Hash {
        self.manifest_hash
    }
    pub fn executables(&self) -> &[Executable<'a>] {
        &self.members
    }
}

fn take<'a>(data: &mut &'a [u8], count: usize) -> Result<&'a [u8]> {
    let (v, rest) = data.split_at_checked(count).ok_or(Error::Format)?;
    *data = rest;
    Ok(v)
}

fn name(bytes: &[u8]) -> Result<&str> {
    if bytes.is_empty()
        || bytes.len() > MAX_NAME
        || !bytes[0].is_ascii_alphanumeric()
        || !bytes
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(b))
    {
        return Err(Error::Format);
    }
    str::from_utf8(bytes).map_err(|_| Error::Format)
}

/// Le format de liaison complet et les protections de segments sont revalidés par le chargeur.
fn check_machine(image: &[u8], machine: Machine) -> Result<()> {
    let header = image.get(..64).ok_or(Error::Format)?;
    if &header[..7] != b"\x7fELF\x02\x01\x01"
        || header[16..18] != 2u16.to_le_bytes()
        || header[20..24] != 1u32.to_le_bytes()
        || header[52..54] != 64u16.to_le_bytes()
    {
        return Err(Error::Format);
    }
    if header[18..20] != (machine as u16).to_le_bytes() {
        return Err(Error::Architecture);
    }
    Ok(())
}

/// Le trousseau fournit la clé active. Le paquet ne choisit aucune clé de confiance.
pub fn verify<'a>(
    key: &VerifyingKey,
    machine: Machine,
    bytes: &'a [u8],
) -> Result<VerifiedPackage<'a>> {
    let mut wire = bytes;
    let len = u32::from_le_bytes(take(&mut wire, 4)?.try_into().unwrap()) as usize;
    if !(HEADER..=MAX_MANIFEST).contains(&len) {
        return Err(Error::Format);
    }
    let manifest = take(&mut wire, len)?;
    let signature = Signature::from_bytes(take(&mut wire, 64)?.try_into().unwrap());
    if wire.len() > MAX_IMAGE {
        return Err(Error::Limit);
    }
    if key.is_weak() || key.verify_strict(manifest, &signature).is_err() {
        return Err(Error::Signature);
    }

    let mut m = manifest;
    if take(&mut m, 16)? != MAGIC {
        return Err(Error::Format);
    }
    let id: Id = take(&mut m, 16)?.try_into().unwrap();
    let version = u32::from_le_bytes(take(&mut m, 4)?.try_into().unwrap());
    if id == [0; 16] || version == 0 {
        return Err(Error::Format);
    }
    if take(&mut m, 2)? != (machine as u16).to_le_bytes() {
        return Err(Error::Architecture);
    }
    let count = u16::from_le_bytes(take(&mut m, 2)?.try_into().unwrap()) as usize;
    if !(1..=MAX_MEMBERS).contains(&count) {
        return Err(Error::Limit);
    }
    let mut members: Vec<Executable> = Vec::new();
    members.try_reserve_exact(count).map_err(|_| Error::Limit)?;
    for _ in 0..count {
        let n = take(&mut m, 1)?[0] as usize;
        let entry = name(take(&mut m, n)?)?;
        if members
            .last()
            .is_some_and(|previous| previous.name >= entry)
        {
            return Err(Error::Format);
        }
        let size = u32::from_le_bytes(take(&mut m, 4)?.try_into().unwrap()) as usize;
        let expected: Hash = take(&mut m, 32)?.try_into().unwrap();
        let image = take(&mut wire, size)?;
        let measured: Hash = Sha256::digest(image).into();
        if measured != expected {
            return Err(Error::Digest);
        }
        check_machine(image, machine)?;
        members.push(Executable {
            name: entry,
            image,
            sha256: measured,
        });
    }
    if !m.is_empty() || !wire.is_empty() {
        return Err(Error::Format);
    }
    Ok(VerifiedPackage {
        id,
        version,
        machine,
        manifest_hash: Sha256::digest(manifest).into(),
        members,
    })
}

use super::{Authority, Device, Fault, Grant, Head, Kind, Registry, Status};
use alloc::{format, string::String};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(super) struct Release {
    id: Id,
    package: Id,
    version: u32,
    machine: u16,
    signer: Hash,
    manifest: Hash,
    object: u64,
    revision: u64,
    entries: Vec<Entry>,
    active: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct Entry {
    name: String,
    digest: Hash,
    start: u64,
    size: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Install,
    Update(Id),
}

#[derive(Debug)]
pub struct Loaded {
    pub installation: Id,
    pub manifest: Hash,
    pub object: u64,
    pub revision: u64,
    pub offset: u64,
    pub digest: Hash,
    pub bytes: Vec<u8>,
}

fn map_verify(error: Error) -> Fault {
    match error {
        Error::Limit => Fault::Limit,
        Error::Signature | Error::Digest => Fault::Denied,
        Error::Architecture | Error::Format => Fault::BadInput,
    }
}

impl<D: Device, A: Authority> Registry<'_, D, A> {
    /// Entrée privée du provisionneur. La racine de confiance est fournie par le service.
    /// Tant que l'autorité n'a pas décidé, aucun nom préparé n'est résoluble.
    pub fn publish_package(
        &mut self,
        holder: Grant,
        request: Id,
        action: Action,
        key: &VerifyingKey,
        target: Machine,
        bytes: &[u8],
    ) -> super::Result<Head> {
        let verified = verify(key, target, bytes).map_err(map_verify)?;
        let (head, mut state) = self.view()?;
        if holder.head != head || request == [0; 16] || self.authority.outcome(request)?.is_some() {
            return Err(Fault::Conflict);
        }
        let owner = state
            .users
            .iter_mut()
            .find(|u| u.id == holder.owner && u.status == Status::Active && u.admin)
            .ok_or(Fault::Denied)?;
        let next = head.generation.checked_add(1).ok_or(Fault::Limit)?;
        let incarnation = super::identity(&head, next, request, 4);
        let signer: Hash = Sha256::digest(key.as_bytes()).into();
        let mut id = incarnation;
        let previous = match action {
            Action::Install => {
                if owner.packages.len() >= super::MAX_SUBJECTS {
                    return Err(Fault::Limit);
                }
                None
            }
            Action::Update(old) => {
                let index = owner
                    .packages
                    .iter()
                    .position(|p| p.id == old && p.active)
                    .ok_or(Fault::Denied)?;
                let current = &owner.packages[index];
                if current.package != verified.id()
                    || current.machine != target as u16
                    || current.signer != signer
                    || current.version >= verified.version()
                {
                    return Err(Fault::Denied);
                }
                id = old;
                Some(index)
            }
        };
        for (n, package) in owner.packages.iter().enumerate() {
            if !package.active || Some(n) == previous {
                continue;
            }
            if package.package == verified.id()
                || package
                    .entries
                    .iter()
                    .any(|e| verified.members.iter().any(|m| e.name == m.name))
            {
                return Err(Fault::Conflict);
            }
        }
        let mut entries = Vec::new();
        entries
            .try_reserve_exact(verified.members.len())
            .map_err(|_| Fault::Limit)?;
        let mut offset = 0;
        for m in &verified.members {
            entries.push(Entry {
                name: m.name.into(),
                digest: m.sha256,
                start: offset,
                size: m.image.len(),
            });
            offset += m.image.len() as u64;
        }
        // Nom inaccessible aux chemins du protocole public (caractère de contrôle).
        // Un échec de préparation ne remplace ni ne supprime l'objet publié.
        let leaf = format!("\u{1f}NX{:032x}", u128::from_le_bytes(incarnation));
        let object = self.fs.create_at(self.fs.root_id(), &leaf, Kind::File)?.id;
        let mut at = 0;
        for member in &verified.members {
            for chunk in member.image.chunks(64 * 1024) {
                self.fs.prepare_write_id(object, at, chunk)?.commit()?;
                at += chunk.len() as u64;
            }
        }
        let stat = self.fs.stat_id(object)?;
        if stat.size != at {
            return Err(Fault::Corrupt);
        }
        let release = Release {
            id,
            package: verified.id(),
            version: verified.version(),
            machine: target as u16,
            signer,
            manifest: verified.manifest_hash(),
            object,
            revision: stat.revision,
            entries,
            active: true,
        };
        if let Some(slot) = previous {
            owner.packages[slot] = release;
        } else {
            owner.packages.push(release);
        }
        self.publish_state(head, state, request, next)
    }

    pub fn withdraw_package(&mut self, holder: Grant, request: Id, id: Id) -> super::Result<Head> {
        let (head, mut state) = self.view()?;
        if holder.head != head {
            return Err(Fault::Conflict);
        }
        let owner = state
            .users
            .iter_mut()
            .find(|u| u.id == holder.owner && u.status == Status::Active && u.admin)
            .ok_or(Fault::Denied)?;
        let p = owner
            .packages
            .iter_mut()
            .find(|p| p.id == id && p.active)
            .ok_or(Fault::Denied)?;
        p.active = false;
        self.publish_state(
            head,
            state,
            request,
            head.generation.checked_add(1).ok_or(Fault::Limit)?,
        )
    }

    /// Le titulaire provient de la session attestée. Le lanceur reçoit la copie
    /// mesurée et la référence immuable de son objet, sans réouverture par nom.
    pub fn load_package(
        &self,
        holder: Grant,
        key: &VerifyingKey,
        machine: Machine,
        leaf: &str,
    ) -> super::Result<Loaded> {
        name(leaf.as_bytes()).map_err(map_verify)?;
        let (head, state) = self.view()?;
        if head != holder.head {
            return Err(Fault::Conflict);
        }
        let owner = state.user(holder.owner)?;
        if owner.status != Status::Active {
            return Err(Fault::Denied);
        }
        let signer: Hash = Sha256::digest(key.as_bytes()).into();
        let (p, entry) = owner
            .packages
            .iter()
            .filter(|p| p.active && p.signer == signer && p.machine == machine as u16)
            .find_map(|p| p.entries.iter().find(|e| e.name == leaf).map(|e| (p, e)))
            .ok_or(Fault::Denied)?;
        let stat = self.fs.stat_id(p.object)?;
        let end = entry
            .start
            .checked_add(entry.size as u64)
            .ok_or(Fault::Corrupt)?;
        if stat.revision != p.revision
            || stat.kind != Kind::File
            || end > stat.size
            || entry.size > MAX_IMAGE
        {
            return Err(Fault::StateMismatch);
        }
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(entry.size)
            .map_err(|_| Fault::Limit)?;
        while bytes.len() < entry.size {
            let take = (entry.size - bytes.len()).min(64 * 1024);
            let part = self
                .fs
                .inspect_id(p.object, entry.start + bytes.len() as u64, take)?;
            if part.len() != take {
                return Err(Fault::Corrupt);
            }
            bytes.extend_from_slice(&part);
        }
        if Hash::from(Sha256::digest(&bytes)) != entry.digest {
            return Err(Fault::StateMismatch);
        }
        Ok(Loaded {
            installation: p.id,
            manifest: p.manifest,
            object: p.object,
            revision: p.revision,
            offset: entry.start,
            digest: entry.digest,
            bytes,
        })
    }
}
