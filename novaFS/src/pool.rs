//! Catalogue de volumes hôte. Le contrôle P2 demeure en lecture seule.
use crate::{Block, Device, Error, Ids, IntegrityReport, Result, check_image};
mod catalog;
pub(crate) mod coordinator;
mod manager;
use catalog::{Entry, load, validate_pair};
pub use catalog::{GRAIN, Type, Uid};
pub use coordinator::{PoolCoordinator, ReadRequest, WriteRequest};
pub use manager::{
    Pool, PoolSpaceStats, View, VolumeInfo, VolumeReservation, inspect_space, inspect_usage,
};

#[derive(Debug)]
pub struct PoolReport {
    /// Chaque génération locale a passé le contrôle de son ancre et de sa bande.
    pub slots: [Result<u64>; 2],
    /// Absence si une bande est illisible ; la filiation requiert les deux slots.
    pub pair: Option<Result<u64>>,
    pub volumes: Vec<VolumeCheck>,
}

#[derive(Debug)]
pub struct VolumeCheck {
    pub id: Uid,
    pub fs: Uid,
    pub kind: Type,
    pub blocks: u64,
    /// Seul un volume déclaré prêt contient obligatoirement un FS vérifiable.
    pub check: Option<IntegrityReport>,
}

struct ReadVolume<'a, D> {
    disk: &'a D,
    entry: &'a Entry,
}

impl<D: Device> Device for ReadVolume<'_, D> {
    fn blocks(&self) -> u64 {
        self.entry.grains * GRAIN
    }
    fn logical_quota_bytes(&self) -> Option<u64> {
        (self.entry.kind != Type::Raw).then(|| self.blocks() * 4096)
    }
    fn read(&self, block: u64) -> Result<Block> {
        self.disk.read(self.entry.physical(block)?)
    }
    fn write(&mut self, _: u64, _: &Block) -> Result<()> {
        Err(Error::ReadOnly)
    }
    fn flush(&mut self) -> Result<()> {
        Err(Error::ReadOnly)
    }
}

/// La migration examine les deux catalogues avant d'ouvrir des vues strictement
/// en lecture seule. Un LV brut ou en préparation demande de garder la source P0.
pub(crate) fn visit_ready<D: Device>(
    dev: &D,
    mut visit: impl FnMut(Uid, Ids, &dyn Device) -> Result<()>,
) -> Result<()> {
    let (mut old, mut head) = (load(dev, 0)?, load(dev, 1)?);
    if head.0.generation < old.0.generation {
        std::mem::swap(&mut old, &mut head);
    }
    validate_pair(&old.0, old.1, &head.0, head.1)?;
    if head.0.entries.iter().any(|e| e.kind != Type::Ready) {
        return Err(Error::Invalid("migration P5 limitée aux pools de LV prêts"));
    }
    for e in &head.0.entries {
        visit(
            e.owner,
            Ids { fs: e.fs, lv: e.id },
            &ReadVolume {
                disk: dev,
                entry: e,
            },
        )?;
    }
    Ok(())
}

/// Le type du rapport distingue les catalogues locaux, leur filiation et le
/// contenu des LV prêts. Aucune vue n'est dérivée d'une paire non vérifiée.
pub fn check_pool<D: Device>(dev: &D) -> PoolReport {
    let left = load(dev, 0);
    let right = load(dev, 1);
    let (left, right) = match (left, right) {
        (Ok(left), Ok(right)) => (left, right),
        (a, b) => {
            return PoolReport {
                slots: [a.map(|v| v.0.generation), b.map(|v| v.0.generation)],
                pair: None,
                volumes: Vec::new(),
            };
        }
    };
    let slots = [Ok(left.0.generation), Ok(right.0.generation)];
    let ((old, old_hash), (head, hash)) = if left.0.generation <= right.0.generation {
        (left, right)
    } else {
        (right, left)
    };
    let mut volumes = Vec::new();
    let pair = validate_pair(&old, old_hash, &head, hash).and_then(|()| {
        volumes
            .try_reserve_exact(head.entries.len())
            .map_err(|_| Error::NoMemory)?;
        for entry in &head.entries {
            volumes.push(VolumeCheck {
                id: entry.id,
                fs: entry.fs,
                kind: entry.kind,
                blocks: entry.grains * GRAIN,
                check: (entry.kind == Type::Ready).then(|| {
                    check_image(
                        &ReadVolume { disk: dev, entry },
                        Ids {
                            fs: entry.fs,
                            lv: entry.id,
                        },
                    )
                }),
            });
        }
        Ok(head.generation)
    });
    PoolReport {
        slots,
        pair: Some(pair),
        volumes,
    }
}
