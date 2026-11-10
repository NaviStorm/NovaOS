//! Export autonome d’une vue partagée. Le flux contient les pages nécessaires
//! aux deux générations internes vérifiées. Les blocs libres restent implicites.
use crate::device::{FileImage, Geometry};
use crate::shared::{ObjectKind, SharedPool};
use crate::{BLOCK_BYTES, Block, Device, Engine, Error, Ids, Mode, Result};
use sha2::{Digest, Sha256};
use std::cell::RefCell;
use std::io::{ErrorKind, Read, Write};

const MAGIC: &[u8; 8] = b"NV5EXPRT";
const END: &[u8; 8] = b"NV5XEND!";
const DOMAIN: &[u8] = b"NovaFS/export/complet/v1\0";
const HEAD: usize = 160;
const MAX_VOLUME: u64 = 4 * crate::pool::GRAIN;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportInfo {
    pub pool: [u8; 16],
    pub id: u64,
    pub owner: [u8; 16],
    pub ids: Ids,
    pub kind: ObjectKind,
    pub source_root: [u8; 32],
    pub blocks: u64,
    pub sector: u32,
    pub pages: u64,
}

impl ExportInfo {
    fn encode(&self) -> Result<[u8; HEAD]> {
        let tag: u64 = match self.kind {
            ObjectKind::Snapshot => 1,
            ObjectKind::Clone => 2,
            _ => return Err(Error::Invalid("export réservé aux snapshots et clones")),
        };
        Geometry::new(self.blocks, self.sector)?;
        if !(crate::pool::GRAIN..=MAX_VOLUME).contains(&self.blocks)
            || !self.blocks.is_multiple_of(crate::pool::GRAIN)
            || self.pages < 2
            || self.pages > self.blocks
            || self.pool == [0; 16]
            || self.id == 0
            || self.owner == [0; 16]
            || self.ids.lv == [0; 16]
            || self.ids.fs == [0; 16]
            || self.source_root == [0; 32]
        {
            return Err(Error::Corrupt("identités ou bornes de l’export"));
        }
        let mut buf = [0; HEAD];
        buf[..8].copy_from_slice(MAGIC);
        buf[8..10].copy_from_slice(&1u16.to_le_bytes());
        buf[10..12].copy_from_slice(&(HEAD as u16).to_le_bytes());
        buf[16..20].copy_from_slice(&(BLOCK_BYTES as u32).to_le_bytes());
        buf[20..24].copy_from_slice(&self.sector.to_le_bytes());
        buf[24..32].copy_from_slice(&self.blocks.to_le_bytes());
        buf[32..48].copy_from_slice(&self.pool);
        buf[48..56].copy_from_slice(&self.id.to_le_bytes());
        buf[56..64].copy_from_slice(&tag.to_le_bytes());
        buf[64..80].copy_from_slice(&self.owner);
        buf[80..96].copy_from_slice(&self.ids.fs);
        buf[96..112].copy_from_slice(&self.ids.lv);
        buf[112..144].copy_from_slice(&self.source_root);
        buf[144..152].copy_from_slice(&self.pages.to_le_bytes());
        Ok(buf)
    }
    fn decode(b: &[u8; HEAD]) -> Result<Self> {
        if b[..8] != MAGIC[..] || u16::from_le_bytes(b[8..10].try_into().unwrap()) != 1 {
            return Err(Error::Corrupt("signature ou version d’export inconnue"));
        }
        let u64_at = |offset| u64::from_le_bytes(b[offset..offset + 8].try_into().unwrap());
        let info = Self {
            sector: u32::from_le_bytes(b[20..24].try_into().unwrap()),
            blocks: u64_at(24),
            pool: b[32..48].try_into().unwrap(),
            id: u64_at(48),
            kind: match u64_at(56) {
                1 => ObjectKind::Snapshot,
                2 => ObjectKind::Clone,
                _ => return Err(Error::Corrupt("type de vue exportée")),
            },
            owner: b[64..80].try_into().unwrap(),
            ids: Ids {
                fs: b[80..96].try_into().unwrap(),
                lv: b[96..112].try_into().unwrap(),
            },
            source_root: b[112..144].try_into().unwrap(),
            pages: u64_at(144),
        };
        if info.encode()? != *b {
            return Err(Error::Corrupt("en-tête ou options d’export"));
        }
        Ok(info)
    }
}

// Quatre grains P3 par domaine. Le bitmap de tout le LV occupe au plus 128 octets.
type Coverage = [u64; MAX_VOLUME.div_ceil(64) as usize];
struct Traced<'a, D> {
    device: &'a D,
    seen: RefCell<Coverage>,
}
impl<D: Device> Device for Traced<'_, D> {
    fn blocks(&self) -> u64 {
        self.device.blocks()
    }
    fn read(&self, b: u64) -> Result<Block> {
        if b >= self.blocks() || b >= MAX_VOLUME {
            return Err(Error::Corrupt("bloc d’export hors LV"));
        }
        let data = self.device.read(b)?;
        self.seen.borrow_mut()[b as usize / 64] |= 1 << (b % 64);
        Ok(data)
    }
    fn write(&mut self, _: u64, _: &Block) -> Result<()> {
        Err(Error::ReadOnly)
    }
    fn flush(&mut self) -> Result<()> {
        Err(Error::ReadOnly)
    }
}
fn coverage(device: &impl Device, ids: Ids) -> Result<Coverage> {
    if device.blocks() > MAX_VOLUME {
        return Err(Error::Limit("taille du volume exporté"));
    }
    let mut audit = Traced {
        device,
        seen: RefCell::new([0; MAX_VOLUME.div_ceil(64) as usize]),
    };
    // Le montage RO P2 parcourt les deux générations, y compris les données des extents.
    drop(Engine::mount(&mut audit, ids, Mode::ReadOnly)?);
    Ok(audit.seen.into_inner())
}

/// Appeler sur un pool stable, avec la géométrie du support source. La fin du flux
/// et son empreinte sont obligatoires, même pour un export sans données de fichier.
pub fn export_view<D: Device>(
    pool: &SharedPool<D>,
    id: u64,
    sector: u32,
    mut output: impl Write,
) -> Result<ExportInfo> {
    let object = pool.info(id)?;
    let device = pool.read_device(id)?;
    let seen = coverage(&device, object.ids)?;
    let info = ExportInfo {
        pool: pool.pool_id(),
        id,
        owner: object.owner,
        ids: object.ids,
        kind: object.kind,
        source_root: pool.receipt(id)?.root,
        blocks: device.blocks(),
        sector,
        pages: seen.iter().map(|w| u64::from(w.count_ones())).sum(),
    };
    let header = info.encode()?;
    let mut sha = Sha256::new();
    sha.update(DOMAIN);
    sha.update(header);
    output.write_all(&header)?;
    for b in 0..info.blocks {
        if seen[b as usize / 64] & (1 << (b % 64)) == 0 {
            continue;
        }
        let data = device.read(b)?;
        let addr = b.to_le_bytes();
        sha.update(addr);
        sha.update(data);
        output.write_all(&addr)?;
        output.write_all(&data)?;
    }
    sha.update(END);
    output.write_all(END)?;
    output.write_all(&sha.finalize())?;
    output.flush()?;
    Ok(info)
}

fn exact(input: &mut impl Read, buf: &mut [u8]) -> Result<()> {
    match input.read_exact(buf) {
        Err(e) if e.kind() == ErrorKind::UnexpectedEof => Err(Error::Corrupt("export interrompu")),
        res => Ok(res?),
    }
}

fn materialize(mut input: impl Read) -> Result<(ExportInfo, FileImage)> {
    let mut header = [0; HEAD];
    exact(&mut input, &mut header)?;
    let info = ExportInfo::decode(&header)?;
    let mut image = FileImage::create(info.blocks, info.sector)?;
    let mut sha = Sha256::new();
    sha.update(DOMAIN);
    sha.update(header);
    let mut seen: Coverage = [0; MAX_VOLUME.div_ceil(64) as usize];
    let mut previous = None;
    let mut data = [0; BLOCK_BYTES];
    for _ in 0..info.pages {
        let mut addr = [0; 8];
        exact(&mut input, &mut addr)?;
        let b = u64::from_le_bytes(addr);
        if b >= info.blocks || previous.is_some_and(|p| p >= b) {
            return Err(Error::Corrupt("ordre ou portée des blocs exportés"));
        }
        exact(&mut input, &mut data)?;
        sha.update(addr);
        sha.update(data);
        image.write(b, &data)?;
        seen[b as usize / 64] |= 1 << (b % 64);
        previous = Some(b);
    }
    let mut end = [0; 8];
    let mut digest = [0; 32];
    exact(&mut input, &mut end)?;
    exact(&mut input, &mut digest)?;
    sha.update(end);
    if &end != END || sha.finalize()[..] != digest || input.read(&mut data[..1])? != 0 {
        return Err(Error::Corrupt("fin, empreinte ou longueur de l’export"));
    }
    if coverage(&image, info.ids)? != seen {
        return Err(Error::Corrupt("ensemble des blocs exportés"));
    }
    Ok((info, image))
}

/// Examine un flux sans écrire sur sa source. Le support de travail est anonyme.
pub fn check_export(input: impl Read) -> Result<ExportInfo> {
    Ok(materialize(input)?.0)
}

/// Toute erreur de décodage ou d’intégrité précède la première publication de destination.
pub fn restore_view<D: Device>(
    pool: &mut SharedPool<D>,
    input: impl Read,
    owner: [u8; 16],
) -> Result<u64> {
    let (info, image) = materialize(input)?;
    pool.import_image(owner, info.ids, &image, info.kind)
}
