//! Registre privé P9. Seul le courtier de confiance possède Engine et Authority.
//! Les preuves de canal de l'appelant sont vérifiées avant la remise d'un Grant.
#![no_std]
extern crate alloc;
use alloc::{string::String, vec::Vec};
use novafs::{Device, Engine, Ids, Kind, View};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub mod lab;
#[cfg(feature = "recuperation-lab")]
pub mod recuperation;
#[cfg(feature = "recuperation-lab")]
pub mod recuperation_store;
#[cfg(feature = "recuperation-lab")]
pub mod recuperation_port;
#[cfg(feature = "recuperation-lab")]
pub mod recuperation_view;
#[cfg(feature = "recuperation-lab")]
pub mod snapshot_digest;
#[cfg(feature = "distribution-lab")]
pub mod distribution;
#[cfg(feature = "politiques-lab")]
pub mod policy;
pub type Id = [u8; 16];
pub type Hash = [u8; 32];
pub const PATHS: [&str; 3] = [
    "/system/identity/users.db",
    "/system/identity/groups.db",
    "/system/identity/credentials.db",
];
const SLOT: usize = 16384;
const MAX_SUBJECTS: usize = 64;

#[derive(Debug)]
pub enum Fault {
    Denied,
    Conflict,
    Corrupt,
    WrongDomain,
    StateMismatch,
    StaleVolume,
    NotEnrolled,
    Unavailable,
    Uncertain,
    Limit,
    BadInput,
    NotNew,
    Storage(novafs::Error),
}
pub type Result<T> = core::result::Result<T, Fault>;
impl From<novafs::Error> for Fault {
    fn from(e: novafs::Error) -> Self {
        Fault::Storage(e)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Head {
    pub domain: Id,
    pub volume: [Id; 2],
    pub recovery: Id,
    pub objects: [u64; 3],
    pub generation: u64,
    pub request: Id,
    pub digest: Hash,
}
/// Fournisseur authentifié et linéarisable. L'écrivain détient le bail
/// exclusif du volume de la préparation jusqu'à la décision/reconciliation.
pub trait Authority {
    fn current(&self) -> Result<Head>;
    fn outcome(&self, request: Id) -> Result<Option<Head>>;
    fn decide(&mut self, previous: Head, next: Head) -> Result<()>;
}
/// Réservée au provisionneur. Installée seulement après vérification du disque
/// et des trois objets. Une autorité existante doit refuser toute réinitialisation.
pub trait Provisioner: Authority {
    fn ensure_unused(&self) -> Result<()>;
    fn bind_new(&mut self, genesis: Head) -> Result<()>;
}
/// Le détenteur doit posséder le droit exclusif de formater cette cible et le
/// droit d'initialiser cette autorité. Aucun appel implicite lors d'un montage.
pub fn provision<D: Device>(
    disk: D,
    ids: Ids,
    domain: Id,
    recovery: Id,
    authority: &mut impl Provisioner,
) -> Result<Engine<D>> {
    authority.ensure_unused()?;
    if recovery == [0; 16] || domain == [0; 16] {
        return Err(Fault::BadInput);
    }
    for i in 0..disk.blocks() {
        if disk.read(i)?.iter().any(|v| *v != 0) {
            return Err(Fault::NotNew);
        }
    }
    let mut fs = Engine::format_p9(disk, ids, novafs::time::Atime::No, None)?;
    fs.mkdir("/system")?;
    fs.mkdir("/system/identity")?;
    let mut objects = [0; 3];
    for (i, path) in PATHS.iter().enumerate() {
        fs.create(path)?;
        objects[i] = fs.stat(path)?.id;
    }
    authority.bind_new(Head {
        domain,
        recovery,
        volume: [ids.fs, ids.lv],
        objects,
        generation: 0,
        request: [0; 16],
        digest: [0; 32],
    })?;
    Ok(fs)
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Active,
    Disabled,
    Deleted,
}
#[derive(Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(not(feature = "politiques-lab"), serde(deny_unknown_fields))]
pub struct User {
    pub id: Id,
    pub name: String,
    pub status: Status,
    pub revision: u64,
    pub admin: bool,
    #[cfg(feature = "politiques-lab")]
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    installations: Vec<policy::Installation>,
    #[cfg(feature = "distribution-lab")]
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    packages: Vec<distribution::Release>,
    #[cfg(feature = "politiques-lab")]
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    folders: Vec<(u64, Hash)>,
    #[cfg(feature = "politiques-lab")]
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    rules: Vec<policy::Rule>,
}
#[derive(Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(not(feature = "politiques-lab"), serde(deny_unknown_fields))]
struct Group {
    id: Id,
    name: String,
    members: Vec<Id>,
    #[cfg(feature = "politiques-lab")]
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    admissions: Vec<(Id, u64)>,
}
#[derive(Debug, Serialize, Deserialize, Clone)]
struct Credential {
    owner: Id,
    reference: Id,
}
/// Une référence de justificatif est un identifiant public de coffre, jamais
/// un secret, un vérificateur ni une capacité au porteur.
#[derive(Default)]
struct State {
    users: Vec<User>,
    groups: Vec<Group>,
    credentials: Vec<Credential>,
}
pub enum Change<'a> {
    Enroll {
        name: &'a str,
        credential: Id,
        group: &'a str,
    },
    AddGroup(&'a str),
    Create {
        name: &'a str,
        credential: Id,
        group: Id,
    },
    Rename(Id, &'a str),
    Disable(Id),
    Delete(Id),
    AddMember(Id, Id),
    RemoveMember(Id, Id),
}
/// Délégation opaque, produite dans le composant de confiance. Aucune
/// désérialisation depuis un client. Toute mutation invalide la vue précédente.
#[derive(Clone, Copy)]
pub struct Grant {
    head: Head,
    owner: Id,
}
impl Grant {
    #[cfg(feature = "distribution-lab")]
    pub fn generation_digest(&self) -> Hash { self.head.digest }

    pub fn subject(&self) -> Id {
        self.owner
    }
}

fn valid_name(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 32
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}
fn identity(h: &Head, gen_no: u64, request: Id, kind: u8) -> Id {
    let mut d = Sha256::new();
    d.update(b"NovaAuth/subject/1");
    d.update(h.domain);
    d.update(gen_no.to_le_bytes());
    d.update(request);
    d.update([kind]);
    d.finalize()[..16].try_into().unwrap()
}
impl State {
    fn user(&self, id: Id) -> Result<&User> {
        self.users.iter().find(|u| u.id == id).ok_or(Fault::Denied)
    }
    fn check_name(&self, name: &str) -> Result<()> {
        if !valid_name(name) {
            return Err(Fault::BadInput);
        }
        if self
            .users
            .iter()
            .any(|u| u.status != Status::Deleted && u.name == name)
        {
            return Err(Fault::Conflict);
        }
        Ok(())
    }
    fn apply(&mut self, h: &Head, c: Change, new: u64, request: Id) -> Result<()> {
        let uid = identity(h, new, request, 1);
        let gid = identity(h, new, request, 2);
        let mut add_user = None;
        match c {
            Change::AddGroup(name) | Change::Enroll { group: name, .. } => {
                if !valid_name(name) {
                    return Err(Fault::BadInput);
                }
                if self.groups.len() >= MAX_SUBJECTS {
                    return Err(Fault::Limit);
                }
                if self.groups.iter().any(|g| g.name == name || g.id == gid) {
                    return Err(Fault::Conflict);
                }
                self.groups.push(Group {
                    id: gid,
                    name: name.into(),
                    members: Vec::new(),
                    #[cfg(feature = "politiques-lab")]
                    admissions: Vec::new(),
                });
                if let Change::Enroll {
                    name, credential, ..
                } = c
                {
                    add_user = Some((name, credential, gid, true));
                }
            }
            Change::Create {
                name,
                credential,
                group,
            } => add_user = Some((name, credential, group, false)),
            Change::Rename(id, name) => {
                self.check_name(name)?;
                let u = self
                    .users
                    .iter_mut()
                    .find(|u| u.id == id && u.status != Status::Deleted)
                    .ok_or(Fault::Denied)?;
                u.name = name.into();
                u.revision = new;
            }
            Change::Disable(id) | Change::Delete(id) => {
                let u = self
                    .users
                    .iter_mut()
                    .find(|u| u.id == id && u.status != Status::Deleted)
                    .ok_or(Fault::Denied)?;
                if u.admin {
                    return Err(Fault::Denied);
                }
                u.revision = new;
                u.status = if matches!(c, Change::Disable(_)) {
                    Status::Disabled
                } else {
                    Status::Deleted
                };
                if u.status == Status::Deleted {
                    // Garder la référence non secrète dans la tombe empêche de
                    // rattacher le même facteur à un autre titulaire.
                    for g in &mut self.groups {
                        #[cfg(feature = "politiques-lab")]
                        g.admissions.retain(|(u, _)| *u != id);
                        g.members.retain(|v| *v != id);
                    }
                }
            }
            Change::AddMember(id, group) => {
                if self.user(id)?.status != Status::Active {
                    return Err(Fault::Denied);
                }
                let g = self
                    .groups
                    .iter_mut()
                    .find(|g| g.id == group)
                    .ok_or(Fault::BadInput)?;
                if g.members.contains(&id) {
                    return Err(Fault::Conflict);
                }
                g.members.push(id);
                #[cfg(feature = "politiques-lab")]
                g.admissions.push((id, new));
                self.users
                    .iter_mut()
                    .find(|u| u.id == id)
                    .ok_or(Fault::Corrupt)?
                    .revision = new;
            }
            Change::RemoveMember(id, group) => {
                let g = self
                    .groups
                    .iter_mut()
                    .find(|g| g.id == group)
                    .ok_or(Fault::BadInput)?;
                let i = g
                    .members
                    .iter()
                    .position(|v| *v == id)
                    .ok_or(Fault::BadInput)?;
                g.members.remove(i);
                #[cfg(feature = "politiques-lab")]
                g.admissions.retain(|(u, _)| *u != id);
                self.users
                    .iter_mut()
                    .find(|u| u.id == id)
                    .ok_or(Fault::Corrupt)?
                    .revision = new;
            }
        }
        if let Some((name, reference, group, admin)) = add_user {
            self.check_name(name)?;
            if self.users.len() >= MAX_SUBJECTS {
                return Err(Fault::Limit);
            }
            if reference == [0; 16]
                || self.credentials.iter().any(|c| c.reference == reference)
                || self.users.iter().any(|u| u.id == uid)
            {
                return Err(Fault::Conflict);
            }
            self.groups
                .iter_mut()
                .find(|g| g.id == group)
                .ok_or(Fault::BadInput)?
                .members
                .push(uid);
            self.users.push(User {
                id: uid,
                name: name.into(),
                status: Status::Active,
                revision: new,
                admin,
                #[cfg(feature = "politiques-lab")]
                installations: Vec::new(),
                #[cfg(feature = "distribution-lab")]
                packages: Vec::new(),
                #[cfg(feature = "politiques-lab")]
                folders: Vec::new(),
                #[cfg(feature = "politiques-lab")]
                rules: Vec::new(),
            });
            self.credentials.push(Credential {
                owner: uid,
                reference,
            });
        }
        Ok(())
    }
}
fn shape<D: Device>(fs: &Engine<D>, h: &Head, domain: Id) -> Result<()> {
    if fs.atime_policy().is_none() || h.domain != domain || h.volume != [fs.ids().fs, fs.ids().lv] {
        return Err(Fault::WrongDomain);
    }
    for (i, path) in PATHS.iter().enumerate() {
        let st = fs.stat(path)?;
        if st.id != h.objects[i] || st.kind != Kind::File {
            return Err(Fault::Corrupt);
        }
    }
    Ok(())
}
fn header(h: &Head, table: usize) -> Vec<u8> {
    let mut v = Vec::from(*b"NVAUTH01");
    v.push(table as u8);
    v.extend(h.domain);
    v.extend(h.generation.to_le_bytes());
    v.extend(h.request);
    v
}
fn digest(h: &Head, parts: &[Vec<u8>; 3]) -> Hash {
    let mut d = Sha256::new();
    d.update(b"NovaAuth/head/1");
    d.update(h.domain);
    d.update(h.volume[0]);
    d.update(h.volume[1]);
    for (id, part) in h.objects.iter().zip(parts) {
        d.update(id.to_le_bytes());
        d.update((part.len() as u64).to_le_bytes());
        d.update(part);
    }
    d.finalize().into()
}
fn slot(h: &Head) -> u64 {
    (h.generation % 2) * SLOT as u64
}
fn read_parts<D: Device>(fs: &Engine<D>, h: &Head) -> Result<[Vec<u8>; 3]> {
    if h.generation == 0 {
        return Err(Fault::NotEnrolled);
    }
    let mut parts = [Vec::new(), Vec::new(), Vec::new()];
    for (i, p) in parts.iter_mut().enumerate() {
        let prefix = header(h, i);
        let bytes = fs.inspect_id(h.objects[i], slot(h), SLOT)?;
        if bytes.len() < prefix.len() + 4 {
            return Err(Fault::StaleVolume);
        }
        if !bytes.starts_with(&prefix) {
            if bytes.starts_with(&prefix[..25])
                && u64::from_le_bytes(bytes[25..33].try_into().unwrap()) < h.generation
            {
                return Err(Fault::StaleVolume);
            }
            return Err(Fault::StateMismatch);
        }
        let n =
            u32::from_le_bytes(bytes[prefix.len()..prefix.len() + 4].try_into().unwrap()) as usize;
        *p = bytes
            .get(prefix.len() + 4..)
            .and_then(|s| s.get(..n))
            .ok_or(Fault::Corrupt)?
            .into();
    }
    // L'empreinte engage aussi les en-têtes via le domaine, la génération et la demande.
    if snapshot_digest(h, &parts) != h.digest {
        return Err(Fault::StateMismatch);
    }
    Ok(parts)
}
fn load<D: Device>(fs: &Engine<D>, h: &Head) -> Result<State> {
    let parts = read_parts(fs, h)?;
    Ok(State {
        users: serde_json::from_slice(&parts[0]).map_err(|_| Fault::Corrupt)?,
        groups: serde_json::from_slice(&parts[1]).map_err(|_| Fault::Corrupt)?,
        credentials: serde_json::from_slice(&parts[2]).map_err(|_| Fault::Corrupt)?,
    })
}
fn snapshot_digest(h: &Head, p: &[Vec<u8>; 3]) -> Hash {
    let mut d = Sha256::new();
    d.update(header(h, 3));
    d.update(digest(h, p));
    d.finalize().into()
}
/// Courtier privé à écrivain unique. Ne jamais exporter le volume brut aux clients.
pub struct Registry<'a, D: Device, A: Authority> {
    fs: &'a mut Engine<D>,
    authority: &'a mut A,
    domain: Id,
}
impl<'a, D: Device, A: Authority> Registry<'a, D, A> {
    pub fn open(fs: &'a mut Engine<D>, authority: &'a mut A, domain: Id) -> Result<Self> {
        let h = authority.current()?;
        shape(fs, &h, domain)?;
        if h.generation > 0 {
            load(fs, &h)?;
        }
        Ok(Self {
            fs,
            authority,
            domain,
        })
    }
    fn view(&self) -> Result<(Head, State)> {
        let h = self.authority.current()?;
        shape(self.fs, &h, self.domain)?;
        Ok((h, load(self.fs, &h)?))
    }
    /// Point privé pour un authentificateur ayant déjà attesté le principal.
    pub fn resolve_verified_factor(&self, reference: Id) -> Result<Grant> {
        let (head, state) = self.view()?;
        let owner = state
            .credentials
            .iter()
            .find(|entry| entry.reference == reference)
            .ok_or(Fault::Denied)?
            .owner;
        if state.user(owner)?.status != Status::Active {
            return Err(Fault::Denied);
        }
        Ok(Grant { head, owner })
    }
    /// Usage privé après attestation par une autre autorité de confiance.
    pub fn grant_attested(&self, owner: Id) -> Result<Grant> {
        let (head, state) = self.view()?;
        if state.user(owner)?.status != Status::Active {
            return Err(Fault::Denied);
        }
        Ok(Grant { head, owner })
    }
    pub fn inspect(&self, g: Grant, subject: Id) -> Result<User> {
        let (h, s) = self.view()?;
        if g.head != h || (g.owner != subject && !s.user(g.owner)?.admin) {
            return Err(Fault::Denied);
        }
        Ok(s.user(subject)?.clone())
    }
    pub fn groups(&self, g: Grant) -> Result<Vec<Id>> {
        let (h, s) = self.view()?;
        if g.head != h {
            return Err(Fault::Denied);
        }
        Ok(s.groups
            .iter()
            .filter(|v| v.members.contains(&g.owner))
            .map(|v| v.id)
            .collect())
    }
    pub fn outcome(&self, request: Id) -> Result<Option<Head>> {
        let h = self.authority.current()?;
        shape(self.fs, &h, self.domain)?;
        self.authority.outcome(request)
    }
    pub fn mutate(&mut self, grant: Grant, request: Id, change: Change) -> Result<Head> {
        let (h, s) = self.view()?;
        if h != grant.head {
            return Err(Fault::Conflict);
        }
        if !s.user(grant.owner)?.admin || matches!(change, Change::Enroll { .. }) {
            return Err(Fault::Denied);
        }
        self.publish(h, s, request, change)
    }
    /// Réservé au port initial du provisionneur, jamais distribué aux utilisateurs.
    pub fn enroll(
        &mut self,
        proof: &Enrollment,
        request: Id,
        name: &str,
        credential: Id,
        group: &str,
    ) -> Result<Head> {
        let h = self.authority.current()?;
        shape(self.fs, &h, self.domain)?;
        if h.generation != 0 || proof.0 != h {
            return Err(Fault::Denied);
        }
        self.publish(
            h,
            State::default(),
            request,
            Change::Enroll {
                name,
                credential,
                group,
            },
        )
    }
    fn publish(&mut self, h: Head, mut s: State, request: Id, change: Change) -> Result<Head> {
        let generation = h.generation.checked_add(1).ok_or(Fault::Limit)?;
        s.apply(&h, change, generation, request)?;
        self.publish_state(h, s, request, generation)
    }
    fn publish_state(&mut self, h: Head, s: State, request: Id, generation: u64) -> Result<Head> {
        if request == [0; 16] || self.authority.outcome(request)?.is_some() {
            return Err(Fault::Conflict);
        }
        let parts = [
            serde_json::to_vec(&s.users),
            serde_json::to_vec(&s.groups),
            serde_json::to_vec(&s.credentials),
        ];
        let [a, b, c] = parts;
        let enc = |_| Fault::Limit;
        let parts = [a.map_err(enc)?, b.map_err(enc)?, c.map_err(enc)?];
        let mut next = Head {
            generation,
            request,
            ..h
        };
        next.digest = snapshot_digest(&next, &parts);
        for (i, p) in parts.iter().enumerate() {
            let mut bytes = header(&next, i);
            bytes.extend((p.len() as u32).to_le_bytes());
            if bytes.len() + p.len() > SLOT {
                return Err(Fault::Limit);
            }
            bytes.extend(p);
            self.fs
                .prepare_write_id(h.objects[i], slot(&next), &bytes)?
                .commit()?;
        }
        load(self.fs, &next)?;
        self.authority.decide(h, next)?;
        Ok(next)
    }
}
pub struct Enrollment(Head);
impl Enrollment {
    /// Le provisionneur conserve cette capacité Rust dans son canal privilégié.
    pub fn from_provisioner(a: &mut impl Provisioner) -> Result<Self> {
        let head = a.current()?;
        if head.generation != 0 {
            return Err(Fault::Denied);
        }
        Ok(Self(head))
    }
}
pub fn created_user(h: &Head) -> Id {
    identity(h, h.generation, h.request, 1)
}
pub fn created_group(h: &Head) -> Id {
    identity(h, h.generation, h.request, 2)
}
