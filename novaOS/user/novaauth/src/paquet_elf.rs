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
