//! Adaptateurs du laboratoire hôte. Aucun constructeur ne prend un chemin de périphérique.
//! Le simulateur représente le support (hors budget de travail du moteur).
use crate::{BLOCK_BYTES, Block, Device, Error, MAX_BLOCKS, Result};
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{self, ErrorKind};
use std::os::unix::fs::FileExt;
use std::sync::{Arc, Mutex, MutexGuard};

const BLOCK: u64 = BLOCK_BYTES as u64;
const MAX_SIM_BLOCKS: u64 = 32_768;
const TRACE_LIMIT: usize = 8_192;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Geometry {
    blocks: u64,
    sector: u32,
}

impl Geometry {
    pub fn new(blocks: u64, sector: u32) -> Result<Self> {
        if !matches!(sector, 512 | 4096) || !(1..=MAX_BLOCKS).contains(&blocks) {
            return Err(Error::Invalid("géométrie de l’image"));
        }
        Ok(Self { blocks, sector })
    }

    pub fn sector_size(self) -> u32 {
        self.sector
    }

    /// LBA natif et nombre de secteurs, pour une plage exprimée en blocs NovaFS.
    pub fn sectors(self, first: u64, count: u64) -> Result<(u64, u64)> {
        if count == 0 || first.checked_add(count).is_none_or(|end| end > self.blocks) {
            return Err(Error::Invalid("plage hors image"));
        }
        let ratio = BLOCK / u64::from(self.sector);
        Ok((first * ratio, count * ratio))
    }

    fn offset(self, index: u64) -> Result<u64> {
        Ok(self.sectors(index, 1)?.0 * u64::from(self.sector))
    }
}

/// Fichier ordinaire anonyme. Le même descripteur reste propriétaire de l'image
/// entre formatage, démontage et inspection. Aucune résolution de nom ultérieure.
pub struct FileImage {
    file: File,
    geometry: Geometry,
}

impl FileImage {
    pub fn create(blocks: u64, sector: u32) -> Result<Self> {
        let geometry = Geometry::new(blocks, sector)?;
        let file = tempfile::tempfile()?;
        if !file.metadata()?.file_type().is_file() {
            return Err(Error::Invalid("fichier temporaire ordinaire requis"));
        }
        file.set_len(blocks * BLOCK)?;
        Ok(Self { file, geometry })
    }

    pub fn geometry(&self) -> Geometry {
        self.geometry
    }
}

impl Device for FileImage {
    fn blocks(&self) -> u64 {
        self.geometry.blocks
    }

    fn read(&self, index: u64) -> Result<Block> {
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
}

fn injected() -> Error {
    Error::Io(io::Error::new(
        ErrorKind::Interrupted,
        "interruption injectée",
    ))
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

    fn persist(&mut self, index: u64, data: Arc<Block>) {
        if data.iter().all(|&byte| byte == 0) {
            self.medium.stable.remove(&index);
        } else {
            self.medium.stable.insert(index, data);
        }
    }

    fn cut(&mut self, how: Persistence) -> Result<()> {
        match how {
            Persistence::None => {}
            Persistence::Newest(n) => {
                let mut order: Vec<_> = self
                    .volatile
                    .iter()
                    .map(|(&i, v)| (v.sequence, i))
                    .collect();
                order.sort_unstable_by(|a, b| b.cmp(a));
                for (_, index) in order.into_iter().take(n) {
                    let data = self.volatile[&index].data.clone();
                    self.persist(index, data);
                }
            }
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
            Persistence::Prefix { block, bytes } => {
                self.medium.geometry.offset(block)?;
                if bytes > BLOCK as usize {
                    return Err(Error::Invalid("taille de déchirure"));
                }
                if let Some(v) = self.volatile.get(&block) {
                    let mut data = self.stable_block(block);
                    data[..bytes].copy_from_slice(&v.data[..bytes]);
                    self.persist(block, Arc::new(data));
                }
            }
        }
        self.volatile.clear();
        self.trace = None;
        self.interrupted = false;
        Ok(())
    }
}

pub struct SimDevice {
    shared: Arc<Mutex<State>>,
}

#[derive(Clone)]
pub struct SimControl {
    shared: Arc<Mutex<State>>,
}

impl SimControl {
    fn state(&self) -> MutexGuard<'_, State> {
        self.shared.lock().expect("contrôleur du banc interrompu")
    }

    pub fn snapshot(&self) -> SimSnapshot {
        self.state().medium.clone()
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
        self.state().cut(persistence)
    }

    /// Le service a quitté. Le cache du périphérique reste lisible.
    pub fn restart_service(&self) {
        let mut state = self.state();
        state.trace = None;
        state.interrupted = false;
    }

    pub fn replace_stable(&self, index: u64, bytes: Block) -> Result<()> {
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
        };
        Self {
            shared: Arc::new(Mutex::new(state)),
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
        }
    }

    pub fn geometry(&self) -> Geometry {
        self.shared.lock().unwrap().medium.geometry
    }
}

impl Device for SimDevice {
    fn blocks(&self) -> u64 {
        self.geometry().blocks
    }

    fn read(&self, index: u64) -> Result<Block> {
        let state = self.shared.lock().unwrap();
        state.medium.geometry.offset(index)?;
        if state.interrupted {
            return Err(injected());
        }
        Ok(match state.volatile.get(&index) {
            Some(v) => *v.data,
            None => state.stable_block(index),
        })
    }

    fn write(&mut self, index: u64, data: &Block) -> Result<()> {
        let mut state = self.shared.lock().unwrap();
        state.medium.geometry.offset(index)?;
        state.point(Event::BeforeWrite(index))?;
        state.sequence = state
            .sequence
            .checked_add(1)
            .ok_or(Error::Limit("séquence du banc"))?;
        let sequence = state.sequence;
        state.volatile.insert(
            index,
            Volatile {
                sequence,
                data: Arc::new(*data),
            },
        );
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
