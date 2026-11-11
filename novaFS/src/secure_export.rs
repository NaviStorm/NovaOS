//! Sauvegarde autonome P7 de pages logiques. Les fichiers de travail ne contiennent
//! que des pages chiffrées. Une vérification intégrale précède l'import transactionnel.
use crate::crypto::{Entropy, Secret};
use crate::io::Scratch;
use crate::io::{Read, Write};
use crate::shared::SharedPool;
use crate::shared_export::{Coverage, ExportInfo, HEAD, coverage, exact, export_manifest};
use crate::{BLOCK_BYTES, Block, Device, Error, Result};
use alloc::vec::Vec;
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

const MAGIC: &[u8; 8] = b"NV7EXPRT";
const END: &[u8; 8] = b"NV7XEND!";
const PREFIX: usize = 32;
const MANIFEST: usize = HEAD + 16;
const WRAP_KEY: &[u8] = b"NovaFS/archive/v1/enveloppe";
const MANIFEST_KEY: &[u8] = b"NovaFS/archive/v1/manifeste";
const DATA_KEY: &[u8] = b"NovaFS/archive/v1/pages";
const FOOT_KEY: &[u8] = b"NovaFS/archive/v1/cloture";

#[derive(Clone, Copy)]
struct Seal {
    nonce: [u8; 24],
    tag: [u8; 16],
}
impl Seal {
    fn encrypt(key: &Secret, rng: &mut impl Entropy, aad: &[u8], data: &mut [u8]) -> Result<Self> {
        let (nonce, tag) = key.seal(rng, aad, data)?;
        Ok(Self { nonce, tag })
    }

    fn read(input: &mut impl Read) -> Result<Self> {
        let mut seal = Self {
            nonce: [0; 24],
            tag: [0; 16],
        };
        exact(input, &mut seal.nonce)?;
        exact(input, &mut seal.tag)?;
        Ok(seal)
    }

    fn write(&self, output: &mut impl Write, data: &[u8]) -> Result<()> {
        output.write_all(&self.nonce)?;
        output.write_all(&self.tag)?;
        output.write_all(data)?;
        Ok(())
    }

    fn hash(&self, hash: &mut Sha256, data: &[u8]) {
        hash.update(self.nonce);
        hash.update(self.tag);
        hash.update(data);
    }
}

fn prefix(id: [u8; 16]) -> [u8; PREFIX] {
    let mut b = [0u8; PREFIX];
    b[..8].copy_from_slice(MAGIC);
    b[8..10].copy_from_slice(&1u16.to_le_bytes());
    b[10..12].copy_from_slice(&(PREFIX as u16).to_le_bytes());
    b[12..16].copy_from_slice(&1u32.to_le_bytes());
    b[16..].copy_from_slice(&id);
    b
}

fn page_aad(header: &[u8; 32], ordinal: u64, lba: u64) -> [u8; 48] {
    let mut aad = [0; 48];
    aad[..32].copy_from_slice(header);
    aad[32..40].copy_from_slice(&ordinal.to_le_bytes());
    aad[40..48].copy_from_slice(&lba.to_le_bytes());
    aad
}

fn footer(hash: Sha256, count: u64) -> [u8; 40] {
    let mut message = [0u8; 40];
    message[..32].copy_from_slice(&hash.chain_update(END).finalize());
    message[32..].copy_from_slice(&count.to_le_bytes());
    message
}

/// Le domaine source provient de la façade déverrouillée. La clé de sauvegarde
/// est indépendante du domaine. Un nouveau secret de transport protège chaque flux.
#[cfg(test)]
pub(crate) fn export_view<D: Device>(
    pool: &SharedPool<D>,
    id: u64,
    sector: u32,
    source_domain: [u8; 16],
    backup: &Secret,
    rng: &mut impl Entropy,
    output: impl Write,
) -> Result<ExportInfo> {
    export_with_parent(
        ExportProfile::P7,
        pool,
        id,
        sector,
        source_domain,
        backup,
        rng,
        output,
    )
}
pub(crate) enum ExportProfile {
    P7,
    P9 { parent: Option<([u8; 16], u64)> },
}
pub(crate) fn export_with_parent<D: Device>(
    profile: ExportProfile,
    pool: &SharedPool<D>,
    id: u64,
    sector: u32,
    source_domain: [u8; 16],
    backup: &Secret,
    rng: &mut impl Entropy,
    mut output: impl Write,
) -> Result<ExportInfo> {
    if source_domain == [0; 16] {
        return Err(Error::Invalid("domaine source nul"));
    }
    let device = pool.read_device(id)?;
    let (info, seen) = export_manifest(pool, id, sector, &device)?;
    let mut manifest = Zeroizing::new(vec![
        0u8;
        MANIFEST
            + if matches!(profile, ExportProfile::P9 { .. }) {
                24
            } else {
                0
            }
    ]);
    manifest[..HEAD].copy_from_slice(&info.encode()?);
    manifest[HEAD..MANIFEST].copy_from_slice(&source_domain);
    if let ExportProfile::P9 {
        parent: Some((pool, parent)),
    } = profile
    {
        manifest[MANIFEST..MANIFEST + 16].copy_from_slice(&pool);
        manifest[MANIFEST + 16..].copy_from_slice(&parent.to_le_bytes());
    }

    let mut archive_id = [0u8; 16];
    rng.fill(&mut archive_id)?;
    let mut header = prefix(archive_id);
    if matches!(profile, ExportProfile::P9 { .. }) {
        header[..8].copy_from_slice(b"NV9EXPRT");
        header[8..10].copy_from_slice(&2u16.to_le_bytes());
    }
    let transport = Secret::generate(rng)?;
    let wrapping = backup.derive(&archive_id, WRAP_KEY)?;
    let mut wrapped = Zeroizing::new(*transport.as_bytes());
    let envelope = Seal::encrypt(&wrapping, rng, &header, &mut wrapped[..])?;
    let mut hash = Sha256::new();
    hash.update(header);
    envelope.hash(&mut hash, &wrapped[..]);
    let manifest_key = transport.derive(&archive_id, MANIFEST_KEY)?;
    let seal = Seal::encrypt(
        &manifest_key,
        rng,
        &hash.clone().finalize(),
        &mut manifest[..],
    )?;
    seal.hash(&mut hash, &manifest[..]);
    let header_digest = hash.clone().finalize().into();
    output.write_all(&header)?;
    envelope.write(&mut output, &wrapped[..])?;
    seal.write(&mut output, &manifest[..])?;

    let data_key = transport.derive(&archive_id, DATA_KEY)?;
    let mut ordinal = 0;
    for lba in 0..info.blocks {
        if seen[lba as usize / 64] & (1 << (lba % 64)) == 0 {
            continue;
        }
        let mut page = Zeroizing::new(device.read(lba)?);
        let aad = page_aad(&header_digest, ordinal, lba);
        let seal = Seal::encrypt(&data_key, rng, &aad, &mut page[..])?;
        output.write_all(&lba.to_le_bytes())?;
        seal.write(&mut output, &page[..])?;
        hash.update(lba.to_le_bytes());
        seal.hash(&mut hash, &page[..]);
        ordinal += 1;
    }
    let foot_key = transport.derive(&archive_id, FOOT_KEY)?;
    output.write_all(END)?;
    output.write_all(&foot_key.mac(&[&footer(hash, ordinal)]))?;
    output.flush()?;
    Ok(info)
}

#[derive(Clone, Copy)]
struct Row {
    ordinal: u64,
    seal: Seal,
}

struct ArchiveImage {
    ciphertexts: Scratch,
    rows: Vec<Option<Row>>,
    key: Secret,
    header: [u8; 32],
}
impl Device for ArchiveImage {
    fn blocks(&self) -> u64 {
        self.ciphertexts.blocks()
    }

    fn read(&self, lba: u64) -> Result<Block> {
        if lba >= self.blocks() {
            return Err(Error::Corrupt("lecture hors du volume exporté"));
        }
        let Some(row) = self.rows[lba as usize] else {
            return Ok([0; BLOCK_BYTES]);
        };
        let mut page = Zeroizing::new(self.ciphertexts.read(lba)?);
        let aad = page_aad(&self.header, row.ordinal, lba);
        self.key
            .open(&row.seal.nonce, &row.seal.tag, &aad, &mut page[..])?;
        Ok(*page)
    }

    fn write(&mut self, _: u64, _: &Block) -> Result<()> {
        Err(Error::ReadOnly)
    }

    fn flush(&mut self) -> Result<()> {
        Err(Error::ReadOnly)
    }
}

/// Domaine et éventuelle filiation engagés par le manifeste authentifié.
pub(crate) struct ArchiveOrigin {
    pub domain: [u8; 16],
    pub parent: Option<([u8; 16], u64)>,
    pub p9: bool,
}
#[cfg(test)]
pub(crate) fn verified_image(
    input: impl Read,
    backup: &Secret,
) -> Result<(ExportInfo, impl Device)> {
    let (info, _, image) = verified_with_origin(input, backup)?;
    Ok((info, image))
}
pub(crate) fn verified_with_origin(
    mut input: impl Read,
    backup: &Secret,
) -> Result<(ExportInfo, ArchiveOrigin, impl Device)> {
    let mut header = [0; PREFIX];
    exact(&mut input, &mut header)?;
    let archive_id = header[16..].try_into().unwrap();
    let p9 = &header[..8] == b"NV9EXPRT";
    let mut expected = prefix(archive_id);
    if p9 {
        expected[..8].copy_from_slice(b"NV9EXPRT");
        expected[8..10].copy_from_slice(&2u16.to_le_bytes());
    }
    if header != expected {
        return Err(Error::Corrupt(
            "signature, version ou algorithme d’archive P7",
        ));
    }
    let envelope = Seal::read(&mut input)?;
    let mut wrapped = Zeroizing::new([0; 32]);
    exact(&mut input, &mut wrapped[..])?;
    let mut hash = Sha256::new();
    hash.update(header);
    envelope.hash(&mut hash, &wrapped[..]);
    let wrapping = backup.derive(&archive_id, WRAP_KEY)?;
    wrapping.open(&envelope.nonce, &envelope.tag, &header, &mut wrapped[..])?;
    let transport = Secret::from_bytes(*wrapped);
    drop(wrapped);

    let seal = Seal::read(&mut input)?;
    let mut manifest = Zeroizing::new(vec![0u8; MANIFEST + if p9 { 24 } else { 0 }]);
    exact(&mut input, &mut manifest[..])?;
    let aad = hash.clone().finalize();
    seal.hash(&mut hash, &manifest[..]);
    let manifest_key = transport.derive(&archive_id, MANIFEST_KEY)?;
    manifest_key.open(&seal.nonce, &seal.tag, &aad, &mut manifest[..])?;
    let info = ExportInfo::decode((&manifest[..HEAD]).try_into().unwrap())?;
    let domain = manifest[HEAD..MANIFEST].try_into().unwrap();
    let parent = if p9 {
        let realm: [u8; 16] = manifest[MANIFEST..MANIFEST + 16].try_into().unwrap();
        let parent = u64::from_le_bytes(manifest[MANIFEST + 16..].try_into().unwrap());
        if (realm == [0; 16]) != (parent == 0) || (realm == info.pool && parent >= info.id) {
            return Err(Error::Corrupt("parent d'archive P9"));
        }
        (parent != 0).then_some((realm, parent))
    } else {
        None
    };
    if domain == [0; 16] {
        return Err(Error::Corrupt("domaine du manifeste P7"));
    }
    let mut rows = Vec::new();
    rows.try_reserve_exact(info.blocks as usize)
        .map_err(|_| Error::NoMemory)?;
    rows.resize(info.blocks as usize, None);
    let mut image = ArchiveImage {
        ciphertexts: Scratch::create(info.blocks, info.sector)?,
        rows,
        key: transport.derive(&archive_id, DATA_KEY)?,
        header: hash.clone().finalize().into(),
    };

    let mut previous = None;
    let mut seen = Coverage::default();
    let mut ciphertext = [0; BLOCK_BYTES];
    for ordinal in 0..info.pages {
        let mut address = [0u8; 8];
        exact(&mut input, &mut address)?;
        let lba = u64::from_le_bytes(address);
        if lba >= info.blocks || previous.is_some_and(|p| p >= lba) {
            return Err(Error::Corrupt("ordre ou portée des pages d’archive P7"));
        }
        let seal = Seal::read(&mut input)?;
        exact(&mut input, &mut ciphertext)?;
        hash.update(address);
        seal.hash(&mut hash, &ciphertext);
        image.ciphertexts.write(lba, &ciphertext)?;
        image.rows[lba as usize] = Some(Row { ordinal, seal });
        seen[lba as usize / 64] |= 1 << (lba % 64);
        previous = Some(lba);
    }
    let mut end = [0; 8];
    let mut mac = [0; 32];
    exact(&mut input, &mut end)?;
    exact(&mut input, &mut mac)?;
    if &end != END || input.read(&mut ciphertext[..1])? != 0 {
        return Err(Error::Corrupt("fin ou longueur de l’archive P7"));
    }
    let foot_key = transport.derive(&archive_id, FOOT_KEY)?;
    foot_key.verify_mac(&[&footer(hash, info.pages)], &mac)?;
    if coverage(&image, info.ids)? != seen {
        return Err(Error::Corrupt("ensemble des pages d’archive P7"));
    }
    Ok((info, ArchiveOrigin { domain, parent, p9 }, image))
}

/// Vérification authentifiée en lecture seule, après clôture du flux.
pub fn check_export(input: impl Read, backup: &Secret) -> Result<ExportInfo> {
    Ok(check_export_profile(input, backup)?.0)
}

/// Le profil annoncé provient du préfixe couvert par l'AEAD du transport.
pub fn check_export_profile(input: impl Read, backup: &Secret) -> Result<(ExportInfo, u16)> {
    let (info, origin, _) = verified_with_origin(input, backup)?;
    Ok((info, if origin.p9 { 9 } else { 7 }))
}

#[cfg(test)]
mod tests;
