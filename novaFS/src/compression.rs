//! Unité P6 d'un bloc physique. Le répertoire adresse au plus 16 flux LZ4
//! indépendants ; le décodage d'un LBA n'engage qu'une page de sortie.
use crate::{BLOCK_BYTES, Block, Error, Result};

const MAGIC: &[u8; 8] = b"NP6LZ4PK";
const VERSION: u16 = 1;
const LZ4: u8 = 1;
const MAX_PAGES: usize = 16;
const DIRECTORY: usize = 32;
const PAYLOAD: usize = DIRECTORY + MAX_PAGES * 4;
const CAPACITY: usize = BLOCK_BYTES - PAYLOAD;
const INVALID: Error = Error::Corrupt("flux LZ4 P6 invalide");

fn word(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([bytes[at], bytes[at + 1]])
}

fn entries(bytes: &Block, count: usize) -> impl Iterator<Item = (u16, usize)> + '_ {
    bytes[DIRECTORY..DIRECTORY + 4 * count]
        .chunks_exact(4)
        .map(|e| (word(e, 0), usize::from(word(e, 2))))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Push {
    Added,
    /// La page peut être réessayée dans une unité neuve.
    Full,
    /// Une page nulle ou incompressible suit le chemin brut ordinaire.
    Raw,
}

pub(crate) struct Packer {
    block: Block,
    used: usize,
    count: usize,
}

impl Drop for Packer {
    fn drop(&mut self) {
        zeroize::Zeroize::zeroize(&mut self.block);
    }
}

impl Default for Packer {
    fn default() -> Self {
        Self {
            block: [0; BLOCK_BYTES],
            used: 0,
            count: 0,
        }
    }
}

impl Packer {
    pub(crate) fn lbas(&self) -> impl Iterator<Item = u16> + '_ {
        entries(&self.block, self.count).map(|(lba, _)| lba)
    }

    pub(crate) fn push(&mut self, lba: u16, data: &Block) -> Result<Push> {
        if lba >= 1024 {
            return Err(Error::Invalid("LBA de compression hors volume"));
        }
        if data.iter().all(|&b| b == 0) {
            return Ok(Push::Raw);
        }
        if let Some(prev) = self.lbas().last() {
            if lba <= prev {
                return Err(Error::Invalid("LBA de compression non croissant"));
            }
            if lba / 64 != prev / 64 || self.count == MAX_PAGES {
                return Ok(Push::Full);
            }
        }
        let mut scratch = [0; CAPACITY];
        let Some(len) = encode_block(data, &mut scratch) else {
            return Ok(Push::Raw);
        };
        if len > CAPACITY - self.used {
            return Ok(Push::Full);
        }
        let at = DIRECTORY + self.count * 4;
        self.block[at..at + 2].copy_from_slice(&lba.to_le_bytes());
        self.block[at + 2..at + 4].copy_from_slice(&(len as u16).to_le_bytes());
        let at = PAYLOAD + self.used;
        self.block[at..at + len].copy_from_slice(&scratch[..len]);
        self.used += len;
        self.count += 1;
        Ok(Push::Added)
    }

    pub(crate) fn finish(mut self) -> Option<Block> {
        if self.count < 2 {
            return None;
        }
        self.block[..8].copy_from_slice(MAGIC);
        self.block[8..10].copy_from_slice(&VERSION.to_le_bytes());
        self.block[10] = LZ4;
        self.block[11] = self.count as u8;
        self.block[12..16].copy_from_slice(&((self.count * BLOCK_BYTES) as u32).to_le_bytes());
        self.block[16..18].copy_from_slice(&(self.used as u16).to_le_bytes());
        self.block[18..20].copy_from_slice(&(BLOCK_BYTES as u16).to_le_bytes());
        Some(self.block)
    }
}

pub(crate) struct Unit<'a> {
    page: &'a Block,
    count: usize,
    used: usize,
}

/// Vérifie toutes les longueurs avant de confier une tranche au décodeur.
/// La référence physique, avec domaine et adresse, est contrôlée par le graphe.
pub(crate) fn inspect(page: &Block) -> Result<Unit<'_>> {
    if &page[..8] != MAGIC || word(page, 8) != VERSION || page[10] != LZ4 {
        return Err(Error::Corrupt(
            "signature, version ou algorithme de compression P6",
        ));
    }
    let count = usize::from(page[11]);
    let logical = u32::from_le_bytes(page[12..16].try_into().expect("quatre octets"));
    let used = usize::from(word(page, 16));
    if !(2..=MAX_PAGES).contains(&count)
        || logical != (count * BLOCK_BYTES) as u32
        || used > CAPACITY
        || usize::from(word(page, 18)) != BLOCK_BYTES
        || page[20..DIRECTORY].iter().any(|&b| b != 0)
        || page[DIRECTORY + 4 * count..PAYLOAD].iter().any(|&b| b != 0)
        || page[PAYLOAD + used..].iter().any(|&b| b != 0)
    {
        return Err(Error::Corrupt("géométrie ou remplissage d'une unité P6"));
    }
    let mut previous = None;
    let mut remaining = used;
    for (lba, size) in entries(page, count) {
        if lba >= 1024
            || previous.is_some_and(|p| p >= lba || p / 64 != lba / 64)
            || size == 0
            || size > remaining
        {
            return Err(Error::Corrupt("répertoire d'une unité P6"));
        }
        previous = Some(lba);
        remaining -= size;
    }
    if remaining != 0 {
        return Err(Error::Corrupt("octets sans flux dans une unité P6"));
    }
    Ok(Unit { page, count, used })
}

impl Unit<'_> {
    pub(crate) fn lbas(&self) -> impl Iterator<Item = u16> + '_ {
        entries(self.page, self.count).map(|(lba, _)| lba)
    }

    pub(crate) fn encoded_bytes(&self) -> usize {
        self.used
    }

    fn streams(&self) -> impl Iterator<Item = (u16, &[u8])> {
        entries(self.page, self.count).scan(PAYLOAD, |at, (lba, len)| {
            let start = *at;
            *at += len;
            Some((lba, &self.page[start..*at]))
        })
    }

    pub(crate) fn page(&self, lba: u16) -> Result<Block> {
        let (_, src) = self
            .streams()
            .find(|&(index, _)| index == lba)
            .ok_or(Error::Corrupt("page absente de l'unité P6"))?;
        decode_block(src)
    }

    pub(crate) fn verify_all(&self) -> Result<()> {
        for (_, stream) in self.streams() {
            decode_block(stream)?;
        }
        Ok(())
    }
}

// LZ4 block sans dictionnaire. La table de recherche locale de 2 Kio ne
// contient que des indices dans le bloc logique courant.
fn encode_block(src: &Block, dst: &mut [u8]) -> Option<usize> {
    #[cfg(all(feature = "latency-probes", not(target_os = "none")))]
    let _probe = crate::latency_probes::enter(crate::latency_probes::Phase::Compression);
    let mut table = [u16::MAX; 1024];
    let mut written = 0;
    let mut anchor = 0;
    let mut pos = 0;
    while pos <= BLOCK_BYTES - 12 {
        let value = u32::from_le_bytes(src[pos..pos + 4].try_into().ok()?);
        let slot = (value.wrapping_mul(2654435761) >> 22) as usize;
        let reference = usize::from(table[slot]);
        table[slot] = pos as u16;
        if reference >= pos || src[reference..reference + 4] != src[pos..pos + 4] {
            pos += 1;
            continue;
        }
        let mut len = 4;
        while pos + len < BLOCK_BYTES - 5 && src[reference + len] == src[pos + len] {
            len += 1;
        }
        written += sequence(
            &src[anchor..pos],
            Some((pos - reference, len)),
            &mut dst[written..],
        )?;
        pos += len;
        anchor = pos;
    }
    written += sequence(&src[anchor..], None, &mut dst[written..])?;
    Some(written)
}

fn extension(len: usize, dst: &mut [u8]) -> Option<usize> {
    if len < 15 {
        return Some(0);
    }
    let extra = len - 15;
    let bytes = dst.get_mut(..extra / 255 + 1)?;
    bytes.fill(255);
    *bytes.last_mut()? = (extra % 255) as u8;
    Some(bytes.len())
}

fn sequence(literals: &[u8], matched: Option<(usize, usize)>, dst: &mut [u8]) -> Option<usize> {
    let match_code = matched.map_or(0, |(_, size)| size - 4);
    *dst.first_mut()? = (literals.len().min(15) << 4 | match_code.min(15)) as u8;
    let mut pos = 1 + extension(literals.len(), &mut dst[1..])?;
    dst.get_mut(pos..pos + literals.len())?
        .copy_from_slice(literals);
    pos += literals.len();
    if let Some((distance, _)) = matched {
        dst.get_mut(pos..pos + 2)?
            .copy_from_slice(&(distance as u16).to_le_bytes());
        pos += 2;
        pos += extension(match_code, &mut dst[pos..])?;
    }
    Some(pos)
}

fn length(input: &mut &[u8], nibble: u8) -> Result<usize> {
    let mut n = usize::from(nibble);
    if nibble == 15 {
        loop {
            let (&byte, rest) = input.split_first().ok_or(INVALID)?;
            *input = rest;
            n += usize::from(byte);
            if n > BLOCK_BYTES {
                return Err(INVALID);
            }
            if byte < 255 {
                break;
            }
        }
    }
    Ok(n)
}

/// Le décodeur ne lit aucun octet hors du flux et initialise strictement
/// les 4 Kio promis. Les copies chevauchantes ne lisent que le préfixe produit.
fn decode_block(mut src: &[u8]) -> Result<Block> {
    #[cfg(all(feature = "latency-probes", not(target_os = "none")))]
    let _probe = crate::latency_probes::enter(crate::latency_probes::Phase::Decompression);
    let mut output = [0; BLOCK_BYTES];
    let mut pos = 0;
    loop {
        let (&token, tail) = src.split_first().ok_or(INVALID)?;
        src = tail;
        let literals = length(&mut src, token >> 4)?;
        let (bytes, rest) = src.split_at_checked(literals).ok_or(INVALID)?;
        let after = pos + literals;
        output
            .get_mut(pos..after)
            .ok_or(INVALID)?
            .copy_from_slice(bytes);
        pos = after;
        src = rest;
        if src.is_empty() {
            if pos != BLOCK_BYTES || literals < 5 {
                return Err(INVALID);
            }
            if output.iter().all(|&b| b == 0) {
                return Err(Error::Corrupt("page nulle compressée P6"));
            }
            return Ok(output);
        }
        let (offset, rest) = src.split_at_checked(2).ok_or(INVALID)?;
        let distance = usize::from(word(offset, 0));
        if distance == 0 || distance > pos || pos > BLOCK_BYTES - 12 {
            return Err(INVALID);
        }
        src = rest;
        let matched = length(&mut src, token & 15)? + 4;
        if matched > BLOCK_BYTES - pos - 5 {
            return Err(INVALID);
        }
        let source = pos - distance;
        let end = pos + matched;
        while pos != end {
            let copy = (end - pos).min(pos - source);
            output.copy_within(source..source + copy, pos);
            pos += copy;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // LZ4_compress_default 1.11.0 appliqué à « Nouvelle » répété 512 fois.
    const REFERENCE: &[u8] = &[
        143, 78, 111, 117, 118, 101, 108, 108, 101, 8, 0, 255, 255, 255, 255, 255, 255, 255, 255,
        255, 255, 255, 255, 255, 255, 255, 239, 80, 118, 101, 108, 108, 101,
    ];

    #[test]
    fn flux_reference_lz4_et_expansion_bornee() {
        let mut input = [0; BLOCK_BYTES];
        for b in input.chunks_exact_mut(8) {
            b.copy_from_slice(b"Nouvelle");
        }
        assert_eq!(decode_block(REFERENCE).unwrap(), input);
        let mut encoded = [0; CAPACITY];
        let n = encode_block(&input, &mut encoded).unwrap();
        assert_eq!(&encoded[..n], REFERENCE);
        for cut in 0..REFERENCE.len() {
            assert!(
                decode_block(&REFERENCE[..cut]).is_err(),
                "troncature à {cut}"
            );
        }
        for (i, byte) in [(9, 0), (9, 9), (26, 240), (26, 238), (27, 0)] {
            let mut bad = REFERENCE.to_vec();
            bad[i] = byte;
            assert!(decode_block(&bad).is_err());
        }
        for bad in [
            &[0x10, 65, 0, 0][..],
            &[0x0f, 1, 0],
            &[0xff; 4096],
            &[0; 4096],
        ] {
            assert!(decode_block(bad).is_err());
        }
        let mut garbage = REFERENCE.to_vec();
        garbage.extend_from_slice(&[0; 4]);
        assert!(decode_block(&garbage).is_err());
        // Le décodeur LZ4 ignore le demi-octet de match inutilisé du dernier token.
        let mut final_token = REFERENCE.to_vec();
        final_token[27] |= 15;
        assert_eq!(decode_block(&final_token).unwrap(), input);
    }

    #[test]
    fn un_bloc_physique_jusqua_seize_pages() -> Result<()> {
        let mut p = Packer::default();
        assert_eq!(p.push(0, &[0; 4096])?, Push::Raw);
        for lba in 0..16u16 {
            assert_eq!(p.push(lba, &[1 + lba as u8; 4096])?, Push::Added);
        }
        assert_eq!(p.push(17, &[1; 4096])?, Push::Full);
        assert_eq!(p.push(64, &[1; 4096])?, Push::Full);
        assert!(p.push(10, &[1; 4096]).is_err());
        assert!(p.push(1024, &[1; 4096]).is_err());
        let data = p.finish().unwrap();
        let u = inspect(&data)?;
        assert_eq!(u.lbas().collect::<Vec<_>>(), (0..16).collect::<Vec<_>>());
        assert!(u.encoded_bytes() < 500);
        u.verify_all()?;
        for lba in 0..16 {
            assert_eq!(u.page(lba)?, [1 + lba as u8; 4096]);
        }
        assert!(u.page(20).is_err());
        Ok(())
    }

    fn pseudo_alea() -> Block {
        let mut x = 0x72fbb4827395478du64;
        let mut page = [0; BLOCK_BYTES];
        for chunk in page.chunks_exact_mut(8) {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            chunk.copy_from_slice(&x.to_le_bytes());
        }
        page
    }

    #[test]
    fn repli_sans_perte_de_la_page_en_attente() -> Result<()> {
        let mut p = Packer::default();
        assert_eq!(p.push(62, &pseudo_alea())?, Push::Raw);
        assert!(p.finish().is_none());
        let mut p = Packer::default();
        let mut half = pseudo_alea();
        half[2048..].fill(0xee);
        assert_eq!(p.push(62, &half)?, Push::Added);
        assert_eq!(p.push(63, &half)?, Push::Full);
        assert_eq!(p.push(64, &[14; 4096])?, Push::Full);
        assert!(p.finish().is_none());
        let mut p = Packer::default();
        assert_eq!(p.push(62, &half)?, Push::Added);
        assert_eq!(p.push(63, &[11; 4096])?, Push::Added);
        let raw = p.finish().unwrap();
        let u = inspect(&raw)?;
        assert_eq!(u.page(62)?, half);
        assert_eq!(u.page(63)?, [11; 4096]);
        Ok(())
    }

    #[test]
    fn controle_du_repertoire_et_des_champs_obligatoires() -> Result<()> {
        let mut p = Packer::default();
        assert_eq!(p.push(32, &[1; 4096])?, Push::Added);
        assert_eq!(p.push(33, &[2; 4096])?, Push::Added);
        let raw = p.finish().unwrap();
        for (at, b) in [
            (0, 0),
            (8, 2),
            (10, 99),
            (11, 0),
            (11, 17),
            (12, 1),
            (16, 1),
            (17, 255),
            (18, 1),
            (20, 2),
            (33, 4),
            (36, 32),
            (36, 64),
            (34, 0),
            (35, 255),
            (40, 1),
            (4095, 1),
        ] {
            let mut bad = raw;
            bad[at] = b;
            assert!(inspect(&bad).is_err(), "position {at}/{b}");
        }
        let mut bad = raw;
        // Le premier flux de la seconde page contient un offset nul.
        let other = PAYLOAD + usize::from(word(&raw, DIRECTORY + 2));
        bad[other + 2..other + 4].fill(0);
        let u = inspect(&bad)?;
        assert_eq!(u.page(32)?, [1; 4096]);
        assert!(u.page(33).is_err());
        assert!(u.verify_all().is_err());
        Ok(())
    }

    /// Le script fournit un fichier privé « page, longueur LE u16, bloc LZ4 »
    /// et compare les flux de réponse au décodeur indépendant de référence.
    #[test]
    #[ignore = "piloté par novaFS/lab/p6-lz4-interop.py"]
    fn corpus_liblz4_bidirectionnel() -> std::io::Result<()> {
        use std::io::{Read, Write};
        let dir = std::path::PathBuf::from(
            std::env::var_os("NOVAFS_LZ4_FIXTURE").expect("répertoire du banc"),
        );
        let mut corpus = std::fs::File::open(dir.join("source"))?;
        let mut reply = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(dir.join("reponse"))?;
        let mut checked = 0;
        let mut input = [0; 4096];
        let mut stream = [0; 4128];
        while corpus.read(&mut input[..1])? == 1 {
            corpus.read_exact(&mut input[1..])?;
            let mut length = [0; 2];
            corpus.read_exact(&mut length)?;
            let n = usize::from(u16::from_le_bytes(length));
            assert!(n <= stream.len());
            corpus.read_exact(&mut stream[..n])?;
            assert_eq!(decode_block(&stream[..n]).unwrap(), input);
            let n = encode_block(&input, &mut stream).unwrap();
            reply.write_all(&(n as u16).to_le_bytes())?;
            reply.write_all(&stream[..n])?;
            checked += 1;
        }
        assert_eq!(checked, 256);
        Ok(())
    }
}
