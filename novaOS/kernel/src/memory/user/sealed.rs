//! Préparation privée d'un objet identifié. Aucun pointeur utilisateur ni mapping DMA
//! ne désigne les octets que mesurera le chargeur.
use super::{Error, Result};
use alloc::vec::Vec;
use core::sync::atomic::{AtomicUsize, Ordering};
use sha2::{Digest, Sha256};

pub(crate) const MAX_IMAGE_BYTES: usize = 4 << 20;
const PREPARATION_BYTES: usize = 2 * MAX_IMAGE_BYTES;
static RESERVED: AtomicUsize = AtomicUsize::new(0);

/// L'autorité de lecture fournit l'identité de vue avant la première lecture.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ObjectRevision {
    pub(crate) volume: [u8; 16],
    pub(crate) object: u64,
    pub(crate) revision: u64,
    pub(crate) length: usize,
}

struct Reservation(usize);
impl Drop for Reservation {
    fn drop(&mut self) {
        RESERVED.fetch_sub(self.0, Ordering::AcqRel);
    }
}

pub(crate) struct ImageStaging {
    object: ObjectRevision,
    expected: [u8; 32],
    bytes: Vec<u8>,
    reservation: Reservation,
}

pub(crate) struct SealedImage {
    object: ObjectRevision,
    bytes: Vec<u8>,
    _reservation: Reservation,
}

impl ImageStaging {
    /// L'appelant de confiance a déjà vérifié le droit de préparer cette vue.
    /// La réservation suit la copie scellée jusqu'à la fin du chargement.
    pub(crate) fn new(object: ObjectRevision, expected: [u8; 32]) -> Result<Self> {
        if !(1..=MAX_IMAGE_BYTES).contains(&object.length) {
            return Err(Error::Geometry);
        }
        RESERVED
            .try_update(Ordering::AcqRel, Ordering::Acquire, |used| {
                used.checked_add(object.length)
                    .filter(|n| *n <= PREPARATION_BYTES)
            })
            .map_err(|_| Error::Exhausted)?;
        let reservation = Reservation(object.length);
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(object.length)
            .map_err(|_| Error::Exhausted)?;
        Ok(Self {
            object,
            expected,
            bytes,
            reservation,
        })
    }

    /// Tout fragment invalide détruit la préparation. L'appelant ne peut pas
    /// intercaler une autre révision puis poursuivre avec l'ancienne.
    pub(crate) fn append(
        mut self,
        object: ObjectRevision,
        offset: usize,
        data: &[u8],
    ) -> Result<Self> {
        if object != self.object
            || offset != self.bytes.len()
            || data.is_empty()
            || data.len() > super::PAGE as usize
            || data.len() > self.object.length - offset
        {
            return Err(Error::Verification);
        }
        self.bytes.extend_from_slice(data);
        Ok(self)
    }

    pub(crate) fn seal(self) -> Result<SealedImage> {
        if self.bytes.len() != self.object.length
            || Sha256::digest(&self.bytes)[..] != self.expected
        {
            return Err(Error::Verification);
        }
        Ok(SealedImage {
            object: self.object,
            bytes: self.bytes,
            _reservation: self.reservation,
        })
    }
}

impl SealedImage {
    pub(crate) fn object(&self) -> ObjectRevision {
        self.object
    }
    pub(super) fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}
