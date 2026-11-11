//! Migration B. Le support source sert exclusivement aux lectures authentifiées.
//! Le candidat n'acquiert ses deux en-têtes de montage qu'après relecture.
use super::*;
use crate::shared_graph::{
    Catalog, Entry, Graph,
    migration::{Remap, install},
};
use crate::sync::Arc;
use alloc::collections::BTreeSet;

pub struct Enrollment<'a> {
    pub old_domain: Id,
    pub phrase: &'a [u8],
    pub recovery: &'a Secret,
}

/// Le reçu encodé dans les en-têtes accompagne aussi une destination copiée.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MigrationReceipt(Vec<u8>);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MigrationView {
    pub id: u64,
    pub lineage: u8,
    pub kind: crate::shared::ObjectKind,
    pub parent: ViewParent,
}

const RECEIPT: &[u8; 8] = b"NP9MIG01";
const DOMAIN_BYTES: usize = 80;
const INCOMPLETE: &[u8; 8] = b"NP9PREP1";
pub fn incomplete(device: &impl Device) -> Result<bool> {
    if device.blocks() < 2 {
        return Ok(false);
    }
    Ok(&device.read(0)?[..8] == INCOMPLETE || &device.read(1)?[..8] == INCOMPLETE)
}
const FIXED: usize = 195;
const VERIFIED: u8 = 1;
const OLD_LABEL: &[u8] = b"NovaFS/P9/recu/source/v1";
const NEW_LABEL: &[u8] = b"NovaFS/P9/recu/destination/v1";

impl MigrationReceipt {
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
    pub fn source(&self) -> Id {
        self.0[8..24].try_into().unwrap()
    }
    pub fn source_root(&self) -> Digest {
        self.0[32..64].try_into().unwrap()
    }
    pub fn destination_root(&self) -> Digest {
        self.0[120..152].try_into().unwrap()
    }
    pub fn graph_digest(&self) -> Digest {
        self.0[160..192].try_into().unwrap()
    }
    pub fn source_generation(&self) -> u64 {
        u64::from_le_bytes(self.0[24..32].try_into().unwrap())
    }
    pub fn units(&self) -> u64 {
        u64::from_le_bytes(self.0[152..160].try_into().unwrap())
    }
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < FIXED + 64
            || &bytes[..8] != RECEIPT
            || !matches!(bytes[192], 1..=3)
            || bytes[193] as usize > LIMIT
            || bytes[194] > 32
            || bytes.len()
                != FIXED
                    + usize::from(bytes[193]) * DOMAIN_BYTES
                    + usize::from(bytes[194]) * 11
                    + 64
        {
            return Err(corrupt());
        }
        let receipt = Self(bytes.to_vec());
        if receipt.destination() == [0; 16]
            || receipt.0[8..24] == [0; 16]
            || receipt.0[8..24] == receipt.destination()
            || receipt.source_generation() == 0
            || !(1..=2).contains(&receipt.destination_generation())
            || receipt.0[32..64] == [0; 32]
            || receipt.0[120..152] == [0; 32]
        {
            return Err(corrupt());
        }
        let mut ids = BTreeSet::new();
        let mut old = BTreeSet::new();
        let mut fresh = BTreeSet::new();
        for (ids_pair, a, b) in receipt.lineages() {
            if ids_pair.fs == [0; 16]
                || ids_pair.lv == [0; 16]
                || !ids.insert((ids_pair.fs, ids_pair.lv))
                || b == [0; 16]
                || !fresh.insert(b)
                || (bytes[192] == VERIFIED && (a == [0; 16] || !old.insert(a)))
                || (bytes[192] != VERIFIED && a != [0; 16])
            {
                return Err(corrupt());
            }
        }
        let accounted = receipt.seals().try_fold(0u64, |sum, (_, data, metadata)| {
            sum.checked_add(data)?.checked_add(metadata)
        });
        if receipt.units() > crate::MAX_BLOCKS
            || accounted.is_none_or(|total| total > receipt.units())
            || !old.is_disjoint(&fresh)
        {
            return Err(corrupt());
        }
        let mut previous = 0;
        for v in
            bytes[FIXED + usize::from(bytes[193]) * DOMAIN_BYTES..bytes.len() - 64].chunks_exact(11)
        {
            let id = u64::from_le_bytes(v[..8].try_into().unwrap());
            if id <= previous || v[8] >= bytes[193] || !matches!((v[9], v[10]), (1, 0) | (2 | 3, 1))
            {
                return Err(corrupt());
            }
            previous = id;
        }
        Ok(receipt)
    }
    pub fn verify_clear_source(
        &self,
        disk: &impl Device,
        expected: Digest,
        witness: &Secret,
    ) -> Result<()> {
        let n = self.0.len() - 64;
        if !matches!(self.0[192], 2 | 3)
            || self.0[64..96] != expected
            || image_digest(disk)? != expected
        {
            return Err(corrupt());
        }
        witness.verify_mac(
            &[OLD_LABEL, &self.0[..n]],
            self.0[n..n + 32].try_into().unwrap(),
        )
    }
    pub fn destination(&self) -> Id {
        self.0[96..112].try_into().unwrap()
    }
    pub fn destination_generation(&self) -> u64 {
        u64::from_le_bytes(self.0[112..120].try_into().unwrap())
    }
    /// Identités durables FS/LV, ancien et nouveau domaine. L'ancien domaine
    /// vaut zéro pour un catalogue P5/P6 dépourvu de domaine cryptographique.
    pub fn lineages(&self) -> impl Iterator<Item = (Ids, Id, Id)> + '_ {
        self.0[FIXED..]
            .chunks_exact(DOMAIN_BYTES)
            .take(usize::from(self.0[193]))
            .map(|s| {
                (
                    Ids {
                        fs: s[..16].try_into().unwrap(),
                        lv: s[16..32].try_into().unwrap(),
                    },
                    s[32..48].try_into().unwrap(),
                    s[48..64].try_into().unwrap(),
                )
            })
    }
    /// Scellements d'unités sous chacune des deux clés neuves (données, métadonnées).
    pub fn seals(&self) -> impl Iterator<Item = (Id, u64, u64)> + '_ {
        self.0[FIXED..]
            .chunks_exact(DOMAIN_BYTES)
            .take(usize::from(self.0[193]))
            .map(|r| {
                (
                    r[48..64].try_into().unwrap(),
                    u64::from_le_bytes(r[64..72].try_into().unwrap()),
                    u64::from_le_bytes(r[72..80].try_into().unwrap()),
                )
            })
    }
    pub fn views(&self) -> impl Iterator<Item = MigrationView> + '_ {
        use crate::shared::ObjectKind;
        let start = FIXED + usize::from(self.0[193]) * DOMAIN_BYTES;
        self.0[start..]
            .chunks_exact(11)
            .take(usize::from(self.0[194]))
            .map(|b| MigrationView {
                id: u64::from_le_bytes(b[..8].try_into().unwrap()),
                lineage: b[8],
                kind: match b[9] {
                    1 => ObjectKind::Origin,
                    2 => ObjectKind::Snapshot,
                    _ => ObjectKind::Clone,
                },
                parent: if b[10] == 0 {
                    ViewParent::Origin
                } else {
                    ViewParent::HistoricalUnknown
                },
            })
    }
    fn verify_destination(&self, id: Id, key: &Secret) -> Result<()> {
        let n = self.0.len() - 32;
        if self.0[96..112] != id {
            return Err(corrupt());
        }
        key.verify_mac(&[NEW_LABEL, &self.0[..n]], self.0[n..].try_into().unwrap())
    }
}

pub(super) fn receipt_field(h: &Block) -> Result<Option<&[u8]>> {
    let n = usize::from(u16::from_le_bytes(h[112..114].try_into().unwrap()));
    let b = h.get(114..114 + n).ok_or_else(corrupt)?;
    if h[114 + n..].iter().any(|&v| v != 0) {
        return Err(corrupt());
    }
    if n == 0 {
        Ok(None)
    } else {
        MigrationReceipt::from_bytes(b)?;
        Ok(Some(b))
    }
}
pub(super) fn verify_destination_header(h: &Block, key: &Secret) -> Result<()> {
    if let Some(b) = receipt_field(h)? {
        MigrationReceipt::from_bytes(b)?.verify_destination(h[16..32].try_into().unwrap(), key)?;
    }
    Ok(())
}

pub(super) fn check_checkpoint(h: &Block, head: Head) -> Result<()> {
    if &h[..8] == P9_MAGIC
        && let Some(bytes) = receipt_field(h)?
    {
        let r = MigrationReceipt::from_bytes(bytes)?;
        let g = r.destination_generation();
        let root = &bytes[120..152];
        if head.generation < g
            || (head.generation == g && head.digest != root)
            || (head.generation == g + 1 && head.parent != root)
        {
            return Err(Error::Corrupt("point de migration P9 inaccessible"));
        }
    }
    Ok(())
}

/// Le support d'arrivée est verrouillé par l'appelant. Les deux blocs montables
/// restent privés, y compris durant une vérification avec le lecteur ordinaire.
struct Staging<D> {
    disk: D,
    headers: [Block; 2],
}
impl<D: Device> Device for Staging<D> {
    fn blocks(&self) -> u64 {
        self.disk.blocks()
    }
    fn read(&self, b: u64) -> Result<Block> {
        if b < 2 {
            Ok(self.headers[b as usize])
        } else {
            self.disk.read(b)
        }
    }
    fn write(&mut self, b: u64, p: &Block) -> Result<()> {
        if b < 2 {
            self.headers[b as usize] = *p;
            Ok(())
        } else {
            self.disk.write(b, p)
        }
    }
    fn flush(&mut self) -> Result<()> {
        self.disk.flush()
    }
}

struct UniqueNonces<'a, E> {
    rng: &'a mut E,
    used: BTreeSet<[u8; 24]>,
}
impl<E: Entropy> Entropy for UniqueNonces<'_, E> {
    fn fill(&mut self, out: &mut [u8]) -> Result<()> {
        if out.len() != 24 {
            return Err(Error::Invalid("tirage de nonce de migration"));
        }
        self.rng.fill(out)?;
        if !self.used.insert(out[..].try_into().unwrap()) {
            return Err(Error::Unavailable);
        }
        Ok(())
    }
}

impl<D: Device, E: Entropy> SecurePool<D, E> {
    pub fn migration_receipt(&mut self) -> Result<Option<MigrationReceipt>> {
        self.with_reader(|p| {
            p.check_physical(false)?;
            let h = read_header(&p.disk)?;
            if !p.state.p9 {
                return Ok(None);
            }
            verify_destination_header(&h, &p.admin)?;
            receipt_field(&h)?
                .map(MigrationReceipt::from_bytes)
                .transpose()
        })
    }

    /// Vérifie l'autorité source et la génération effectivement relue. Une
    /// source ayant avancé exige sa copie historique correspondant au reçu.
    pub fn verify_migration_source(&mut self, r: &MigrationReceipt) -> Result<()> {
        self.with_reader(|p| {
            p.available()?;
            p.check_physical(true)?;
            let b = r.as_bytes();
            let n = b.len() - 64;
            let h = p.log.head();
            if b[192] != VERIFIED
                || b[8..24] != p.pool
                || r.source_generation() != h.generation
                || b[32..64] != h.digest
                || b[64..96] != h.parent
            {
                return Err(corrupt());
            }
            p.admin
                .verify_mac(&[OLD_LABEL, &b[..n]], b[n..n + 32].try_into().unwrap())
        })
    }

    /// Relit les deux générations de chaque image au point de migration.
    /// Recalcule le reçu à partir des arêtes et compare les clairs sous les deux clés.
    /// Les originaux conservés servent de témoins si les pools ont depuis avancé.
    pub fn verify_migration_pair<T: Device, F: Entropy>(
        &mut self,
        destination: &mut SecurePool<T, F>,
    ) -> Result<()> {
        let receipt = destination.migration_receipt()?.ok_or(Error::NotFound)?;
        self.verify_migration_source(&receipt)?;
        self.with_reader(|source| {
            destination.with_reader(|target| {
                let input = source.describe()?;
                target.available()?;
                target.check_physical(true)?;
                super::inspect::verify_redundancy(&target.disk, target.pool, &target.admin)?;
                let h = target.log.head();
                if h.generation != receipt.destination_generation()
                    || h.digest != receipt.0[120..152]
                    || input.head.generation != receipt.source_generation()
                    || input.head.digest != receipt.0[32..64]
                {
                    return Err(Error::Invalid("images au point de migration requises"));
                }
                let cats = Graph::visible_catalogs(
                    &Prefix {
                        state: &target.state,
                        size: coord(target.disk.blocks())?,
                    },
                    target.pool,
                )?;
                if cats.len() != input.catalogs.len() {
                    return Err(corrupt());
                }
                let mut remap = Remap::default();
                let mut canonical = Sha256::new();
                canonical.update(b"NovaFS/P9/vues et racines/v1");
                for (a, b) in input.catalogs.iter().zip(&cats).rev() {
                    if a.next_id != b.next_id || a.entries.len() != b.entries.len() {
                        return Err(corrupt());
                    }
                    for (old, new) in a.entries.iter().zip(&b.entries) {
                        let domain = source.state.domain(old.ids)?.id;
                        let next = target.state.domain(new.ids)?.id;
                        if !receipt.lineages().any(|row| row == (old.ids, domain, next)) {
                            return Err(corrupt());
                        }
                        let from = source.media(Some(domain))?;
                        let to = target.media(Some(next))?;
                        let converted = remap.view(&from, old, true, |block, role, wanted| {
                            let pos = to.physical(block)?;
                            let hash =
                                unit::fingerprint(&to.disk.read(pos)?, &to.disk.read(pos + 1)?);
                            let actual =
                                zeroize::Zeroizing::new(to.read_object(block, role, &hash)?);
                            if &*actual != wanted {
                                return Err(corrupt());
                            }
                            Ok(hash)
                        })?;
                        if &converted != new {
                            return Err(corrupt());
                        }
                        record_view(&mut canonical, a.generation, b.generation, old, new);
                    }
                }
                let (units, edges) = remap.identity();
                canonical.update(edges);
                if units != receipt.units() || canonical.finalize()[..] != receipt.0[160..192] {
                    return Err(corrupt());
                }
                Ok(())
            })
        })
    }

    pub fn migrate_p9<T: Device, F: Entropy>(
        &mut self,
        destination: T,
        administration: &Secret,
        enroll: &[Enrollment<'_>],
        mut entropy: F,
    ) -> Result<(T, MigrationReceipt)> {
        if self.state.p9 {
            return Err(Error::Invalid("source historique requise"));
        }
        let source = self.disk.mutation_lock();
        let _guard = source
            .as_ref()
            .map(|g| g.try_lock().map_err(|_| Error::Busy))
            .transpose()?;
        build(self, destination, administration, enroll, &mut entropy)
    }
}

struct InputDomain {
    ids: Ids,
    id: Id,
    encrypted: bool,
}
struct Inventory {
    pool: Id,
    head: Head,
    blocks: u64,
    domains: Vec<InputDomain>,
    catalogs: Vec<Catalog>,
    profile: u8,
}
trait MigrationSource {
    fn gate(&self) -> Option<Arc<crate::sync::Mutex<()>>>;
    fn describe(&mut self) -> Result<Inventory>;
    fn copy<T: Device>(
        &mut self,
        e: &Entry,
        domain: Id,
        map: &mut Remap,
        to: &mut T,
    ) -> Result<Entry>;
    fn confirm(&mut self, input: &Inventory) -> Result<()>;
    fn attest(&self, payload: &[u8]) -> Digest;
}
fn write_verified<T: Device>(to: &mut T, b: u64, role: ObjectRole, page: &Block) -> Result<Digest> {
    let hash = to.write_object(b, page, role)?;
    if &*zeroize::Zeroizing::new(to.read_object(b, role, &hash)?) != page {
        return Err(corrupt());
    }
    Ok(hash)
}
impl<D: Device, E: Entropy> MigrationSource for SecurePool<D, E> {
    fn gate(&self) -> Option<Arc<crate::sync::Mutex<()>>> {
        self.disk.mutation_lock()
    }
    fn describe(&mut self) -> Result<Inventory> {
        self.available()?;
        self.check_physical(true)?;
        super::inspect::verify_redundancy(&self.disk, self.pool, &self.admin)?;
        let blocks = coord(self.disk.blocks())?;
        Ok(Inventory {
            pool: self.pool,
            head: self.log.head(),
            blocks,
            profile: VERIFIED,
            domains: self
                .state
                .domains
                .iter()
                .map(|d| InputDomain {
                    ids: d.ids,
                    id: d.id,
                    encrypted: d.slots.is_some(),
                })
                .collect(),
            catalogs: Graph::visible_catalogs(
                &Prefix {
                    state: &self.state,
                    size: blocks,
                },
                self.pool,
            )?,
        })
    }
    fn copy<T: Device>(
        &mut self,
        e: &Entry,
        domain: Id,
        map: &mut Remap,
        to: &mut T,
    ) -> Result<Entry> {
        let from = self.media(Some(domain))?;
        map.view(&from, e, true, |b, role, page| {
            let actual = if let ObjectRole::Data(i) = role {
                let ctx = unit::context(&from.disk.read(from.physical(b)?)?)?;
                if ctx.kind == Kind::Metadata {
                    ObjectRole::FsMetadata(i)
                } else {
                    role
                }
            } else {
                role
            };
            write_verified(to, b, actual, page)
        })
    }
    fn confirm(&mut self, input: &Inventory) -> Result<()> {
        self.check_physical(true)?;
        super::inspect::verify_redundancy(&self.disk, self.pool, &self.admin)?;
        if self.log.head() != input.head {
            return Err(Error::Busy);
        }
        Ok(())
    }
    fn attest(&self, payload: &[u8]) -> Digest {
        self.admin.mac(&[OLD_LABEL, payload])
    }
}

struct ClearInput<'a, D> {
    reader: &'a SharedPool<D>,
    expected: Digest,
    witness: &'a Secret,
}
/// Empreinte externe d'une image historique sans attestation de domaine.
pub fn image_digest(d: &impl Device) -> Result<Digest> {
    let mut hasher = Sha256::new();
    for i in 0..d.blocks() {
        hasher.update(d.read(i)?);
    }
    Ok(hasher.finalize().into())
}
impl<D: Device> MigrationSource for ClearInput<'_, D> {
    fn gate(&self) -> Option<Arc<crate::sync::Mutex<()>>> {
        self.reader.source_parts().0.mutation_lock()
    }
    fn describe(&mut self) -> Result<Inventory> {
        self.reader.verify_source()?;
        let (disk, gr) = self.reader.source_parts();
        if image_digest(disk)? != self.expected {
            return Err(corrupt());
        }
        let catalogs = gr.protected_catalogs().cloned().collect::<Vec<_>>();
        let mut domains = Vec::<InputDomain>::new();
        for e in catalogs.iter().flat_map(|c| &c.entries) {
            if !domains.iter().any(|d| d.ids == e.ids) {
                domains.push(InputDomain {
                    ids: e.ids,
                    id: [0; 16],
                    encrypted: false,
                });
            }
        }
        let cat = gr.catalog();
        let block = disk.read(3 + cat.generation % 4)?;
        Ok(Inventory {
            pool: cat.pool,
            head: Head {
                generation: cat.generation,
                digest: block[80..112].try_into().unwrap(),
                parent: self.expected,
            },
            profile: 2,
            blocks: disk.blocks(),
            domains,
            catalogs,
        })
    }
    fn copy<T: Device>(&mut self, e: &Entry, _: Id, map: &mut Remap, to: &mut T) -> Result<Entry> {
        let (disk, gr) = self.reader.source_parts();
        let files = self.reader.classify_view(e)?;
        map.view(disk, e, gr.is_p6(), |b, role, page| {
            let actual = match role {
                ObjectRole::Data(i) if files[usize::from(i) / 64] & (1u64 << (i % 64)) == 0 => {
                    ObjectRole::FsMetadata(i)
                }
                _ => role,
            };
            write_verified(to, b, actual, page)
        })
    }
    fn confirm(&mut self, _: &Inventory) -> Result<()> {
        if image_digest(self.reader.source_parts().0)? != self.expected {
            return Err(Error::Busy);
        }
        Ok(())
    }
    fn attest(&self, payload: &[u8]) -> Digest {
        self.witness.mac(&[OLD_LABEL, payload])
    }
}

impl<D: Device> SharedPool<D> {
    /// Sur P5/P6 clair, l'appelant fournit une empreinte obtenue par un canal
    /// de confiance et la clé du témoin. Le reçu distingue ce cas d'une preuve P7.
    pub fn migrate_p9<T: Device, E: Entropy>(
        &self,
        destination: T,
        expected: Digest,
        witness: &Secret,
        administration: &Secret,
        mut entropy: E,
    ) -> Result<(T, MigrationReceipt)> {
        let gate = self.source_parts().0.mutation_lock();
        let _guard = gate
            .as_ref()
            .map(|g| g.try_lock().map_err(|_| Error::Busy))
            .transpose()?;
        let mut source = ClearInput {
            reader: self,
            expected,
            witness,
        };
        build(&mut source, destination, administration, &[], &mut entropy)
    }
}

struct LegacyInput<'a, S, D> {
    image: &'a S,
    public: ClearInput<'a, D>,
    digest: Digest,
}
impl<S: Device, D: Device> MigrationSource for LegacyInput<'_, S, D> {
    fn gate(&self) -> Option<Arc<crate::sync::Mutex<()>>> {
        self.image.mutation_lock()
    }
    fn describe(&mut self) -> Result<Inventory> {
        let mut i = self.public.describe()?;
        let (generation, digest) = crate::pool::checkpoint(self.image)?;
        i.profile = 3;
        i.pool = self.digest[..16].try_into().unwrap();
        i.head = Head {
            generation,
            digest,
            parent: self.digest,
        };
        Ok(i)
    }
    fn copy<T: Device>(
        &mut self,
        e: &Entry,
        domain: Id,
        map: &mut Remap,
        to: &mut T,
    ) -> Result<Entry> {
        self.public.copy(e, domain, map, to)
    }
    fn confirm(&mut self, input: &Inventory) -> Result<()> {
        self.public.confirm(input)?;
        if image_digest(self.image)? != self.digest {
            return Err(Error::Busy);
        }
        Ok(())
    }
    fn attest(&self, payload: &[u8]) -> Digest {
        self.public.attest(payload)
    }
}
/// L'ancien catalogue P0 ne possède pas d'UUID de pool. Le reçu identifie son
/// image par son SHA-256 complet. Les LV Raw/Preparing suivent le refus P5.
pub fn migrate_ready_pool<S: Device, D: Device, T: Device, E: Entropy>(
    source: &S,
    scratch: D,
    destination: T,
    expected: Digest,
    witness: &Secret,
    admin: &Secret,
    mut entropy: E,
) -> Result<(T, MigrationReceipt)> {
    let source_gate = source.mutation_lock();
    let work_gate = scratch.mutation_lock().ok_or(Error::ReadOnly)?;
    let dest_gate = destination.mutation_lock().ok_or(Error::ReadOnly)?;
    if Arc::ptr_eq(&work_gate, &dest_gate)
        || source_gate
            .as_ref()
            .is_some_and(|g| Arc::ptr_eq(g, &work_gate) || Arc::ptr_eq(g, &dest_gate))
    {
        return Err(Error::Exists);
    }
    let _read = source_gate
        .as_ref()
        .map(|g| g.try_lock().map_err(|_| Error::Busy))
        .transpose()?;
    let _work = work_gate.try_lock().map_err(|_| Error::Busy)?;
    if image_digest(source)? != expected {
        return Err(corrupt());
    }
    for lba in 0..scratch.blocks() {
        if scratch.read(lba)? != [0; 4096] {
            return Err(Error::Exists);
        }
    }
    let mut stage = SharedPool::format_p6(scratch, expected[..16].try_into().unwrap())?;
    stage.migrate_legacy(source)?;
    let intermediate = image_digest(stage.source_parts().0)?;
    let mut legacy = LegacyInput {
        image: source,
        public: ClearInput {
            reader: &stage,
            witness,
            expected: intermediate,
        },
        digest: expected,
    };
    build(&mut legacy, destination, admin, &[], &mut entropy)
}

fn record_view(hash: &mut Sha256, source: u64, destination: u64, old: &Entry, new: &Entry) {
    hash.update(source.to_le_bytes());
    hash.update(destination.to_le_bytes());
    hash.update(old.id.to_le_bytes());
    hash.update(old.owner);
    hash.update([old.kind as u8, old.policy as u8]);
    hash.update(old.blocks.to_le_bytes());
    hash.update(old.ids.fs);
    hash.update(old.ids.lv);
    hash.update(old.root.hash);
    hash.update(new.root.hash);
}

type DomainKeys = BTreeMap<Id, Keys>;
fn provision_lineages(
    input: &Inventory,
    enroll: &[Enrollment<'_>],
    entropy: &mut impl Entropy,
) -> Result<(Vec<Domain>, DomainKeys)> {
    let mut keys = BTreeMap::new();
    let mut domains = Vec::<Domain>::new();
    for d in &input.domains {
        let mut id = [0; 16];
        entropy.fill(&mut id)?;
        if id == [0; 16]
            || input.domains.iter().any(|v| v.id == id)
            || domains.iter().any(|v| v.id == id)
        {
            return Err(Error::Unavailable);
        }
        let slots = if d.encrypted {
            let e = enroll
                .iter()
                .find(|e| e.old_domain == d.id)
                .ok_or(Error::ReadOnly)?;
            let master = Secret::generate(entropy)?;
            let wrapped = Slots::create(&master, &id, e.phrase, e.recovery, entropy)?;
            keys.insert(id, Keys::derive(&master, &id)?);
            Some(wrapped)
        } else {
            None
        };
        domains.push(Domain {
            id,
            ids: d.ids,
            slots,
        });
    }
    Ok((domains, keys))
}

fn build<S: MigrationSource, T: Device, F: Entropy>(
    src: &mut S,
    destination: T,
    administration: &Secret,
    enroll: &[Enrollment<'_>],
    entropy: &mut F,
) -> Result<(T, MigrationReceipt)> {
    let gate = destination.mutation_lock().ok_or(Error::ReadOnly)?;
    if src.gate().is_some_and(|s| Arc::ptr_eq(&s, &gate)) {
        return Err(Error::Exists);
    }
    let _exclusive = gate.try_lock().map_err(|_| Error::Busy)?;
    let input = src.describe()?;
    let size = coord(destination.blocks())?;
    for b in 0..destination.blocks() {
        if destination.read(b)? != [0; 4096] {
            return Err(Error::Exists);
        }
    }
    if size < input.blocks {
        return Err(Error::NoSpace);
    }
    let encrypted: Vec<_> = input
        .domains
        .iter()
        .filter(|d| d.encrypted)
        .map(|d| d.id)
        .collect();
    if enroll.len() != encrypted.len()
        || encrypted
            .iter()
            .any(|id| enroll.iter().filter(|e| e.old_domain == *id).count() != 1)
    {
        return Err(Error::Invalid("enrôlements des lignées"));
    }
    let mut pool = [0; 16];
    entropy.fill(&mut pool)?;
    if pool == [0; 16] || pool == input.pool {
        return Err(Error::Unavailable);
    }
    let (h, admin) = new_header(
        pool,
        destination.blocks(),
        administration,
        entropy,
        true,
        true,
    )?;
    let mut dst = Staging {
        disk: destination,
        headers: [h, h],
    };
    let mut preparing = [0; 4096];
    preparing[..8].copy_from_slice(INCOMPLETE);
    preparing[8..24].copy_from_slice(&input.pool);
    preparing[24..40].copy_from_slice(&pool);
    for i in 0..2 {
        dst.disk.write(i, &preparing)?;
    }
    dst.disk.flush()?;
    let (domains, keys) = provision_lineages(&input, enroll, entropy)?;
    let previous_head = input.head;
    let src_catalogs = &input.catalogs;
    let mut remap = Remap::default();
    let mut canonical = Sha256::new();
    canonical.update(b"NovaFS/P9/vues et racines/v1");
    let mut state = State::empty();
    state.p9 = true;
    let mut inner_parent = [0; 32];
    let mut log: Option<CommitLog> = None;
    let mut last_ids = BTreeSet::new();
    let mut nonce = UniqueNonces {
        rng: entropy,
        used: BTreeSet::new(),
    };
    let mut views = BTreeMap::new();
    for (index, original) in src_catalogs.iter().rev().enumerate() {
        let g = 1 + index as u64;
        let mut next = Catalog {
            pool,
            p6: true,
            generation: g,
            next_id: original.next_id,
            entries: Vec::new(),
        };
        let has = |d: &Domain| {
            original.entries.iter().any(|e| e.ids == d.ids) || last_ids.contains(&d.id)
        };
        state.domains = domains.iter().filter(|d| has(d)).cloned().collect();
        state.imports.clear();
        for d in &state.domains {
            if d.slots.is_some() {
                let old = input
                    .domains
                    .iter()
                    .find(|o| o.ids == d.ids)
                    .ok_or_else(corrupt)?;
                state.imports.insert(d.id, (input.pool, old.id));
            }
        }
        for e in &original.entries {
            let old = input
                .domains
                .iter()
                .find(|d| d.ids == e.ids)
                .ok_or_else(corrupt)?
                .id;
            let new = state.domain(e.ids)?.id;
            let marked = core::cell::Cell::new(false);
            let mut to = Media {
                disk: &mut dst,
                state: &mut state,
                keys: &keys,
                pool,
                generation: g,
                read_scope: Some(new),
                write_scope: Some(new),
                writer: Some((&mut nonce, &marked)),
            };
            let converted = src.copy(e, old, &mut remap, &mut to)?;
            record_view(&mut canonical, original.generation, g, e, &converted);
            let lineage = domains
                .iter()
                .position(|d| d.id == new)
                .ok_or_else(corrupt)? as u8;
            views.insert(e.id, (lineage, e.kind as u8));
            next.entries.push(converted);
        }
        last_ids = domains
            .iter()
            .filter(|d| original.entries.iter().any(|e| e.ids == d.ids))
            .map(|d| d.id)
            .collect();
        inner_parent = install(&mut state.prefix, &next, size, inner_parent)?;
        state.proofs = state.signed_views(size, pool, &keys, None, false)?;
        {
            let verify = Media {
                disk: &mut dst,
                state: &mut state,
                keys: &keys,
                pool,
                generation: g,
                read_scope: None,
                write_scope: None,
                writer: None,
            };
            SharedPool::open(verify)?;
        }
        if let Some(l) = log.as_mut() {
            l.publish(&mut dst, &admin, |disk, g| state.save(disk, pool, g))?;
        } else {
            log = Some(CommitLog::format(&mut dst, 2, pool, &admin, |disk, g| {
                state.save(disk, pool, g)
            })?);
        }
    }
    let head = log.ok_or_else(corrupt)?.head();
    let (units, edges) = remap.identity();
    canonical.update(edges);
    let mut b = Vec::new();
    b.extend(RECEIPT);
    b.extend(input.pool);
    b.extend(previous_head.generation.to_le_bytes());
    b.extend(previous_head.digest);
    b.extend(previous_head.parent);
    b.extend(pool);
    b.extend(head.generation.to_le_bytes());
    b.extend(head.digest);
    b.extend(units.to_le_bytes());
    b.extend(<[u8; 32]>::from(canonical.finalize()));
    b.extend([input.profile, domains.len() as u8, views.len() as u8]);
    let mut counts = BTreeMap::<Id, [u64; 2]>::new();
    for (logical, ids) in remap.addresses() {
        let domain = domains.iter().find(|d| d.ids == ids).ok_or_else(corrupt)?;
        if domain.slots.is_some() {
            let page = dst.read(FIRST_UNIT + 2 * (logical - CONTROL as u64))?;
            let c = unit::context(&page)?;
            if c.domain != domain.id {
                return Err(corrupt());
            }
            counts.entry(domain.id).or_default()[usize::from(c.kind == Kind::Metadata)] += 1;
        }
    }
    for (old, new) in input.domains.iter().zip(&domains) {
        b.extend(old.ids.fs);
        b.extend(old.ids.lv);
        b.extend(old.id);
        b.extend(new.id);
        for n in counts.get(&new.id).copied().unwrap_or([0; 2]) {
            b.extend(n.to_le_bytes());
        }
    }
    for (id, (lineage, kind)) in views {
        b.extend(id.to_le_bytes());
        b.extend([
            lineage,
            kind,
            if kind == crate::shared_graph::Kind::Origin as u8 {
                0
            } else {
                1
            },
        ]);
    }
    b.extend(src.attest(&b));
    b.extend(admin.mac(&[NEW_LABEL, &b]));
    let receipt = MigrationReceipt::from_bytes(&b)?;
    src.confirm(&input)?;
    publish_receipt(dst, h, receipt, head, pool, &admin, &keys)
}

fn publish_receipt<T: Device>(
    mut dst: Staging<T>,
    mut header: Block,
    receipt: MigrationReceipt,
    head: Head,
    pool: Id,
    admin: &Secret,
    keys: &DomainKeys,
) -> Result<(T, MigrationReceipt)> {
    let b = receipt.as_bytes();
    header[112..114].copy_from_slice(&(b.len() as u16).to_le_bytes());
    header
        .get_mut(114..114 + b.len())
        .ok_or(Error::Limit("reçu P9"))?
        .copy_from_slice(b);
    dst.headers = [header, header];
    verify_destination_header(&header, admin)?;
    let mac = admin.mac(&[b"NovaFS/P7/session/header", &header]);
    let mut v = Validation::new(head, pool, admin, &mac, keys);
    let reopened = CommitLog::open(&dst, 2, pool, admin, |d, h| v.visit(d, h, None))?;
    super::inspect::verify_redundancy(&dst, pool, admin)?;
    if reopened.head() != head {
        return Err(corrupt());
    }
    // Les attestations sont durables sur le candidat privé. F1 précède
    // les deux en-têtes contenant le reçu. Un échec de publication ou F2
    // impose une nouvelle inspection de la destination.
    dst.disk.flush()?;
    let unknown = |cause| Error::Publication {
        phase: "publication du reçu P9",
        cause: Box::new(cause),
    };
    dst.disk.write(0, &header).map_err(unknown)?;
    dst.disk.write(1, &header).map_err(unknown)?;
    dst.disk.flush().map_err(unknown)?;
    if read_header(&dst.disk)? != header {
        return Err(corrupt());
    }
    Ok((dst.disk, receipt))
}
