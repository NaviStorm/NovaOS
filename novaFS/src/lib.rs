//! Prototype hôte NovaFS P1. Le moteur lit et écrit le format de laboratoire P0.
mod cache;
pub use cache::CacheStats;
pub mod codec;
#[path = "../lab/device.rs"]
pub mod device;
mod engine;
mod fs;
mod space;
pub mod tree;

pub use codec::{Ids, Manifest, Ref};
pub use engine::{Engine, Mode, SpaceStats, Tx, View};
pub use fs::{Kind, Stat};

pub const BLOCK_BYTES: usize = 4096;
pub const MAX_BLOCKS: u64 = 2_097_152;
pub type Block = [u8; BLOCK_BYTES];
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    Corrupt(&'static str),
    Invalid(&'static str),
    Limit(&'static str),
    Io(std::io::Error),
    NotFound,
    Exists,
    NoSpace,
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
impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
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
impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
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
    fn blocks(&self) -> u64;
    fn read(&self, block: u64) -> Result<Block>;
    fn write(&mut self, block: u64, data: &Block) -> Result<()>;
    fn flush(&mut self) -> Result<()>;
}

impl<D: Device> Device for &mut D {
    fn blocks(&self) -> u64 {
        (**self).blocks()
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
}
