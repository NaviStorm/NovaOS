#![no_std]
extern crate alloc;
#[cfg(test)]
extern crate self as novaos_catalogue;
#[cfg(test)]
extern crate std;
use alloc::{string::String, vec::Vec};
use serde::Deserialize;
mod admission;
mod limits;
mod schema;
pub use admission::{Blob, Origin, Signer};
pub use schema::MAX_MANIFEST;
mod matching;
mod resolve;
mod selection;
pub use limits::Limits;
pub use matching::Bus;
pub use resolve::{Binding, resolve};
pub use selection::{Plan, Target};

pub type Version = [u32; 3];
pub type Result<T> = core::result::Result<T, Error>;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Format,
    Limit,
    Signature,
    Digest,
    Elf,
    Denied,
    Revoked,
    Floor,
    Dependency,
    Cycle,
    Conflict,
    NoMatch,
}

pub(crate) fn name(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 96
        && s.as_bytes()[0].is_ascii_alphanumeric()
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
}
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    SystemProfile,
    Controller,
    Service,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Dependency {
    pub id: String,
    pub kind: Kind,
    pub min: Version,
    pub before: Version,
    #[serde(deserialize_with = "schema::hexbytes")]
    pub sha256: [u8; 32],
    pub required: bool,
}
pub struct Resource {
    id: String,
    kind: Kind,
    version: Version,
    digest: [u8; 32],
    record_digest: [u8; 32],
    dependencies: Vec<Dependency>,
}
pub struct Policy {
    blocked: Vec<[u8; 32]>,
    origin: Origin,
    floor: u32,
    signers: Vec<Signer>,
    system_key: [u8; 32],
    revision: u64,
    resources: Vec<Resource>,
}
pub struct Catalog<'a> {
    policy: &'a Policy,
    entries: Vec<Entry>,
}
struct Entry {
    manifest: Manifest,
    digest: [u8; 32],
    priority: u16,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    schema: String,
    origin: Origin,
    publisher: String,
    signing_key_id: String,
    security_epoch: u32,
    #[serde(deserialize_with = "schema::hexbytes")]
    elf_package_id: [u8; 16],
    elf_package_revision: u32,
    provides: Vec<String>,
    id: String,
    version: Version,
    class: String,
    compat: Compat,
    image: Vec<Image>,
    #[serde(rename = "match")]
    matches: Vec<Bus>,
    required_caps: Vec<String>,
    optional_caps: Vec<String>,
    dependency: Vec<Dependency>,
    hardware: Hardware,
    quotas: Quotas,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Compat {
    architectures: Vec<String>,
    abi_major: u32,
    os_min: Version,
    os_before: Version,
    required_features: Vec<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Image {
    member: String,
    isa: String,
    #[serde(deserialize_with = "schema::hexbytes")]
    elf_sha256: [u8; 32],
    #[serde(deserialize_with = "schema::hexbytes")]
    package_sha256: [u8; 32],
}
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Hardware {
    pub dma: Vec<String>,
    pub mmio: Vec<String>,
    pub irq: Vec<String>,
    pub max_irq_routes: u16,
    pub dma_pages: u32,
    pub queues: u16,
    pub entries_per_queue: u16,
}
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Quotas {
    pub private_pages: u32,
    pub ipc_roots_created: u8,
    pub ipc_handles: u8,
    pub ipc_boxes_created: u8,
    pub pending_operations: u16,
}
