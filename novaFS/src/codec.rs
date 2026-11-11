//! Enveloppes P0. Tous les scalaires sont décodés champ par champ en little-endian.
use crate::{Block, Error, Result};
use sha2::{Digest, Sha256};

pub const HEADER: usize = 96;
pub const MAX_BLOCKS: u64 = 1 << 21;
const MAGIC: &[u8; 8] = b"NOVAP0FS";
const ANCHOR: u16 = 1;
const MANIFEST: u16 = 2;
pub(crate) const TREE: u16 = 3;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Ids {
    pub fs: [u8; 16],
    pub lv: [u8; 16],
}

impl Ids {
    fn validate(self) -> Result<()> {
        if self.fs == [0; 16] || self.lv == [0; 16] {
            return Err(Error::Corrupt("identité nulle"));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Ref {
    pub block: u64,
    pub birth: u64,
    pub hash: [u8; 32],
}

impl Ref {
    pub const ZERO: Self = Self {
        block: 0,
        birth: 0,
        hash: [0; 32],
    };

    pub fn to_bytes(self) -> [u8; 48] {
        let mut bytes = [0; 48];
        bytes[..8].copy_from_slice(&self.block.to_le_bytes());
        bytes[8..16].copy_from_slice(&self.birth.to_le_bytes());
        bytes[16..].copy_from_slice(&self.hash);
        bytes
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.len() != 48 {
            return Err(Error::Corrupt("taille de référence"));
        }
        let r = Self {
            block: le64(&bytes[..8]),
            birth: le64(&bytes[8..16]),
            hash: bytes[16..48].try_into().unwrap(),
        };
        if r != Self::ZERO && (r.block < 2 || r.block >= MAX_BLOCKS || r.birth == 0) {
            return Err(Error::Corrupt("référence hors profil"));
        }
        Ok(r)
    }

    pub fn validate(self, blocks: u64, generation: u64) -> Result<()> {
        if !(2..blocks).contains(&self.block) || self.birth == 0 || self.birth > generation {
            return Err(Error::Corrupt("référence hors volume ou génération"));
        }
        Ok(())
    }
}

// Ces lecteurs internes reçoivent exclusivement des tranches de largeur déjà contrôlée.
pub(crate) fn le64(bytes: &[u8]) -> u64 {
    u64::from_le_bytes(bytes.try_into().unwrap())
}
pub(crate) fn le16(bytes: &[u8]) -> u16 {
    u16::from_le_bytes(bytes.try_into().unwrap())
}

fn meta_hash(bytes: &Block) -> [u8; 32] {
    let mut sha = Sha256::new();
    sha.update(b"NovaFS.P0.meta\0");
    sha.update(&bytes[..64]);
    sha.update([0; 32]);
    sha.update(&bytes[96..]);
    sha.finalize().into()
}

pub fn data_hash(ids: Ids, block: u64, birth: u64, bytes: &Block) -> [u8; 32] {
    let mut sha = Sha256::new();
    sha.update(b"NovaFS.P0.data\0");
    sha.update(ids.fs);
    sha.update(ids.lv);
    sha.update(block.to_le_bytes());
    sha.update(birth.to_le_bytes());
    sha.update(bytes);
    sha.finalize().into()
}

pub(crate) fn finish_meta(
    mut bytes: Block,
    kind: u16,
    ids: Ids,
    block: u64,
    birth: u64,
) -> Result<(Ref, Block)> {
    ids.validate()?;
    if birth == 0 || block >= MAX_BLOCKS || ((kind == ANCHOR) != (block < 2)) {
        return Err(Error::Invalid("position ou naissance du bloc"));
    }
    bytes[..8].copy_from_slice(MAGIC);
    bytes[8..12].fill(0);
    bytes[12..14].copy_from_slice(&kind.to_le_bytes());
    bytes[14..16].copy_from_slice(&(HEADER as u16).to_le_bytes());
    bytes[16..32].copy_from_slice(&ids.fs);
    bytes[32..48].copy_from_slice(&ids.lv);
    bytes[48..56].copy_from_slice(&block.to_le_bytes());
    bytes[56..64].copy_from_slice(&birth.to_le_bytes());
    let hash = meta_hash(&bytes);
    bytes[64..96].copy_from_slice(&hash);
    Ok((Ref { block, birth, hash }, bytes))
}

fn envelope(bytes: &Block, kind: u16, slot: u64) -> Result<(Ids, Ref)> {
    if &bytes[..8] != MAGIC
        || bytes[8..12] != [0; 4]
        || le16(&bytes[12..14]) != kind
        || usize::from(le16(&bytes[14..16])) != HEADER
    {
        return Err(Error::Corrupt("signature, version ou type de métadonnée"));
    }
    let ids = Ids {
        fs: bytes[16..32].try_into().unwrap(),
        lv: bytes[32..48].try_into().unwrap(),
    };
    ids.validate()?;
    let r = Ref {
        block: le64(&bytes[48..56]),
        birth: le64(&bytes[56..64]),
        hash: bytes[64..96].try_into().unwrap(),
    };
    if slot >= MAX_BLOCKS || r.block != slot || r.birth == 0 || r.hash != meta_hash(bytes) {
        return Err(Error::Corrupt(
            "position, naissance ou checksum de métadonnée",
        ));
    }
    Ok((ids, r))
}

pub(crate) fn check_meta(bytes: &Block, kind: u16, ids: Ids, expected: Ref) -> Result<()> {
    expected.validate(MAX_BLOCKS, u64::MAX)?;
    if envelope(bytes, kind, expected.block)? != (ids, expected) {
        return Err(Error::Corrupt("filiation ou identité de métadonnée"));
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Anchor {
    pub ids: Ids,
    pub blocks: u64,
    pub manifest: Ref,
}

impl Anchor {
    fn validate(&self, slot: u64) -> Result<()> {
        self.ids.validate()?;
        if slot > 1
            || self.blocks > MAX_BLOCKS
            || self.blocks < 3
            || (self.manifest.birth > 1 && self.manifest.birth % 2 != slot)
        {
            return Err(Error::Corrupt("géométrie ou parité de l’ancre"));
        }
        self.manifest.validate(self.blocks, u64::MAX)
    }

    pub fn encode(&self, slot: u64) -> Result<Block> {
        self.validate(slot)?;
        let mut bytes = [0; 4096];
        bytes[96..100].copy_from_slice(&4096u32.to_le_bytes());
        bytes[104..112].copy_from_slice(&self.blocks.to_le_bytes());
        bytes[136..184].copy_from_slice(&self.manifest.to_bytes());
        Ok(finish_meta(bytes, ANCHOR, self.ids, slot, self.manifest.birth)?.1)
    }

    pub fn decode(bytes: &Block, slot: u64) -> Result<Self> {
        let (ids, own) = envelope(bytes, ANCHOR, slot)?;
        if bytes[96..100] != 4096u32.to_le_bytes()
            || bytes[100..104] != [0; 4]
            || bytes[112..136] != [0; 24]
            || bytes[184..].iter().any(|&b| b != 0)
        {
            return Err(Error::Corrupt(
                "géométrie, fonctionnalités ou suffixe de l’ancre",
            ));
        }
        let anchor = Self {
            ids,
            blocks: le64(&bytes[104..112]),
            manifest: Ref::from_bytes(&bytes[136..184])?,
        };
        if own.birth != anchor.manifest.birth {
            return Err(Error::Corrupt("naissance du manifeste"));
        }
        anchor.validate(slot)?;
        Ok(anchor)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Manifest {
    /// None désigne exclusivement le contenu historique.
    pub atime: Option<crate::time::Atime>,
    pub previous: u64,
    pub previous_hash: [u8; 32],
    pub root_oid: u64,
    pub next_oid: u64,
    pub blocks: u64,
    pub roots: [Ref; 5],
}

impl Manifest {
    fn validate(&self, block: u64, generation: u64) -> Result<()> {
        if generation == 0
            || self.previous != generation - 1
            || (generation == 1 && self.previous_hash != [0; 32])
            || self.blocks > MAX_BLOCKS
            || !(2..self.blocks).contains(&block)
            || self.root_oid == 0
            || self.next_oid <= self.root_oid
            || self.roots[0] == Ref::ZERO
        {
            return Err(Error::Corrupt(
                "géométrie, compteur ou filiation du manifeste",
            ));
        }
        for r in &self.roots {
            if *r != Ref::ZERO {
                r.validate(self.blocks, generation)?;
            }
        }
        Ok(())
    }

    pub fn encode(&self, ids: Ids, block: u64, generation: u64) -> Result<(Ref, Block)> {
        self.validate(block, generation)?;
        let mut bytes = [0; 4096];
        bytes[96..104].copy_from_slice(&self.previous.to_le_bytes());
        bytes[104..136].copy_from_slice(&self.previous_hash);
        bytes[136..144].copy_from_slice(&self.root_oid.to_le_bytes());
        bytes[144..152].copy_from_slice(&self.next_oid.to_le_bytes());
        bytes[152..160].copy_from_slice(&self.blocks.to_le_bytes());
        for (chunk, r) in bytes[160..400].chunks_exact_mut(48).zip(self.roots) {
            chunk.copy_from_slice(&r.to_bytes());
        }
        if let Some(policy) = self.atime {
            bytes[400..408].copy_from_slice(b"NP9TIME1");
            bytes[408] = policy as u8;
        }
        finish_meta(bytes, MANIFEST, ids, block, generation)
    }

    pub fn decode(bytes: &Block, ids: Ids, expected: Ref) -> Result<Self> {
        check_meta(bytes, MANIFEST, ids, expected)?;
        let atime = if &bytes[400..408] == b"NP9TIME1" {
            Some(crate::time::Atime::decode(bytes[408])?)
        } else if bytes[400..409] == [0; 9] {
            None
        } else {
            return Err(Error::Corrupt("profil de contenu inconnu"));
        };
        if bytes[409..].iter().any(|&b| b != 0) {
            return Err(Error::Corrupt("suffixe du manifeste"));
        }
        let mut roots = [Ref::ZERO; 5];
        for (r, chunk) in roots.iter_mut().zip(bytes[160..400].chunks_exact(48)) {
            *r = Ref::from_bytes(chunk)?;
        }
        let manifest = Self {
            atime,
            previous: le64(&bytes[96..104]),
            previous_hash: bytes[104..136].try_into().unwrap(),
            root_oid: le64(&bytes[136..144]),
            next_oid: le64(&bytes[144..152]),
            blocks: le64(&bytes[152..160]),
            roots,
        };
        manifest.validate(expected.block, expected.birth)?;
        Ok(manifest)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const IDS: Ids = Ids {
        fs: [1; 16],
        lv: [2; 16],
    };

    #[test]
    fn vecteur_externe_ancre_et_domaine() {
        let r = Ref {
            block: 2,
            birth: 1,
            hash: [3; 32],
        };
        let anchor = Anchor {
            ids: IDS,
            blocks: 1024,
            manifest: r,
        };
        let block = anchor.encode(0).unwrap();
        // Vecteur produit indépendamment avec hashlib + scalaires LE.
        assert_eq!(
            &block[64..96],
            &[
                0x46, 0xcf, 0x8, 0xe8, 0xdf, 0x36, 0xdd, 0xc6, 0xb1, 0x0, 0xaa, 0xe6, 0x5c, 0x92,
                0xa4, 0x3f, 0x9b, 0xf7, 0xf0, 0xc0, 0x4f, 0xea, 0xec, 0x86, 0x86, 0xa7, 0x19, 0xc6,
                0xa, 0xfe, 0x2b, 0x4
            ]
        );
        assert!(block[184..].iter().all(|&b| b == 0));
        assert_eq!(Anchor::decode(&block, 0).unwrap(), anchor);
        assert!(Anchor::decode(&block, 1).is_err());
        let hash = data_hash(IDS, 13, 3, &block);
        assert_ne!(hash, data_hash(IDS, 14, 3, &block));
        assert_ne!(hash, data_hash(IDS, 13, 4, &block));
        assert_ne!(
            hash,
            data_hash(
                Ids {
                    fs: IDS.lv,
                    lv: IDS.fs
                },
                13,
                3,
                &block
            )
        );
    }

    #[test]
    fn bit_requis_et_suffixes_malformes_resignes() {
        let a = Anchor {
            ids: IDS,
            blocks: 500,
            manifest: Ref {
                block: 24,
                birth: 2,
                hash: [1; 32],
            },
        };
        let original = a.encode(0).unwrap();
        for pos in [100, 112, 120, 128, 184, 4095] {
            let mut b = original;
            b[pos] = 1;
            let (_, b) = finish_meta(b, ANCHOR, IDS, 0, 2).unwrap();
            assert!(Anchor::decode(&b, 0).is_err(), "offset {pos}");
        }
        assert!(a.encode(1).is_err());
    }
}
