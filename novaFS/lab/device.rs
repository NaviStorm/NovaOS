//! Adaptateurs du laboratoire hôte. Aucun constructeur ne prend un chemin de périphérique.
//! Le simulateur représente le support (hors budget de travail du moteur).
use crate::{BLOCK_BYTES, Block, Device, Error, Result};
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{self, ErrorKind};
use std::os::unix::fs::FileExt;
use std::sync::{Arc, Mutex, MutexGuard};

const BLOCK: u64 = BLOCK_BYTES as u64;
const MAX_SIM_BLOCKS: u64 = 32_768;
const TRACE_LIMIT: usize = 8_192;

pub use crate::geometry::Geometry;

/// Image dans un fichier ordinaire. Les E/S sont positionnelles et portent
/// toujours sur le descripteur initial, y compris après démontage.
pub struct FileImage {
    file: File,
    geometry: Geometry,
    mutation_lock: Option<Arc<Mutex<()>>>,
}

impl FileImage {
    pub fn create(blocks: u64, sector: u32) -> Result<Self> {
        let geometry = Geometry::new(blocks, sector)?;
        let file = tempfile::tempfile()?;
        if !file.metadata()?.is_file() {
            return Err(Error::Invalid("fichier temporaire ordinaire requis"));
        }
        file.set_len(blocks * BLOCK)?;
        Ok(Self {
            file,
            geometry,
            mutation_lock: Some(Arc::default()),
        })
    }

    /// Reprendre le descripteur d'une image de laboratoire existante. Le banc
    /// garde les autres écrivains exclus et la longueur fixe pendant son usage.
    /// Un descripteur ouvert en lecture seule convient au montage RO.
    pub fn from_file(file: File, blocks: u64, sector: u32) -> Result<Self> {
        let geometry = Geometry::new(blocks, sector)?;
        let metadata = file.metadata()?;
        if !metadata.is_file() || metadata.len() != blocks * BLOCK {
            return Err(Error::Invalid(
                "fichier ordinaire de longueur exacte requis",
            ));
        }
        Ok(Self {
            file,
            geometry,
            mutation_lock: None,
        })
    }

    /// Rend le descripteur après consommation du propriétaire de l’image.
    pub fn into_file(self) -> File {
        self.file
    }

    /// Occupation physique du fichier image rapportée par Darwin.
    pub fn allocated_bytes(&self) -> Result<u64> {
        use std::os::unix::fs::MetadataExt;
        self.file
            .metadata()?
            .blocks()
            .checked_mul(512)
            .ok_or(Error::Limit("occupation du fichier image"))
    }

    pub fn geometry(&self) -> Geometry {
        self.geometry
    }
}

impl Device for FileImage {
    fn mutation_lock(&self) -> Option<Arc<Mutex<()>>> {
        self.mutation_lock.clone()
    }
    fn blocks(&self) -> u64 {
        self.geometry.blocks
    }

    fn read(&self, index: u64) -> Result<Block> {
        #[cfg(feature = "latency-probes")]
        let _sample = crate::latency_probes::enter(crate::latency_probes::Phase::ImageRead);
        let mut data = [0; BLOCK as usize];
        self.file
            .read_exact_at(&mut data, self.geometry.offset(index)?)?;
        Ok(data)
    }

    fn write(&mut self, index: u64, data: &Block) -> Result<()> {
        self.file.write_all_at(data, self.geometry.offset(index)?)?;
        Ok(())
    }

    fn flush(&mut self) -> Result<()> {
        // Sur les deux cibles Darwin qualifiées, Rust appelle F_FULLFSYNC.
        // Sa réussite atteste la réponse de l'hôte à la demande de vidage.
        self.file.sync_all()?;
        Ok(())
    }
}

/// Une branche stable indépendante, copie à l'écriture du support virtuel.
#[derive(Clone)]
pub struct SimSnapshot {
    geometry: Geometry,
    stable: BTreeMap<u64, Arc<Block>>,
}

impl SimSnapshot {
    pub fn device(&self) -> SimDevice {
        SimDevice::from_snapshot(self.clone())
    }

    pub fn bytes(&self) -> Result<Vec<u8>> {
        let len = (self.geometry.blocks * BLOCK) as usize;
        let mut out = Vec::new();
        out.try_reserve_exact(len)
            .map_err(|_| Error::Limit("export du simulateur"))?;
        out.resize(len, 0);
        for (index, block) in &self.stable {
            let offset = (*index * BLOCK) as usize;
            out[offset..offset + BLOCK as usize].copy_from_slice(block.as_ref());
        }
        Ok(out)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Event {
    BeforeWrite(u64),
    WriteSubmitted(u64),
    WriteCompleted(u64),
    FlushSubmitted(u32),
    FlushEffect(u32),
    FlushCompleted(u32),
}

#[derive(Clone, Copy, Debug)]
pub enum Persistence {
    /// Perte de toutes les commandes encore volatiles.
    None,
    /// Stabiliser les n écritures les plus récentes, dans l'ordre inverse.
    Newest(usize),
    /// Secteurs natifs entiers anciens ou nouveaux, aux mêmes positions.
    Sectors { block: u64, mask: u8 },
    /// Support sans garantie d'atomicité du secteur choisi.
    Prefix { block: u64, bytes: usize },
    /// Plage contiguë d'une commande volatile ; les autres octets restent stables.
    #[cfg(test)]
    Range {
        block: u64,
        offset: usize,
        bytes: usize,
    },
}

/// Erreur d'E/S ponctuelle, distincte d'une coupure du service. Une commande
/// en échec peut avoir modifié tout ou partie du support ou de son cache.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SimFault {
    Read {
        block: u64,
    },
    /// Accepter ce préfixe dans le cache avant de signaler l'échec.
    Write {
        block: u64,
        prefix: usize,
    },
    /// Au n-ième flush après armement (à partir de 1), stabiliser au plus
    /// `persist_newest` blocs volatils en ordre inverse puis signaler l'échec.
    Flush {
        ordinal: u32,
        persist_newest: usize,
    },
}

#[derive(Default)]
struct Trace {
    events: Vec<Event>,
    fail_at: Option<usize>,
    ignore_flush: Option<u32>,
    flushes: u32,
}

struct Volatile {
    sequence: u64,
    data: Arc<Block>,
}

struct State {
    medium: SimSnapshot,
    volatile: BTreeMap<u64, Volatile>,
    sequence: u64,
    trace: Option<Trace>,
    interrupted: bool,
    fault: Option<SimFault>,
}

fn injected() -> Error {
    Error::Io(io::Error::new(
        ErrorKind::Interrupted,
        "interruption injectée",
    ))
}

fn io_fault() -> Error {
    // EIO dans sys/errno.h sur les deux cibles Darwin du laboratoire.
    const EIO: i32 = 5;
    io::Error::from_raw_os_error(EIO).into()
}

impl State {
    fn point(&mut self, event: Event) -> Result<()> {
        if self.interrupted {
            return Err(injected());
        }
        if let Some(trace) = &mut self.trace {
            if trace.events.len() >= TRACE_LIMIT {
                return Err(Error::Limit("journal du simulateur"));
            }
            trace.events.push(event);
            if trace.fail_at == Some(trace.events.len() - 1) {
                self.interrupted = true;
                return Err(injected());
            }
        }
        Ok(())
    }

    fn stable_block(&self, index: u64) -> Block {
        self.medium
            .stable
            .get(&index)
            .map_or([0; BLOCK as usize], |b| **b)
    }

    fn visible_block(&self, index: u64) -> Block {
        match self.volatile.get(&index) {
            Some(v) => *v.data,
            None => self.stable_block(index),
        }
    }

    fn queue(&mut self, index: u64, data: Block) -> Result<()> {
        self.sequence = self
            .sequence
            .checked_add(1)
            .ok_or(Error::Limit("séquence du banc"))?;
        self.volatile.insert(
            index,
            Volatile {
                sequence: self.sequence,
                data: Arc::new(data),
            },
        );
        Ok(())
    }

    fn persist(&mut self, index: u64, data: Arc<Block>) {
        if data.iter().all(|&byte| byte == 0) {
            self.medium.stable.remove(&index);
        } else {
            self.medium.stable.insert(index, data);
        }
    }

    fn persist_newest(&mut self, count: usize) -> Result<()> {
        if count == 0 {
            return Ok(());
        }
        // La capacité du support borne le nombre de blocs volatils distincts.
        let mut order = Vec::new();
        order
            .try_reserve_exact(self.volatile.len())
            .map_err(|_| Error::Limit("ordre des écritures du simulateur"))?;
        order.extend(self.volatile.iter().map(|(&i, v)| (v.sequence, i)));
        order.sort_unstable_by(|a, b| b.cmp(a));
        for (_, index) in order.into_iter().take(count) {
            let v = self
                .volatile
                .remove(&index)
                .expect("bloc volatil répertorié");
            self.persist(index, v.data);
        }
        Ok(())
    }

    fn persist_range(&mut self, block: u64, offset: usize, bytes: usize) -> Result<()> {
        self.medium.geometry.offset(block)?;
        let end = offset
            .checked_add(bytes)
            .filter(|&end| end <= BLOCK as usize)
            .ok_or(Error::Invalid("plage de déchirure"))?;
        if let Some(v) = self.volatile.get(&block) {
            let mut data = self.stable_block(block);
            data[offset..end].copy_from_slice(&v.data[offset..end]);
            self.persist(block, Arc::new(data));
        }
        Ok(())
    }

    fn cut(&mut self, how: Persistence) -> Result<()> {
        match how {
            Persistence::None => {}
            Persistence::Newest(n) => self.persist_newest(n)?,
            Persistence::Sectors { block, mask } => {
                self.medium.geometry.offset(block)?;
                let sector = self.medium.geometry.sector as usize;
                let count = BLOCK as usize / sector;
                if u16::from(mask) >= 1u16 << count {
                    return Err(Error::Invalid("masque de secteurs"));
                }
                if let Some(v) = self.volatile.get(&block) {
                    let mut data = self.stable_block(block);
                    for i in 0..count {
                        if mask & (1 << i) != 0 {
                            data[i * sector..(i + 1) * sector]
                                .copy_from_slice(&v.data[i * sector..(i + 1) * sector]);
                        }
                    }
                    self.persist(block, Arc::new(data));
                }
            }
            Persistence::Prefix { block, bytes } => self.persist_range(block, 0, bytes)?,
            #[cfg(test)]
            Persistence::Range {
                block,
                offset,
                bytes,
            } => self.persist_range(block, offset, bytes)?,
        }
        self.volatile.clear();
        self.trace = None;
        self.interrupted = false;
        self.fault = None;
        Ok(())
    }
}

pub struct SimDevice {
    gate: Arc<Mutex<()>>,
    shared: Arc<Mutex<State>>,
}

#[derive(Clone)]
pub struct SimControl {
    gate: Arc<Mutex<()>>,
    shared: Arc<Mutex<State>>,
}

impl SimControl {
    fn state(&self) -> MutexGuard<'_, State> {
        self.shared.lock().expect("contrôleur du banc interrompu")
    }

    pub fn snapshot(&self) -> SimSnapshot {
        self.state().medium.clone()
    }

    /// Une seule injection peut être armée. L'EIO la consomme ; couper
    /// l'alimentation ou redémarrer le banc annule un plan encore en attente.
    pub fn inject(&self, fault: SimFault) -> Result<()> {
        let mut state = self.state();
        if state.fault.is_some() || state.interrupted {
            return Err(Error::Busy);
        }
        match fault {
            SimFault::Write { prefix, .. } if prefix > BLOCK_BYTES => {
                return Err(Error::Invalid("préfixe d'écriture"));
            }
            SimFault::Read { block } | SimFault::Write { block, .. } => {
                state.medium.geometry.offset(block)?;
            }
            SimFault::Flush { ordinal: 0, .. } => return Err(Error::Invalid("rang du flush")),
            SimFault::Flush { .. } => {}
        }
        state.fault = Some(fault);
        Ok(())
    }

    /// Le compteur d'événement commence à 0 et celui des flushs à 1.
    pub fn trace(&self, fail_at: Option<usize>, ignore_flush: Option<u32>) {
        self.state().trace = Some(Trace {
            fail_at,
            ignore_flush,
            ..Trace::default()
        });
    }

    pub fn events(&self) -> Vec<Event> {
        self.state()
            .trace
            .as_ref()
            .map_or_else(Vec::new, |t| t.events.clone())
    }

    pub fn power_cut(&self, persistence: Persistence) -> Result<()> {
        let _guard = self.gate.try_lock().map_err(|_| Error::Busy)?;
        self.state().cut(persistence)
    }

    /// Le service a quitté. Le cache du périphérique reste lisible.
    pub fn restart_service(&self) {
        let _guard = self.gate.lock().expect("phase du simulateur interrompue");
        let mut state = self.state();
        state.trace = None;
        state.interrupted = false;
        state.fault = None;
    }

    pub fn replace_stable(&self, index: u64, bytes: Block) -> Result<()> {
        let _guard = self.gate.try_lock().map_err(|_| Error::Busy)?;
        let mut state = self.state();
        state.medium.geometry.offset(index)?;
        if !state.volatile.is_empty() {
            return Err(Error::Busy);
        }
        state.persist(index, Arc::new(bytes));
        Ok(())
    }
}

impl SimDevice {
    pub fn new(blocks: u64, sector: u32) -> Result<Self> {
        if blocks > MAX_SIM_BLOCKS {
            return Err(Error::Limit("capacité du simulateur"));
        }
        let geometry = Geometry::new(blocks, sector)?;
        Ok(Self::from_snapshot(SimSnapshot {
            geometry,
            stable: BTreeMap::new(),
        }))
    }

    fn from_snapshot(medium: SimSnapshot) -> Self {
        let state = State {
            medium,
            volatile: BTreeMap::new(),
            sequence: 0,
            trace: None,
            interrupted: false,
            fault: None,
        };
        Self {
            shared: Arc::new(Mutex::new(state)),
            gate: Arc::default(),
        }
    }

    pub fn from_bytes(bytes: &[u8], sector: u32) -> Result<Self> {
        if bytes.len() % BLOCK as usize != 0 {
            return Err(Error::Invalid("image tronquée"));
        }
        let device = Self::new((bytes.len() / BLOCK as usize) as u64, sector)?;
        {
            let mut state = device.shared.lock().unwrap();
            for (i, block) in bytes.chunks_exact(BLOCK as usize).enumerate() {
                if block.iter().any(|&b| b != 0) {
                    state
                        .medium
                        .stable
                        .insert(i as u64, Arc::new(block.try_into().expect("bloc complet")));
                }
            }
        }
        Ok(device)
    }

    pub fn control(&self) -> SimControl {
        SimControl {
            shared: self.shared.clone(),
            gate: self.gate.clone(),
        }
    }

    pub fn geometry(&self) -> Geometry {
        self.shared.lock().unwrap().medium.geometry
    }
}

impl Device for SimDevice {
    fn mutation_lock(&self) -> Option<Arc<Mutex<()>>> {
        Some(self.gate.clone())
    }
    fn blocks(&self) -> u64 {
        self.geometry().blocks
    }

    fn read(&self, index: u64) -> Result<Block> {
        let mut state = self.shared.lock().unwrap();
        state.medium.geometry.offset(index)?;
        if state.interrupted {
            return Err(injected());
        }
        if state.fault == Some(SimFault::Read { block: index }) {
            state.fault = None;
            return Err(io_fault());
        }
        Ok(state.visible_block(index))
    }

    fn write(&mut self, index: u64, data: &Block) -> Result<()> {
        let mut state = self.shared.lock().unwrap();
        state.medium.geometry.offset(index)?;
        state.point(Event::BeforeWrite(index))?;
        if let Some(SimFault::Write { block, prefix }) = state.fault
            && block == index
        {
            state.fault = None;
            if prefix != 0 {
                let mut partial = state.visible_block(index);
                partial[..prefix].copy_from_slice(&data[..prefix]);
                state.queue(index, partial)?;
            }
            return Err(io_fault());
        }
        state.queue(index, *data)?;
        state.point(Event::WriteSubmitted(index))?;
        state.point(Event::WriteCompleted(index))
    }

    fn flush(&mut self) -> Result<()> {
        let mut state = self.shared.lock().unwrap();
        let ordinal = match &mut state.trace {
            None => 0,
            Some(trace) => {
                trace.flushes += 1;
                trace.flushes
            }
        };
        state.point(Event::FlushSubmitted(ordinal))?;
        if let Some(SimFault::Flush {
            ordinal: remaining,
            persist_newest,
        }) = state.fault
        {
            if remaining == 1 {
                state.fault = None;
                state.persist_newest(persist_newest)?;
                return Err(io_fault());
            }
            state.fault = Some(SimFault::Flush {
                ordinal: remaining - 1,
                persist_newest,
            });
        }
        if state
            .trace
            .as_ref()
            .is_none_or(|t| t.ignore_flush != Some(ordinal))
        {
            for (index, v) in std::mem::take(&mut state.volatile) {
                state.persist(index, v.data);
            }
        }
        state.point(Event::FlushEffect(ordinal))?;
        state.point(Event::FlushCompleted(ordinal))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn geometric_and_temporary_io() -> Result<()> {
        for sector in [512, 4096] {
            let mut file = FileImage::create(520, sector)?;
            assert_eq!(
                file.geometry().sectors(257, 1)?,
                (257 * (4096 / u64::from(sector)), 4096 / u64::from(sector))
            );
            for i in [0, 1, 256, 257, 519] {
                let pattern = [(i % 251) as u8; 4096];
                file.write(i, &pattern)?;
                file.flush()?;
                assert_eq!(file.read(i)?, pattern);
            }
            assert!(file.write(520, &[1; 4096]).is_err());
            assert!(file.read(u64::MAX).is_err());
            assert!(file.geometry().sectors(2, u64::MAX).is_err());
        }
        assert!(FileImage::create(512, 1024).is_err());
        Ok(())
    }

    #[test]
    fn reprise_fichier_par_descripteur_et_refus_des_geometries_inexactes() -> Result<()> {
        for sector in [512, 4096] {
            let temp = tempfile::NamedTempFile::new()?;
            temp.as_file().set_len(2 * BLOCK)?;
            assert!(FileImage::from_file(temp.reopen()?, 1, sector).is_err());
            assert!(FileImage::from_file(temp.reopen()?, 3, sector).is_err());
            assert!(FileImage::from_file(temp.reopen()?, 2, 2048).is_err());
            let mut writer = FileImage::from_file(temp.reopen()?, 2, sector)?;
            writer.write(1, &[0x41; BLOCK_BYTES])?;
            writer.flush()?;
            drop(writer);
            let mut reader = FileImage::from_file(File::open(temp.path())?, 2, sector)?;
            assert_eq!(reader.read(1)?, [0x41; BLOCK_BYTES]);
            assert!(matches!(
                reader.write(1, &[0; BLOCK_BYTES]),
                Err(Error::Io(_))
            ));
            assert_eq!(reader.read(1)?, [0x41; BLOCK_BYTES]);
        }
        let directory = tempfile::tempdir()?;
        assert!(FileImage::from_file(File::open(directory.path())?, 1, 512).is_err());
        Ok(())
    }

    #[test]
    fn erreurs_de_lecture_et_ecriture_partielle_a_usage_unique() -> Result<()> {
        for sector in [512, 4096] {
            let mut disk = SimDevice::new(4, sector)?;
            let ctl = disk.control();
            assert!(ctl.inject(SimFault::Read { block: 4 }).is_err());
            assert!(
                ctl.inject(SimFault::Write {
                    block: 2,
                    prefix: BLOCK_BYTES + 1
                })
                .is_err()
            );
            assert!(
                ctl.inject(SimFault::Flush {
                    ordinal: 0,
                    persist_newest: 2
                })
                .is_err()
            );
            ctl.inject(SimFault::Read { block: 1 })?;
            assert!(matches!(
                ctl.inject(SimFault::Read { block: 0 }),
                Err(Error::Busy)
            ));
            assert_eq!(disk.read(0)?, [0; BLOCK_BYTES]);
            assert!(matches!(disk.read(1), Err(Error::Io(e)) if e.raw_os_error() == Some(5)));
            assert_eq!(disk.read(1)?, [0; BLOCK_BYTES]);

            for prefix in [0, 1, sector as usize - 1, BLOCK_BYTES] {
                disk.write(2, &[1; BLOCK_BYTES])?;
                disk.flush()?;
                disk.write(2, &[2; BLOCK_BYTES])?;
                ctl.inject(SimFault::Write { block: 2, prefix })?;
                assert!(matches!(
                    disk.write(2, &[3; BLOCK_BYTES]),
                    Err(Error::Io(_))
                ));
                let mut expected = [2; BLOCK_BYTES];
                expected[..prefix].fill(3);
                assert_eq!(disk.read(2)?, expected);
                assert_eq!(ctl.snapshot().device().read(2)?, [1; BLOCK_BYTES]);
                ctl.power_cut(Persistence::Newest(1))?;
                assert_eq!(disk.read(2)?, expected);
            }
        }
        Ok(())
    }

    #[test]
    fn flush_en_erreur_peut_stabiliser_une_partie_ou_tout_le_cache() -> Result<()> {
        for sector in [512, 4096] {
            for newest in [0, 1, 2, usize::MAX] {
                let mut disk = SimDevice::new(8, sector)?;
                let ctl = disk.control();
                ctl.inject(SimFault::Flush {
                    ordinal: 2,
                    persist_newest: newest,
                })?;
                disk.write(4, &[4; BLOCK_BYTES])?;
                disk.flush()?;
                disk.write(2, &[2; BLOCK_BYTES])?;
                disk.write(5, &[5; BLOCK_BYTES])?;
                disk.write(1, &[1; BLOCK_BYTES])?;
                assert!(matches!(disk.flush(), Err(Error::Io(e)) if e.raw_os_error() == Some(5)));
                for b in [1, 2, 4, 5] {
                    assert_eq!(disk.read(b)?, [b as u8; BLOCK_BYTES]);
                }
                ctl.power_cut(Persistence::None)?;
                assert_eq!(disk.read(4)?, [4; BLOCK_BYTES]);
                for (rank, b) in [1u64, 5, 2].into_iter().enumerate() {
                    let expected = if rank < newest { b as u8 } else { 0 };
                    assert_eq!(disk.read(b)?, [expected; BLOCK_BYTES]);
                }
                disk.write(0, &[8; BLOCK_BYTES])?;
                disk.flush()?;
                assert_eq!(ctl.snapshot().device().read(0)?, [8; BLOCK_BYTES]);
            }
        }
        Ok(())
    }

    #[test]
    fn power_cut_reverses_and_tears_only_selected_sectors() -> Result<()> {
        let mut disk = SimDevice::new(9, 512)?;
        let host = disk.control();
        disk.write(2, &[7; 4096])?;
        disk.write(3, &[8; 4096])?;
        host.power_cut(Persistence::Newest(1))?;
        assert_eq!(disk.read(2)?, [0; 4096]);
        assert_eq!(disk.read(3)?, [8; 4096]);
        disk.write(3, &[9; 4096])?;
        host.power_cut(Persistence::Sectors {
            block: 3,
            mask: 0x55,
        })?;
        for (index, chunk) in disk.read(3)?.chunks_exact(512).enumerate() {
            assert_eq!(chunk, [if index % 2 == 0 { 9 } else { 8 }; 512]);
        }
        Ok(())
    }
}
