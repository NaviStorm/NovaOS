//! Deux fichiers P9 internes à contenu CoW. L’écrivain unique publie une
//! génération dans le slot opposé. Le nom de la préparation n’est jamais élu.
use alloc::vec::Vec;
use novafs::{Device, Engine, Error, Kind, Result, View};
const ROOTS: [&str; 2] = ["\x1fNovaOS-services-0", "\x1fNovaOS-services-1"];
const STAGE: &str = "\x1fNovaOS-services-new";
const ENROLMENT: &str = "\x1fNovaOS-services-enrolled";
pub const AUTHORITY: &str = "\x1fNovaOS-services-authority";
const AUTH_MAGIC: &[u8; 8] = b"NSVCAU01";
const AUTH_BYTES: usize = 112;
const MAGIC: &[u8; 8] = b"NSVCCFG1";
const HEADER: usize = 64;
pub const LIMIT: usize = 4096;
pub const CONFLICT: &str = "génération de configuration périmée";
pub fn hidden(n: &str) -> bool {
    matches!(n, STAGE | ENROLMENT | AUTHORITY) || ROOTS.contains(&n)
}
pub(super) fn crc(bytes: &[u8]) -> u32 {
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
    digest: [u8; 32],
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
        digest: digest(fs.ids(), g, n, &bytes),
    }))
}
// SHA-256 du volume, de la génération, de la longueur et du contenu complété
// à 4096 octets. Le préfixe de position réservé dépasse MAX_BLOCKS de P9.
fn digest(ids: novafs::Ids, generation: u64, len: usize, envelope: &[u8]) -> [u8; 32] {
    let domain = u64::from_be_bytes(*b"NSVCAP\0\0");
    novafs::codec::data_hash(
        ids,
        domain | len as u64,
        generation,
        envelope[HEADER..].try_into().unwrap(),
    )
}

fn authority_error() -> Error {
    Error::Corrupt("autorité durable des capacités de service")
}

fn private_record<D: Device>(fs: &Engine<D>, name: &str, size: usize) -> Result<Option<Vec<u8>>> {
    let file = match fs.lookup_at(fs.root_id(), name) {
        Ok(file) => file,
        Err(Error::NotFound) => return Ok(None),
        Err(e) => return Err(e),
    };
    if file.kind != Kind::File || file.size != size as u64 {
        return Err(authority_error());
    }
    let bytes = fs.inspect_id(file.id, 0, size)?;
    if bytes.len() != size {
        return Err(authority_error());
    }
    Ok(Some(bytes))
}

fn provenance(ids: novafs::Ids) -> [u8; 40] {
    let mut header = [0; 40];
    header[..8].copy_from_slice(AUTH_MAGIC);
    header[8..24].copy_from_slice(&ids.fs);
    header[24..40].copy_from_slice(&ids.lv);
    header
}

fn reference(
    ids: novafs::Ids,
    generation: u64,
    head: [u8; 32],
    prior: [u8; 32],
) -> [u8; AUTH_BYTES] {
    let mut record = [0; AUTH_BYTES];
    record[..40].copy_from_slice(&provenance(ids));
    record[40..48].copy_from_slice(&generation.to_le_bytes());
    record[48..80].copy_from_slice(&head);
    record[80..].copy_from_slice(&prior);
    record
}

// Le marqueur est créé une seule fois, avant la première restriction. Distinguer
// le premier enrôlement de la perte ultérieure de l'un des deux objets privés.
fn authority<D: Device>(fs: &Engine<D>) -> Result<Option<Vec<u8>>> {
    match (
        private_record(fs, ENROLMENT, 40)?,
        private_record(fs, AUTHORITY, AUTH_BYTES)?,
    ) {
        (None, None) => Ok(None),
        (Some(marker), Some(decision))
            if marker == provenance(fs.ids()) && decision[..40] == marker =>
        {
            Ok(Some(decision))
        }
        _ => Err(authority_error()),
    }
}

fn extended(data: &[u8]) -> bool {
    data.starts_with(b"NVSVCC01") && data.get(8..16) != Some(4u64.to_le_bytes().as_slice())
}

pub fn load<D: Device>(fs: &Engine<D>, previous: bool) -> Result<Option<Record>> {
    if fs.atime_policy().is_none() {
        return Err(Error::Invalid("P9 requis"));
    }
    let decision = authority(fs)?;
    let (a, b) = match (record(fs, 0), record(fs, 1)) {
        (Ok(a), Ok(b)) => (a, b),
        // Le gestionnaire expose ce choix comme récupération dégradée. Une
        // erreur de lecture P9 reste une erreur, sans repli silencieux.
        (Err(Error::Corrupt("enveloppe de configuration")), Ok(Some(good)))
        | (Ok(Some(good)), Err(Error::Corrupt("enveloppe de configuration")))
            if previous && decision.is_none() && !extended(&good.data) =>
        {
            return Ok(Some(good));
        }
        (Err(e), _) | (_, Err(e)) => return Err(e),
    };
    let (old, new) = match (a, b) {
        (None, None) if decision.is_none() => return Ok(None),
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
    if let Some(decision) = decision {
        let prior = old.as_ref().map_or([0; 32], |r| r.digest);
        if decision != reference(fs.ids(), new.generation, new.digest, prior) || previous {
            return Err(authority_error());
        }
    } else if extended(&new.data) || old.as_ref().is_some_and(|r| extended(&r.data)) {
        return Err(authority_error());
    }
    Ok(if previous { old } else { Some(new) })
}
// La publication intermédiaire peut avoir une issue inconnue chez P9. La
// nouvelle politique, elle, exige aussi le slot final, encore non soumis.
fn preparation_error(error: Error) -> Error {
    match error {
        Error::UnknownOutcome | Error::Publication { .. } => Error::Unavailable,
        other => other,
    }
}

pub fn save<D: Device>(fs: &mut Engine<D>, expected: u64, data: &[u8]) -> Result<u64> {
    if data.is_empty() || data.len() > LIMIT {
        return Err(Error::Invalid("taille configuration"));
    }
    let current = load(fs, false)?;
    if current.as_ref().map_or(0, |r| r.generation) != expected {
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
    let enrolled = authority(fs)?.is_some();
    if enrolled && !extended(data) {
        return Err(Error::Invalid(
            "retour au catalogue historique après enrôlement",
        ));
    }
    if enrolled || extended(data) {
        // Le marqueur et la décision sont des objets P9 distincts des slots et
        // de leur préparation. Aucune voie VFS cliente ne peut les modifier.
        let lineage = provenance(fs.ids());
        let decision = reference(
            fs.ids(),
            next,
            digest(fs.ids(), next, data.len(), &bytes),
            current.as_ref().map_or([0; 32], |r| r.digest),
        );
        let prepare = (|| {
            let authority = if enrolled {
                fs.lookup_at(root, AUTHORITY)?
            } else {
                let marker = fs.create_at(root, ENROLMENT, Kind::File)?;
                fs.prepare_write_id(marker.id, 0, &lineage)?.commit()?;
                fs.create_at(root, AUTHORITY, Kind::File)?
            };
            fs.prepare_write_id(authority.id, 0, &decision)?.commit()
        })();
        // Une coupure entre les deux commits rend le jeu incohérent : LOAD et
        // PREVIOUS refusent alors le lancement. La réponse de succès suit F2 du slot.
        prepare.map_err(preparation_error)?;
    }
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
            stage.map_err(preparation_error)?;
            fs.rename_at(root, STAGE, root, name)?;
        }
        Err(e) => return Err(e),
    }
    Ok(next)
}
