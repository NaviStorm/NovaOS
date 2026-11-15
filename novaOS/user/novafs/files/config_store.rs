//! Deux fichiers P9 internes à contenu CoW. L’écrivain unique publie une
//! génération dans le slot opposé. Le nom de la préparation n’est jamais élu.
use alloc::vec::Vec;
use novafs::{Device, Engine, Error, Kind, Result, View};
const ROOTS: [&str; 2] = ["\x1fNovaOS-services-0", "\x1fNovaOS-services-1"];
const STAGE: &str = "\x1fNovaOS-services-new";
const MAGIC: &[u8; 8] = b"NSVCCFG1";
const HEADER: usize = 64;
pub const LIMIT: usize = 4096;
pub const CONFLICT: &str = "génération de configuration périmée";
pub fn hidden(n: &str) -> bool {
    n == STAGE || ROOTS.contains(&n)
}
fn crc(bytes: &[u8]) -> u32 {
    let mut c = !0u32;
    for &b in bytes {
        c ^= b as u32;
        for _ in 0..8 {
            c = (c >> 1) ^ (0xedb88320 & 0u32.wrapping_sub(c & 1));
        }
    }
    !c
}
pub struct Record {
    pub generation: u64,
    pub data: Vec<u8>,
}
fn record<D: Device>(fs: &Engine<D>, slot: usize) -> Result<Option<Record>> {
    let file = match fs.lookup_at(fs.root_id(), ROOTS[slot]) {
        Ok(f) => f,
        Err(Error::NotFound) => return Ok(None),
        Err(e) => return Err(e),
    };
    let bad = || Error::Corrupt("enveloppe de configuration");
    if file.kind != Kind::File || file.size != (HEADER + LIMIT) as u64 {
        return Err(bad());
    }
    let bytes = fs.inspect_id(file.id, 0, HEADER + LIMIT)?;
    if bytes.len() != HEADER + LIMIT
        || &bytes[0..8] != MAGIC
        || bytes[8..24] != fs.ids().fs
        || bytes[24..40] != fs.ids().lv
    {
        return Err(bad());
    }
    let g = u64::from_le_bytes(bytes[40..48].try_into().unwrap());
    let n = u32::from_le_bytes(bytes[48..52].try_into().unwrap()) as usize;
    let checksum = u32::from_le_bytes(bytes[52..56].try_into().unwrap());
    if g == 0
        || (g % 2) as usize != slot
        || !(1..=LIMIT).contains(&n)
        || bytes[56..HEADER]
            .iter()
            .chain(&bytes[HEADER + n..])
            .any(|&b| b != 0)
        || crc(&bytes[..52]) ^ crc(&bytes[HEADER..]) != checksum
    {
        return Err(bad());
    }
    Ok(Some(Record {
        generation: g,
        data: bytes[HEADER..HEADER + n].to_vec(),
    }))
}
pub fn load<D: Device>(fs: &Engine<D>, previous: bool) -> Result<Option<Record>> {
    if fs.atime_policy().is_none() {
        return Err(Error::Invalid("P9 requis"));
    }
    let (a, b) = match (record(fs, 0), record(fs, 1)) {
        (Ok(a), Ok(b)) => (a, b),
        // Le gestionnaire expose ce choix comme récupération dégradée. Une
        // erreur de lecture P9 reste une erreur, sans repli silencieux.
        (Err(Error::Corrupt("enveloppe de configuration")), Ok(Some(good)))
        | (Ok(Some(good)), Err(Error::Corrupt("enveloppe de configuration")))
            if previous =>
        {
            return Ok(Some(good));
        }
        (Err(e), _) | (_, Err(e)) => return Err(e),
    };
    let (old, new) = match (a, b) {
        (None, None) => return Ok(None),
        (None, Some(first)) if first.generation == 1 => (None, first),
        (Some(a), Some(b)) if a.generation.abs_diff(b.generation) == 1 => {
            if a.generation < b.generation {
                (Some(a), b)
            } else {
                (Some(b), a)
            }
        }
        _ => return Err(Error::Corrupt("séquence des configurations")),
    };
    Ok(if previous { old } else { Some(new) })
}
pub fn save<D: Device>(fs: &mut Engine<D>, expected: u64, data: &[u8]) -> Result<u64> {
    if data.is_empty() || data.len() > LIMIT {
        return Err(Error::Invalid("taille configuration"));
    }
    if load(fs, false)?.map_or(0, |r| r.generation) != expected {
        return Err(Error::Invalid(CONFLICT));
    }
    let next = expected
        .checked_add(1)
        .ok_or(Error::Limit("génération configuration"))?;
    let root = fs.root_id();
    let name = ROOTS[(next % 2) as usize];
    let mut bytes = alloc::vec![0;HEADER+LIMIT];
    bytes[..8].copy_from_slice(MAGIC);
    bytes[8..24].copy_from_slice(&fs.ids().fs);
    bytes[24..40].copy_from_slice(&fs.ids().lv);
    bytes[40..48].copy_from_slice(&next.to_le_bytes());
    bytes[48..52].copy_from_slice(&(data.len() as u32).to_le_bytes());
    bytes[HEADER..HEADER + data.len()].copy_from_slice(data);
    let checksum = crc(&bytes[..52]) ^ crc(&bytes[HEADER..]);
    bytes[52..56].copy_from_slice(&checksum.to_le_bytes());
    match fs.lookup_at(root, name) {
        Ok(st) => fs.prepare_write_id(st.id, 0, &bytes)?.commit()?,
        Err(Error::NotFound) => {
            // Tout échec avant le renommage laisse la génération courante intacte.
            let stage = (|| {
                let st = match fs.lookup_at(root, STAGE) {
                    Ok(s) if s.kind == Kind::File && s.size <= (HEADER + LIMIT) as u64 => s,
                    Ok(_) => return Err(Error::Corrupt("préparation de configuration")),
                    Err(Error::NotFound) => fs.create_at(root, STAGE, Kind::File)?,
                    Err(e) => return Err(e),
                };
                fs.prepare_write_id(st.id, 0, &bytes)?.commit()
            })();
            stage.map_err(|e| match e {
                Error::UnknownOutcome | Error::Publication { .. } => Error::Unavailable,
                other => other,
            })?;
            fs.rename_at(root, STAGE, root, name)?;
        }
        Err(e) => return Err(e),
    }
    Ok(next)
}
