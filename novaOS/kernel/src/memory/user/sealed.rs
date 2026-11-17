//! Préparation privée d'un objet identifié. Aucun pointeur utilisateur ni mapping DMA
//! ne désigne les octets que mesurera le chargeur.
use super::PAGE;
use super::{Error, Result};
use crate::{
    hal::memory::{Access, VirtualAddress},
    heap,
};
use sha2::{Digest, Sha256};

pub(crate) const MAX_IMAGE_BYTES: usize = 4 << 20;
// Fenêtre superviseur de 8 Mio dans ARENA. Seules les pages en cours de
// préparation sont mappées. Les métadonnées sont sous le verrou de Memory.
const STAGING_BASE: usize = 0xffff_f040_0000_0000;
const PAGE_BYTES: usize = PAGE as usize;
const PAGES: usize = 2 * MAX_IMAGE_BYTES / PAGE_BYTES;

pub(in crate::memory) struct StagingArena([u64; PAGES / 64]);
impl StagingArena {
    pub(in crate::memory) const fn empty() -> Self {
        Self([0; PAGES / 64])
    }

    fn first_free(&self, pages: usize) -> Option<usize> {
        let mut run = 0;
        for i in 0..PAGES {
            if self.0[i / 64] & (1 << (i % 64)) != 0 {
                run = 0;
            } else {
                run += 1;
                if run == pages {
                    return Some(i + 1 - pages);
                }
            }
        }
        None
    }

    fn toggle(&mut self, first: usize, count: usize) {
        for i in first..first + count {
            self.0[i / 64] ^= 1 << (i % 64);
        }
    }

    pub(in crate::memory) fn resident(&self) -> u64 {
        self.0.iter().map(|word| u64::from(word.count_ones())).sum()
    }
}

struct PrivatePages {
    first: usize,
    length: usize,
}

impl PrivatePages {
    fn address(index: usize) -> VirtualAddress {
        VirtualAddress(STAGING_BASE + index * PAGE_BYTES)
    }

    fn release(m: &mut super::Memory, first: usize, count: usize) {
        for i in (first..first + count).rev() {
            // Sûreté : pages exclusivement au superviseur. À la destruction
            // aucun emprunt ne subsiste. Le retrait synchronise le TLB.
            unsafe { m.release_page(Self::address(i)) }.expect("page ELF réservée");
        }
    }

    fn new(length: usize) -> Result<Self> {
        let count = length.div_ceil(PAGE_BYTES);
        heap::with_pages(|m| {
            let first = m.staging.first_free(count).ok_or(Error::Exhausted)?;
            for i in first..first + count {
                if let Err(e) = m.allocate_page(Self::address(i), Access::ReadWrite) {
                    Self::release(m, first, i - first);
                    return Err(e);
                }
            }
            m.staging.toggle(first, count);
            Ok(Self { first, length })
        })
    }

    fn bytes(&self) -> &[u8] {
        // Sûreté : la fenêtre mappée est contiguë, initialisée et privée.
        unsafe {
            core::slice::from_raw_parts(Self::address(self.first).0 as *const u8, self.length)
        }
    }
    fn bytes_mut(&mut self) -> &mut [u8] {
        // Sûreté : l'objet possède seul la plage. Aucun handle ni alias DMA.
        unsafe {
            core::slice::from_raw_parts_mut(Self::address(self.first).0 as *mut u8, self.length)
        }
    }
}

impl Drop for PrivatePages {
    fn drop(&mut self) {
        let count = self.length.div_ceil(PAGE_BYTES);
        heap::with_pages(|m| {
            Self::release(m, self.first, count);
            m.staging.toggle(self.first, count);
            Ok(())
        })
        .expect("restitution des pages ELF");
    }
}

/// L'autorité de lecture fournit l'identité de vue avant la première lecture.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ObjectRevision {
    pub(crate) volume: [u8; 16],
    pub(crate) object: u64,
    pub(crate) revision: u64,
    pub(crate) length: usize,
}

pub(crate) struct ImageStaging {
    object: ObjectRevision,
    expected: [u8; 32],
    bytes: PrivatePages,
    received: usize,
}

pub(crate) struct SealedImage {
    digest: [u8; 32],
    object: ObjectRevision,
    bytes: PrivatePages,
}

impl ImageStaging {
    /// L'appelant de confiance a déjà vérifié le droit de préparer cette vue.
    /// La réservation suit la copie scellée jusqu'à la fin du chargement.
    pub(crate) fn new(object: ObjectRevision, expected: [u8; 32]) -> Result<Self> {
        if !(1..=MAX_IMAGE_BYTES).contains(&object.length) {
            return Err(Error::Geometry);
        }
        Ok(Self {
            object,
            expected,
            bytes: PrivatePages::new(object.length)?,
            received: 0,
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
            || offset != self.received
            || data.is_empty()
            || data.len() > super::PAGE as usize
            || data.len() > self.object.length - offset
        {
            return Err(Error::Verification);
        }
        self.bytes.bytes_mut()[offset..offset + data.len()].copy_from_slice(data);
        self.received += data.len();
        Ok(self)
    }

    pub(crate) fn seal(self) -> Result<SealedImage> {
        if self.received != self.object.length
            || Sha256::digest(self.bytes.bytes())[..] != self.expected
        {
            return Err(Error::Verification);
        }
        Ok(SealedImage {
            digest: self.expected,
            object: self.object,
            bytes: self.bytes,
        })
    }
}

impl SealedImage {
    pub(crate) fn digest(&self) -> [u8; 32] { self.digest }
    pub(crate) fn object(&self) -> ObjectRevision {
        self.object
    }
    pub(super) fn bytes(&self) -> &[u8] {
        self.bytes.bytes()
    }
}
