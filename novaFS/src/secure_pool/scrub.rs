//! Scrub du dernier état observable. Chaque appel repart du support sous sa garde.
//! Le rappel de progression peut suspendre puis reprendre le même parcours sous
//! verrou. Un abandon rend toute preuve locale inutilisable par un appel ultérieur.
use super::*;
use crate::shared_graph::{Graph, PageRef};
use std::cell::{Cell, RefCell};
use std::time::Instant;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Diagnostic,
    Full,
    Repair,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Status {
    Healthy = 0,
    Damaged = 1,
    Repaired = 4,
    Irreparable = 5,
    Incomplete = 3,
    Io = 6,
    MissingKeys = 7,
}
impl Status {
    pub fn label(self) -> &'static str {
        match self {
            Self::Healthy => "coherent",
            Self::Damaged => "corruption",
            Self::Repaired => "repare",
            Self::Irreparable => "non_reparable",
            Self::Incomplete => "incomplet",
            Self::Io => "erreur_io",
            Self::MissingKeys => "authentification_indisponible",
        }
    }
}
pub struct Authority<'a> {
    pub pool: [u8; 16],
    pub admin: &'a Secret,
    pub domains: &'a [([u8; 16], Credential<'a>)],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Issue {
    pub stage: &'static str,
    pub generation: Option<u64>,
    pub view: Option<u64>,
    pub volume: Option<[u8; 16]>,
    /// Plage de la structure, ou LBA de l'E/S défaillante. Jamais un LBA déduit
    /// arbitrairement de la dernière lecture réussie.
    pub blocks: Option<(u64, u64)>,
    pub reason: &'static str,
    pub corruption: bool,
}
enum Finding {
    Corruption(&'static str),
    Incomplete(&'static str),
}
impl From<&Error> for Finding {
    fn from(e: &Error) -> Self {
        match e {
            Error::Corrupt(_) | Error::Invalid(_) => Self::Corruption(reason(e)),
            _ => Self::Incomplete(reason(e)),
        }
    }
}
pub struct Report {
    pub run: [u8; 16],
    pub status: Status,
    pub format: &'static str,
    pub generation: Option<u64>,
    pub authenticated: bool,
    pub views: u64,
    pub reads: u64,
    pub distinct_blocks: u64,
    pub locked_views: Vec<(u64, u64)>,
    pub writes: u64,
    pub flushes: u64,
    pub elapsed_ns: u64,
    pub possible: u8,
    pub repaired: u8,
    pub irreparable: u32,
    pub issues: Vec<Issue>,
}
impl Report {
    fn new(run: [u8; 16]) -> Self {
        Self {
            run,
            status: Status::Incomplete,
            format: "inconnu",
            generation: None,
            authenticated: false,
            views: 0,
            reads: 0,
            distinct_blocks: 0,
            locked_views: Vec::new(),
            writes: 0,
            flushes: 0,
            elapsed_ns: 0,
            possible: 0,
            repaired: 0,
            irreparable: 0,
            issues: Vec::new(),
        }
    }
    fn issue(
        &mut self,
        stage: &'static str,
        generation: Option<u64>,
        view: Option<u64>,
        blocks: Option<(u64, u64)>,
        finding: Finding,
    ) {
        let (reason, corruption) = match finding {
            Finding::Corruption(reason) => (reason, true),
            Finding::Incomplete(reason) => (reason, false),
        };
        self.issues.push(Issue {
            stage,
            generation,
            view,
            volume: None,
            blocks,
            reason,
            corruption,
        });
    }
}

pub struct Progress {
    pub run: [u8; 16],
    pub reads: u64,
    pub block: u64,
}
struct Scan<'a, D, F> {
    disk: &'a mut D,
    size: u64,
    run: [u8; 16],
    progress: RefCell<F>,
    reads: Cell<u64>,
    visited: RefCell<Vec<u64>>,
    writes: u64,
    flushes: u64,
    interrupted: Cell<bool>,
    io_error: Cell<Option<u64>>,
    object_error: Cell<Option<u64>>,
    writable: bool,
}
impl<D: Device, F: FnMut(Progress) -> bool> Device for Scan<'_, D, F> {
    fn blocks(&self) -> u64 {
        self.size
    }
    fn read(&self, block: u64) -> Result<Block> {
        let reads = self.reads.get();
        if self.interrupted.get()
            || (reads % 64 == 0
                && !(self.progress.borrow_mut())(Progress {
                    run: self.run,
                    reads,
                    block,
                }))
        {
            self.interrupted.set(true);
            return Err(Error::Busy);
        }
        if self.disk.blocks() != self.size {
            return Err(Error::UnknownOutcome);
        }
        if block >= self.size {
            return Err(Error::Corrupt("LBA hors support"));
        }
        self.visited.borrow_mut()[block as usize / 64] |= 1 << (block % 64);
        self.reads.set(reads + 1);
        self.disk
            .read(block)
            .inspect_err(|_| self.io_error.set(Some(block)))
    }
    fn read_object(&self, block: u64, role: ObjectRole, expected: &Digest) -> Result<Block> {
        ReadOnly(self)
            .read_object(block, role, expected)
            .inspect_err(|_| self.object_error.set(Some(block)))
    }
    fn write(&mut self, b: u64, p: &Block) -> Result<()> {
        if !self.writable {
            return Err(Error::ReadOnly);
        }
        if self.disk.blocks() != self.size || b >= self.size {
            return Err(Error::UnknownOutcome);
        }
        self.writes += 1;
        self.disk
            .write(b, p)
            .inspect_err(|_| self.io_error.set(Some(b)))
    }
    fn flush(&mut self) -> Result<()> {
        if !self.writable {
            return Err(Error::ReadOnly);
        }
        if self.disk.blocks() != self.size {
            return Err(Error::UnknownOutcome);
        }
        self.flushes += 1;
        self.disk.flush()
    }
}

struct Copies {
    head: Head,
    parent: Option<Head>,
    missing: Vec<u64>,
}
fn copies(d: &impl Device, pool: Id, key: Option<&Secret>, report: &mut Report) -> Result<Copies> {
    let mut records = [None; 8];
    let mut broken = [false; 8];
    for (i, record) in records.iter_mut().enumerate() {
        let page = d.read(2 + i as u64)?;
        if page == [0; 4096] {
            continue;
        }
        let result = match key {
            Some(k) => Head::inspect_authenticated(&page, pool, 2, i as u64, k),
            None => Head::inspect_public(&page, pool, 2, i as u64),
        };
        match result {
            Ok(h) => *record = Some(h),
            Err(_) => broken[i] = true,
        }
    }
    let head = records
        .iter()
        .flatten()
        .max_by_key(|h| h.generation)
        .copied()
        .ok_or(Error::Corrupt("aucune attestation utilisable"))?;
    report.generation = Some(head.generation);
    let parent = if head.generation > 1 {
        let h = records
            .iter()
            .flatten()
            .find(|r| r.generation == head.generation - 1)
            .ok_or(Error::Corrupt("attestation parente inaccessible"))?;
        if head.parent != h.digest {
            return Err(Error::Corrupt("filiation du journal"));
        }
        Some(*h)
    } else {
        None
    };
    for pair in records.chunks_exact(2) {
        if let [Some(a), Some(b)] = pair
            && a.generation == b.generation
            && a != b
        {
            return Err(Error::Corrupt("copies concurrentes divergentes"));
        }
    }
    let mut missing = Vec::new();
    for i in 0..8 {
        let lba = 2 + i as u64;
        let expected = [Some(head), parent]
            .into_iter()
            .flatten()
            .find(|h| h.generation % 4 == i as u64 / 2);
        if let Some(h) = expected
            && records[i] != Some(h)
        {
            if records[i].is_some() {
                return Err(Error::UnknownOutcome);
            }
            missing.push(lba);
            report.issue(
                "attestation",
                Some(h.generation),
                None,
                Some((lba, 1)),
                Finding::Corruption("réplique absente ou invalide"),
            );
        } else if broken[i]
            || (expected.is_none()
                && records[i].is_none()
                && (1..=3).any(|age| {
                    head.generation
                        .checked_sub(age)
                        .is_some_and(|g| g > 0 && g % 4 == i as u64 / 2)
                }))
        {
            report.issue(
                "attestation",
                None,
                None,
                Some((lba, 1)),
                Finding::Corruption("copie hors paire protégée invalide"),
            );
            report.irreparable += 1;
        }
    }
    // Une banque préparée seule ne prouve pas la publication. Si elle paraît
    // plus récente, une perte des deux attestations et un arrêt avant publication
    // peuvent être indiscernables sans reçu indépendant.
    for slot in 0..4 {
        if [Some(head), parent]
            .into_iter()
            .flatten()
            .any(|h| h.generation % 4 == slot)
        {
            continue;
        }
        let base = FIRST_BANK + slot * BANK_BLOCKS;
        let page = d.read(base)?;
        if &page[..8] == b"NP7DIR01"
            && page[8..24] == pool
            && u64::from_le_bytes(page[24..32].try_into().unwrap()) > head.generation
        {
            report.issue(
                "fraicheur",
                None,
                None,
                Some((base, BANK_BLOCKS)),
                Finding::Incomplete("banque plus récente sans publication établie"),
            );
            return Err(Error::UnknownOutcome);
        }
    }
    Ok(Copies {
        head,
        parent,
        missing,
    })
}

fn reason(error: &Error) -> &'static str {
    match error {
        Error::Corrupt(s) | Error::Limit(s) | Error::Invalid(s) => s,
        Error::Io(_) => "erreur du périphérique",
        Error::ReadOnly => "clé ou autorité en écriture absente",
        Error::UnknownOutcome => "issue de publication indéterminée",
        Error::Busy => "contrôle interrompu ou occupé",
        Error::Unavailable => "authentification indisponible",
        _ => "validation incomplète",
    }
}
fn failure(error: &Error, mode: Mode) -> Status {
    match error {
        Error::Io(_) | Error::Publication { .. } => Status::Io,
        Error::ReadOnly | Error::Unavailable => Status::MissingKeys,
        Error::Corrupt(_) | Error::Invalid(_) if mode == Mode::Repair => Status::Irreparable,
        Error::Corrupt(_) | Error::Invalid(_) => Status::Damaged,
        _ => Status::Incomplete,
    }
}

fn keys(state: &State, auth: &Authority<'_>) -> Result<BTreeMap<Id, Keys>> {
    let mut keys = BTreeMap::new();
    for (id, cred) in auth.domains {
        let slots = state
            .domains
            .iter()
            .find(|d| d.id == *id)
            .and_then(|d| d.slots.as_ref())
            .ok_or(Error::Unavailable)?;
        let cred = match cred {
            Credential::Recovery(s) => Credential::Recovery(s),
            Credential::Passphrase(p) => Credential::Passphrase(p),
        };
        let secret = slots.unlock(id, cred).map_err(|_| Error::Unavailable)?;
        if keys.insert(*id, Keys::derive(&secret, id)?).is_some() {
            return Err(Error::Invalid("domaine répété"));
        }
    }
    Ok(keys)
}

fn load_state(d: &impl Device, pool: Id, h: Head, report: &mut Report) -> Result<State> {
    State::load(d, pool, &h).inspect_err(|e| {
        report.issue(
            "banque",
            Some(h.generation),
            None,
            Some((FIRST_BANK + h.generation % 4 * BANK_BLOCKS, BANK_BLOCKS)),
            e.into(),
        )
    })
}

/// Conserve la paire fautive uniquement si l'opération sur cet objet échoue.
/// Une erreur de relation entre objets conserve seulement l'identité de la vue.
struct Located<'a, 'm, D> {
    media: &'a Media<'m, D>,
    pair: Cell<Option<u64>>,
}
impl<D: Device> Located<'_, '_, D> {
    fn locate<T>(&self, b: u64, result: Result<T>) -> Result<T> {
        result.inspect_err(|_| self.pair.set(self.media.physical(b).ok()))
    }
}
impl<D: Device> Device for Located<'_, '_, D> {
    fn blocks(&self) -> u64 {
        self.media.blocks()
    }
    fn read(&self, b: u64) -> Result<Block> {
        self.media.read(b)
    }
    fn typed_fs_pages(&self) -> bool {
        true
    }
    fn check_file_class(&self, b: u64, data: bool) -> Result<()> {
        self.locate(b, self.media.check_file_class(b, data))
    }
    fn check_lineage(&self, b: u64, ids: Ids) -> Result<()> {
        self.locate(b, self.media.check_lineage(b, ids))
    }
    fn read_object(&self, b: u64, r: ObjectRole, h: &Digest) -> Result<Block> {
        self.locate(b, self.media.read_object(b, r, h))
    }
    fn write(&mut self, _: u64, _: &Block) -> Result<()> {
        Err(Error::ReadOnly)
    }
    fn flush(&mut self) -> Result<()> {
        Err(Error::ReadOnly)
    }
}

fn inspect_views<D: Device>(
    d: &D,
    pool: Id,
    h: Head,
    state: &mut State,
    keys: &BTreeMap<Id, Keys>,
    report: &mut Report,
) -> Result<()> {
    let mut ro = ReadOnly(d);
    let mut media = Media {
        disk: &mut ro,
        state,
        keys,
        pool,
        generation: h.generation,
        read_scope: None,
        write_scope: None,
        writer: None,
    };
    for e in Graph::views_at_head(&media, pool)? {
        report.views += 1;
        let domain = media.state.domain(e.ids)?;
        let id = domain.id;
        let encrypted = domain.slots.is_some();
        media.read_scope = Some(id);
        let root = if e.root == PageRef::EMPTY {
            None
        } else {
            Some(media.physical(e.root.block)?)
        };
        let located = Located {
            media: &media,
            pair: Cell::new(None),
        };
        let result = if encrypted && !keys.contains_key(&id) {
            report.locked_views.push((h.generation, e.id));
            // Le graphe est opaque. Seule sa racine publique et son empreinte
            // attendue peuvent être rapprochées du catalogue.
            if let Some(b) = root {
                located.pair.set(Some(b));
                (|| {
                    let bytes = d.read(b)?;
                    let ctx = unit::context(&bytes)?;
                    if ctx.pool != pool
                        || ctx.domain != id
                        || ctx.physical != b
                        || ctx.kind != Kind::Metadata
                        || ctx.logical != 1 << 32
                        || ctx.generation > h.generation
                    {
                        return Err(corrupt());
                    }
                    let desc = unit::inspect(ctx, &bytes, &d.read(b + 1)?, &e.root.hash)?;
                    if !desc.encrypted {
                        return Err(corrupt());
                    }
                    Ok(())
                })()
            } else {
                Ok(())
            }
        } else {
            crate::shared::scrub_view(&located, pool, e.id)
        };
        if let Err(e2) = result {
            if !matches!(e2, Error::Corrupt(_) | Error::Invalid(_)) {
                return Err(e2);
            }
            report.issue(
                "vue",
                Some(h.generation),
                Some(e.id),
                located.pair.get().map(|b| (b, 2)),
                (&e2).into(),
            );
            report.irreparable += 1;
        }
    }
    Ok(())
}

fn inspect_generations(
    disk: &impl Device,
    pool: Id,
    copies: &Copies,
    state: &mut State,
    keys: &BTreeMap<Id, Keys>,
    report: &mut Report,
) -> Result<()> {
    let size = coord(disk.blocks())?;
    let inspect = |h: Head, s: &mut State, report: &mut Report| {
        s.signed_views(size, pool, keys, Some([0; 16]), true)?;
        for id in keys.keys() {
            s.signed_views(size, pool, keys, Some(*id), true)?;
        }
        inspect_views(disk, pool, h, s, keys, report)
    };
    if let Some(h) = copies.parent {
        let mut parent = load_state(disk, pool, h, report)?;
        let from = Graph::views_at_head(
            &Prefix {
                state: &parent,
                size,
            },
            pool,
        )?;
        let catalogs = Graph::visible_catalogs(&Prefix { state, size }, pool)?;
        if catalogs.get(1).is_none_or(|c| c.entries != from) {
            return Err(corrupt());
        }
        inspect(h, &mut parent, report)?;
    }
    inspect(copies.head, state, report)
}

/// Contrôle frais après la barrière. Une preuve antérieure ne sert que de
/// valeur attendue. Le rapport conserve la localisation des nouveaux défauts.
fn recheck(
    disk: &impl Device,
    pool: Id,
    admin: &Secret,
    keys: &BTreeMap<Id, Keys>,
    expected: &Copies,
    missing: &[u64],
    report: &mut Report,
) -> Result<()> {
    let mut fresh = Report::new(report.run);
    let result = (|| {
        let now = copies(disk, pool, Some(admin), &mut fresh)?;
        if now.head != expected.head || now.parent != expected.parent || now.missing != missing {
            return Err(Error::UnknownOutcome);
        }
        let mut state = load_state(disk, pool, now.head, &mut fresh)?;
        inspect_generations(disk, pool, &now, &mut state, keys, &mut fresh)?;
        if fresh.irreparable != 0 || !fresh.locked_views.is_empty() {
            return Err(corrupt());
        }
        Ok(())
    })();
    if result.is_err() {
        for issue in fresh.issues {
            if !report.issues.contains(&issue) {
                report.issues.push(issue);
            }
        }
        report.irreparable = report.irreparable.max(fresh.irreparable);
    }
    result
}

fn p7<D: Device, F: FnMut(Progress) -> bool>(
    disk: &mut Scan<'_, D, F>,
    auth: Option<&Authority<'_>>,
    mode: Mode,
    report: &mut Report,
) -> Result<()> {
    let header = read_header(disk)
        .inspect_err(|e| report.issue("en-tetes", None, None, Some((0, 2)), e.into()))?;
    let pool: Id = header[16..32].try_into().unwrap();
    let admin = auth
        .map(|a| {
            if a.pool != pool {
                return Err(Error::Unavailable);
            }
            publication_key(&header, pool, a.admin).map_err(|_| Error::Unavailable)
        })
        .transpose()?;
    let copies = copies(disk, pool, admin.as_ref(), report)?;
    let mut state = load_state(disk, pool, copies.head, report)?;
    let keys = auth
        .map(|a| keys(&state, a))
        .transpose()?
        .unwrap_or_default();
    let needed_keys = state
        .domains
        .iter()
        .filter(|d| d.slots.is_some())
        .all(|d| keys.contains_key(&d.id));
    inspect_generations(disk, pool, &copies, &mut state, &keys, report)?;
    if report.irreparable != 0 {
        report.status = if mode == Mode::Repair {
            Status::Irreparable
        } else {
            Status::Damaged
        };
        return Ok(());
    }
    let Some(admin) = admin.filter(|_| needed_keys && report.locked_views.is_empty()) else {
        report.status = if mode == Mode::Diagnostic && !report.issues.is_empty() {
            Status::Damaged
        } else if mode == Mode::Diagnostic {
            Status::Incomplete
        } else {
            Status::MissingKeys
        };
        return Ok(());
    };
    let mac = admin.mac(&[b"NovaFS/P7/session/header", &header]);
    let mut validate = Validation::new(copies.head, pool, &admin, &mac, &keys);
    let mut log = CommitLog::open(disk, 2, pool, &admin, |d, h| validate.visit(d, h, None))?;
    if log.head() != copies.head {
        return Err(corrupt());
    }
    report.authenticated = true;
    report.possible = copies.missing.len() as u8;
    if copies.missing.is_empty() {
        report.status = Status::Healthy;
        return Ok(());
    }
    if mode != Mode::Repair {
        report.status = Status::Damaged;
        return Ok(());
    }
    // Refaire le contrôle strict de *tous* les emplacements après le drainage,
    // avant que resume puisse écrire une réplique. Un seul verrou couvre ces E/S.
    let mut post = Validation::new(copies.head, pool, &admin, &mac, &keys);
    let mut checked_slots = false;
    disk.writable = true;
    let repaired = log.resume(disk, &admin, |d, h| {
        if !checked_slots {
            recheck(d, pool, &admin, &keys, &copies, &copies.missing, report)?;
            checked_slots = true;
        }
        post.visit(d, h, None)
    })?;
    disk.writable = false;
    if !repaired {
        return Err(Error::UnknownOutcome);
    }
    verify_commit_replicas(disk, pool, auth.unwrap().admin)?;
    let mut reopened = Validation::new(copies.head, pool, &admin, &mac, &keys);
    if CommitLog::open(disk, 2, pool, &admin, |d, h| reopened.visit(d, h, None))?.head()
        != copies.head
    {
        return Err(Error::UnknownOutcome);
    }
    recheck(disk, pool, &admin, &keys, &copies, &[], report)?;
    report.repaired = report.possible;
    report.status = Status::Repaired;
    Ok(())
}

fn inspect_lv(
    report: &mut Report,
    fs: crate::IntegrityReport,
    volume: Id,
    local: bool,
) -> Result<()> {
    let mut first = None;
    for s in fs.slots {
        if let Some(issue) = s.issue {
            report.issue(
                issue.stage,
                report.generation,
                None,
                issue.block.filter(|_| local).map(|b| (b, 1)),
                (&issue.error).into(),
            );
            report.issues.last_mut().unwrap().volume = Some(volume);
            first.get_or_insert(issue.error);
        }
    }
    if let Some(e) = first {
        return Err(e);
    }
    match fs.pair {
        crate::PairStatus::Verified { .. } => Ok(()),
        crate::PairStatus::Rejected(issue) => Err(issue.error),
        _ => Err(corrupt()),
    }
}

/// Sur un espace sans verrou, le détenteur fournit une image hors ligne stable.
/// La réparation exige toujours le verrou du backend. Le diagnostic ne draine
/// jamais de commandes. Une nouvelle invocation reconstruit toutes ses preuves.
pub fn scrub<D: Device>(
    device: &mut D,
    mode: Mode,
    auth: Option<&Authority<'_>>,
    progress: impl FnMut(Progress) -> bool,
) -> Result<Report> {
    let started = Instant::now();
    let mut run = [0; 16];
    HostEntropy.fill(&mut run)?;
    let mut report = Report::new(run);
    let gate = device.mutation_lock();
    if gate.is_none() && mode == Mode::Repair {
        return Err(Error::ReadOnly);
    }
    let _guard = gate
        .as_ref()
        .map(|g| g.try_lock().map_err(|_| Error::Busy))
        .transpose()?;
    let blocks = device.blocks();
    if !(1..=crate::MAX_BLOCKS).contains(&blocks) {
        return Err(Error::Invalid("géométrie du scrub"));
    }
    let mut visited = Vec::new();
    let words = blocks.div_ceil(64) as usize;
    visited
        .try_reserve_exact(words)
        .map_err(|_| Error::NoMemory)?;
    visited.resize(words, 0);
    let mut scan = Scan {
        disk: device,
        size: blocks,
        run: report.run,
        progress: RefCell::new(progress),
        reads: Cell::new(0),
        visited: RefCell::new(visited),
        writes: 0,
        flushes: 0,
        interrupted: Cell::new(false),
        io_error: Cell::new(None),
        object_error: Cell::new(None),
        writable: false,
    };
    let result = (|| {
        if looks_like(&scan)? || auth.is_some() {
            report.format = "P7";
            p7(&mut scan, auth, mode, &mut report)
        } else if crate::shared::looks_like_pool(&scan)? {
            report.format = "P5/P6";
            let (catalogs, mixed) = Graph::scrub_catalogs(&scan)?;
            report.format = if catalogs[0].p6 { "P6" } else { "P5" };
            report.generation = Some(catalogs[0].generation);
            for catalog in catalogs {
                for entry in catalog.entries {
                    report.views += 1;
                    scan.object_error.set(None);
                    if let Err(e) =
                        crate::shared::scrub_historical_view(&scan, catalog.generation, &entry)
                    {
                        if !matches!(e, Error::Invalid(_) | Error::Corrupt(_)) {
                            return Err(e);
                        }
                        report.issue(
                            "vue",
                            Some(catalog.generation),
                            Some(entry.id),
                            scan.object_error.get().map(|b| (b, 1)),
                            (&e).into(),
                        );
                        report.irreparable += 1;
                    }
                }
            }
            if report.irreparable != 0 {
                report.status = if mode == Mode::Repair {
                    Status::Irreparable
                } else {
                    Status::Damaged
                };
            } else {
                let union = SharedPool::open(ReadOnly(&scan))?;
                if Some(union.generation()) != report.generation {
                    return Err(Error::UnknownOutcome);
                }
                report.status = if mixed {
                    Status::Incomplete
                } else {
                    Status::Healthy
                };
                if mixed {
                    report.issue(
                        "sélecteur",
                        report.generation,
                        None,
                        Some((2, 1)),
                        Finding::Incomplete("publication partielle"),
                    );
                }
            }
            Ok(())
        } else {
            // Une ancre P0 déclare les identités. L'autre ancre et les deux
            // graphes sont ensuite validés contre cette même paire.
            for slot in 0..scan.blocks().min(2) {
                let page = scan.read(slot)?;
                if &page[..8] == b"NOVAP0FS" {
                    report.format = "LV P0";
                    let anchor = match crate::codec::Anchor::decode(&page, slot) {
                        Ok(a) => a,
                        Err(e) => {
                            if slot == 0 && scan.blocks() >= 2 && &scan.read(1)?[..8] == b"NOVAP0FS"
                            {
                                continue;
                            }
                            report.issue("ancre", None, None, Some((slot, 1)), (&e).into());
                            return Err(e);
                        }
                    };
                    let fs = crate::check_image(&scan, anchor.ids);
                    report.generation = fs
                        .slots
                        .iter()
                        .filter_map(|s| s.anchor.as_ref().map(|a| a.manifest.birth))
                        .max();
                    report.views = 1;
                    inspect_lv(&mut report, fs, anchor.ids.lv, true)?;
                    report.status = Status::Healthy;
                    return Ok(());
                }
            }
            report.format = "catalogue P0";
            let p = crate::pool::check_pool(&scan);
            for s in p.slots {
                s?;
            }
            report.generation = Some(p.pair.ok_or_else(corrupt)??);
            report.status = Status::Healthy;
            for v in p.volumes {
                report.views += 1;
                match v.check {
                    Some(fs) => inspect_lv(&mut report, fs, v.id, false)?,
                    None => report.status = Status::Incomplete,
                }
            }
            Ok(())
        }
    })();
    if let Err(e) = result {
        report.status = if scan.interrupted.get() {
            Status::Incomplete
        } else {
            failure(&e, mode)
        };
        if report.issues.last().is_none_or(|i| i.reason != reason(&e))
            || scan.io_error.get().is_some()
        {
            report.issue(
                "parcours",
                report.generation,
                None,
                scan.io_error.get().map(|b| (b, 1)),
                (&e).into(),
            );
        }
        if matches!(report.status, Status::Irreparable | Status::Damaged) && report.irreparable == 0
        {
            report.irreparable = 1
        }
        report.authenticated = false;
        report.possible = 0;
    }
    report.distinct_blocks = scan
        .visited
        .into_inner()
        .iter()
        .map(|w| u64::from(w.count_ones()))
        .sum();
    report.reads = scan.reads.get();
    report.writes = scan.writes;
    report.flushes = scan.flushes;
    report.elapsed_ns = started.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64;
    Ok(report)
}
