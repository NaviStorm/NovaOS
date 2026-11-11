//! Moteur NovaFS commun aux adaptateurs hôtes et aux cibles nues. Contenu P0, vues CoW, compression et chiffrement de volumes.
#![cfg_attr(target_os = "none", no_std)]
#[macro_use]
extern crate alloc;
pub mod geometry;
pub mod io;
pub mod sync;
use alloc::boxed::Box;
mod cache;
pub use cache::CacheStats;
pub mod codec;
mod compression;
pub mod crypto;
#[cfg(not(target_os = "none"))]
#[path = "../lab/device.rs"]
pub mod device;
mod engine;
mod fs;
#[cfg(all(feature = "latency-probes", not(target_os = "none")))]
#[path = "../lab/latency_probes.rs"]
pub mod latency_probes;
pub mod pool;
mod secure_commit;

#[cfg(test)]
pub(crate) static KDF_TEST: crate::sync::Mutex<()> = crate::sync::Mutex::new(());
pub mod secure_export;
pub mod secure_pool;
pub mod secure_unit;
pub mod shared;
pub mod shared_export;
mod shared_graph;
pub use shared_graph::ObjectRole;
mod space;
pub mod time;
pub mod tree;

pub use codec::{Ids, Manifest, Ref};
pub use engine::integrity::{
    CheckIssue, IntegrityReport, PairStatus, SlotReport, check_image,
    check_image_with_bitmap_budget,
};
pub use engine::{
    Engine, Mode, PreparedWrite, ReservationCost, SpaceStats, Tx, View, VolumeAccounting,
    VolumePolicy,
};
pub use fs::{Kind, Stat};

pub const BLOCK_BYTES: usize = 4096;
pub const MAX_BLOCKS: u64 = 2_097_152;
pub type Block = [u8; BLOCK_BYTES];
/// Pages privées d'une transaction, effacées sur toutes les sorties, y compris abandon.
type PrivatePage = Box<zeroize::Zeroizing<Block>>;
fn private_page(page: Block) -> PrivatePage {
    Box::new(zeroize::Zeroizing::new(page))
}

/// Politique persistante appliquée aux nouvelles pages de données d'une vue.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum Compression {
    #[default]
    None = 0,
    Lz4 = 1,
}
pub type Result<T> = core::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    Corrupt(&'static str),
    Invalid(&'static str),
    Limit(&'static str),
    Io(crate::io::Error),
    NotFound,
    Exists,
    NoSpace,
    PoolFull,
    QuotaExceeded,
    MaintenanceExhausted,
    NoMemory,
    Busy,
    ReadOnly,
    UnknownOutcome,
    Unavailable,
    Publication {
        phase: &'static str,
        cause: Box<Error>,
    },
    NotDirectory,
    NotEmpty,
    IsDirectory,
}
impl From<crate::io::Error> for Error {
    fn from(e: crate::io::Error) -> Self {
        Self::Io(e)
    }
}
impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NoSpace => write!(f, "espace ordinaire du volume insuffisant"),
            Self::PoolFull => write!(f, "espace physique du pool insuffisant"),
            Self::QuotaExceeded => write!(f, "quota dépassé"),
            Self::MaintenanceExhausted => write!(f, "réserve de maintenance insuffisante"),
            Self::Publication { phase, cause } => {
                write!(f, "publication indéterminée ({phase}) : {cause}")
            }
            Self::Io(e) => write!(f, "E/S : {e}"),
            Self::Corrupt(s) => write!(f, "image incohérente : {s}"),
            Self::Invalid(s) => write!(f, "argument invalide : {s}"),
            Self::Limit(s) => write!(f, "limite P0 : {s}"),
            _ => write!(f, "{self:?}"),
        }
    }
}
impl core::error::Error for Error {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::Publication { cause, .. } => Some(cause.as_ref()),
            _ => None,
        }
    }
}

/// E/S par bloc entier de 4 Kio, indexé depuis zéro. Le détenteur assure
/// l'exclusivité de l'écrivain et conserve la géométrie pendant le montage.
/// Une écriture réussie est acceptée et lisible ; son contenu peut encore
/// dépendre d'un cache volatil. Un flush réussi confirme les écritures
/// précédentes dans le domaine de persistance déclaré par le backend.
/// Un échec d'écriture ou de flush peut avoir pris effet partiellement ou
/// entièrement. Une erreur de lecture ne fournit aucun bloc à l'appelant.
pub trait Device {
    /// Le conteneur P9 authentifie une genèse importée. Un pool historique la refuse.
    fn imported_genesis(&self) -> bool {
        false
    }

    /// Verrou partagé par tous les acteurs pouvant modifier ce support P7.
    /// Un backend qui ne peut exclure les écritures étrangères reste consultable.
    fn mutation_lock(&self) -> Option<crate::sync::Arc<crate::sync::Mutex<()>>> {
        None
    }
    fn blocks(&self) -> u64;
    /// Plafond logique stable pendant la durée de la vue, imposé par son propriétaire. Une image
    /// FS autonome peut conserver les fichiers creux du profil P0 sans plafond.
    fn logical_quota_bytes(&self) -> Option<u64> {
        None
    }
    fn read(&self, block: u64) -> Result<Block>;
    fn write(&mut self, block: u64, data: &Block) -> Result<()>;
    fn flush(&mut self) -> Result<()>;

    /// Un adaptateur chiffré contrôle le domaine attendu et la sous-clé de la page.
    fn typed_fs_pages(&self) -> bool {
        false
    }
    fn check_file_class(&self, _b: u64, _file_data: bool) -> Result<()> {
        Ok(())
    }
    fn check_lineage(&self, _block: u64, _ids: Ids) -> Result<()> {
        Ok(())
    }
    fn read_object(
        &self,
        block: u64,
        context: ObjectRole,
        expected_hash: &[u8; 32],
    ) -> Result<Block> {
        let page = self.read(block)?;
        if context.historical_hash(block, &page) != *expected_hash {
            return Err(Error::Corrupt("checksum de page physique P5"));
        }
        Ok(page)
    }

    /// Renvoie la référence de l'objet effectivement écrit. Un support chiffré
    /// lie sa référence à l'enveloppe persistante et authentifiée.
    fn write_object(&mut self, block: u64, data: &Block, context: ObjectRole) -> Result<[u8; 32]> {
        let hash = context.historical_hash(block, data);
        self.write(block, data)?;
        Ok(hash)
    }
}

impl<D: Device> Device for &mut D {
    fn imported_genesis(&self) -> bool {
        (**self).imported_genesis()
    }
    fn mutation_lock(&self) -> Option<crate::sync::Arc<crate::sync::Mutex<()>>> {
        (**self).mutation_lock()
    }
    fn blocks(&self) -> u64 {
        (**self).blocks()
    }
    fn logical_quota_bytes(&self) -> Option<u64> {
        (**self).logical_quota_bytes()
    }
    fn read(&self, b: u64) -> Result<Block> {
        (**self).read(b)
    }
    fn write(&mut self, b: u64, data: &Block) -> Result<()> {
        (**self).write(b, data)
    }
    fn flush(&mut self) -> Result<()> {
        (**self).flush()
    }
    fn typed_fs_pages(&self) -> bool {
        (**self).typed_fs_pages()
    }
    fn check_file_class(&self, b: u64, data: bool) -> Result<()> {
        (**self).check_file_class(b, data)
    }
    fn check_lineage(&self, b: u64, ids: Ids) -> Result<()> {
        (**self).check_lineage(b, ids)
    }
    fn read_object(
        &self,
        block: u64,
        context: ObjectRole,
        expected_hash: &[u8; 32],
    ) -> Result<Block> {
        (**self).read_object(block, context, expected_hash)
    }
    fn write_object(&mut self, block: u64, data: &Block, context: ObjectRole) -> Result<[u8; 32]> {
        (**self).write_object(block, data, context)
    }
}
