//! Contrôle public des deux générations protégées. L'adaptateur de lecture
//! rejette toute écriture ou barrière, y compris pour le parcours des vues claires.
use super::*;
use crate::secure_commit::COMMIT_BLOCKS;
use crate::shared_graph::{Graph, PageRef};

pub struct StructuralReport {
    pub pool: Id,
    pub generation: u64,
    pub protected_generations: u8,
    pub attestations: u8,
    pub redundant_copies: bool,
    pub active_views: Vec<u64>,
    pub checked_root_references: u32,
    pub encrypted_root_references: u32,
}

struct ReadOnly<'a, D>(&'a D);
impl<D: Device> Device for ReadOnly<'_, D> {
    fn blocks(&self) -> u64 {
        self.0.blocks()
    }
    fn read(&self, b: u64) -> Result<Block> {
        self.0.read(b)
    }
    fn write(&mut self, _: u64, _: &Block) -> Result<()> {
        Err(Error::ReadOnly)
    }
    fn flush(&mut self) -> Result<()> {
        Err(Error::ReadOnly)
    }
}

fn inspect_generation<D: Device>(d: &D, h: Head, report: &mut StructuralReport) -> Result<()> {
    let pool = report.pool;
    let mut state = State::load(d, pool, &h)?;
    // Zéro ne peut être l'identité d'un domaine. Ce contrôle recopie les tags
    // opaques et vérifie la totalité du bourrage public et des entrées claires.
    state.signed_views(
        coord(d.blocks())?,
        pool,
        &BTreeMap::new(),
        Some([0; 16]),
        true,
    )?;
    let mut disk = ReadOnly(d);
    let keys = BTreeMap::new();
    let mut media = Media {
        disk: &mut disk,
        state: &mut state,
        keys: &keys,
        pool,
        generation: h.generation,
        read_scope: None,
        write_scope: None,
        writer: None,
    };
    for e in Graph::views_at_head(&media, pool)? {
        let domain = media.state.domain(e.ids)?;
        if h.generation == report.generation {
            report.active_views.push(e.id);
        }
        if e.root == PageRef::EMPTY {
            continue;
        }
        let physical = media.physical(e.root.block)?;
        let header = d.read(physical)?;
        let ctx = unit::context(&header)?;
        let (kind, logical) = role(ObjectRole::Root);
        if ctx.pool != pool
            || ctx.domain != domain.id
            || ctx.physical != physical
            || ctx.logical != logical
            || ctx.kind != kind
            || ctx.generation > h.generation
        {
            return Err(corrupt());
        }
        let descriptor = unit::inspect(ctx, &header, &d.read(physical + 1)?, &e.root.hash)?;
        if descriptor.encrypted != domain.slots.is_some() {
            return Err(corrupt());
        }
        report.checked_root_references += 1;
        if descriptor.encrypted {
            report.encrypted_root_references += 1;
        } else {
            // Sans clé, les liens internes d'une vue claire sont accessibles.
            media.read_scope = Some(domain.id);
            Graph::inspect_one(&media, pool, e.id, |_, _| Ok(()))?;
        }
    }
    report.protected_generations += 1;
    Ok(())
}

pub fn structural_check(d: &impl Device) -> Result<StructuralReport> {
    let pool = inspect_header(d)?;
    let mut records = [None; COMMIT_BLOCKS as usize];
    let mut count = 0u8;
    for (offset, record) in records.iter_mut().enumerate() {
        let block = d.read(2 + offset as u64)?;
        if block != [0; 4096] {
            *record = Some(Head::inspect_public(&block, pool, 2, offset as u64)?);
            count += 1;
        }
    }
    for pair in records.chunks_exact(2) {
        if let [Some(a), Some(b)] = pair
            && a.generation == b.generation
            && a != b
        {
            return Err(corrupt());
        }
    }
    let head = *records
        .iter()
        .flatten()
        .max_by_key(|h| h.generation)
        .ok_or_else(corrupt)?;
    let mut report = StructuralReport {
        pool,
        generation: head.generation,
        attestations: count,
        redundant_copies: protected_redundancy(&records, head.generation),
        protected_generations: 0,
        active_views: Vec::new(),
        checked_root_references: 0,
        encrypted_root_references: 0,
    };
    if head.generation > 1 {
        let older = *records
            .iter()
            .flatten()
            .find(|h| h.generation == head.generation - 1)
            .ok_or_else(corrupt)?;
        if older.digest != head.parent {
            return Err(corrupt());
        }
        inspect_generation(d, older, &mut report)?;
    }
    inspect_generation(d, head, &mut report)?;
    Ok(report)
}

fn protected_redundancy(records: &[Option<Head>; COMMIT_BLOCKS as usize], generation: u64) -> bool {
    (generation.saturating_sub(1).max(1)..=generation).all(|g| {
        let first = (g % 4) as usize * 2;
        matches!((records[first], records[first + 1]),
            (Some(a), Some(b)) if a.generation == g && a == b)
    })
}

/// Contrôle strict hors ligne de chaque copie présente, y compris celles
/// que la sélection de reprise peut écarter après une écriture déchirée.
pub fn verify_commit_replicas(d: &impl Device, pool: Id, administration: &Secret) -> Result<()> {
    let key = super::publication_key(d, pool, administration)?;
    let mut records = [None; COMMIT_BLOCKS as usize];
    for (offset, record) in records.iter_mut().enumerate() {
        let offset = offset as u64;
        let b = d.read(2 + offset)?;
        if b != [0; 4096] {
            *record = Some(Head::inspect_authenticated(&b, pool, 2, offset, &key)?);
        }
    }
    let g = records
        .iter()
        .flatten()
        .map(|h| h.generation)
        .max()
        .ok_or_else(corrupt)?;
    if !protected_redundancy(&records, g) {
        return Err(Error::Corrupt(
            "redondance dégradée des attestations P7 protégées",
        ));
    }
    Ok(())
}
