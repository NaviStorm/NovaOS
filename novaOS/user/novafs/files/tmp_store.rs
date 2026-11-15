//! Objets P9 réservés au service de temporaires. Le volume provient du montage
//! de confiance du gestionnaire et possède un seul écrivain. Les API clientes
//! n'acceptent aucun octet de contrôle dans les noms. Le service masque aussi
//! ces entrées à l'énumération de la racine publique.
use alloc::{string::String, vec::Vec};
use novafs::{Device, Engine, Error, Kind, Result, Stat, View};

pub const ROOT_NAME: &str = "\x1fNovaOS-tmp-v1";
const PREPARATION: &str = "\x1fNovaOS-tmp-v1.new-";
// Deux préparations distinctes permettent la reprise après une première
// interruption. Un objet préexistant sans provenance ne sert jamais de réserve.
const STAGES: [&str; 2] = ["\x1fNovaOS-tmp-v1.new-0", "\x1fNovaOS-tmp-v1.new-1"];
// Un sceau et deux noms par vue. Les registres en quarantaine comptent aussi.
const MAX_NAMES: usize = 1 + 2 * 32;
const SEAL: &str = "registre";
const UNVERIFIED: &str = "provenance du temporaire invérifiable";
const WIDTH: usize = 104;

pub fn hidden_root_name(name: &str) -> bool {
    name == ROOT_NAME || name.starts_with(PREPARATION)
}

#[derive(Clone, Copy)]
pub struct Store {
    id: u64,
    seal: u64,
}

fn optional<D: Device>(fs: &Engine<D>, parent: u64, name: &str) -> Result<Option<Stat>> {
    match fs.lookup_at(parent, name) {
        Ok(st) => Ok(Some(st)),
        Err(Error::NotFound) => Ok(None),
        Err(err) => Err(err),
    }
}

// Un registre nouveau ajoute exactement WIDTH octets logiques. Ne pas laisser
// un objet vide si le quota P9 connu avant la création refuse cette écriture.
fn room_for_record<D: Device>(fs: &Engine<D>) -> Result<()> {
    let a = fs.accounting()?;
    if a.logical_quota_bytes
        .is_some_and(|n| a.logical_bytes + WIDTH as u128 > u128::from(n))
    {
        return Err(Error::QuotaExceeded);
    }
    Ok(())
}

fn valid_identity(id: &[u8; 32]) -> bool {
    id[..16] != [0; 16]
        && u64::from_le_bytes(id[16..24].try_into().unwrap()) > 1 << 63
        && id[24..] != [0; 8]
}

fn name_of(prefix: char, id: &[u8; 32]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut name = String::with_capacity(65);
    name.push(prefix);
    for b in id {
        name.push(char::from(DIGITS[usize::from(b >> 4)]));
        name.push(char::from(DIGITS[usize::from(b & 15)]));
    }
    name
}

impl Store {
    fn record<D: Device>(
        &self,
        fs: &Engine<D>,
        owner: &[u8; 32],
        file: u64,
        data: u64,
    ) -> [u8; WIDTH] {
        let mut b = [0; WIDTH];
        b[..8].copy_from_slice(b"NVTMP001");
        b[8..24].copy_from_slice(&fs.ids().fs);
        b[24..40].copy_from_slice(&fs.ids().lv);
        b[40..48].copy_from_slice(&self.id.to_le_bytes());
        b[48..56].copy_from_slice(&self.seal.to_le_bytes());
        b[56..88].copy_from_slice(owner);
        b[88..96].copy_from_slice(&file.to_le_bytes());
        b[96..].copy_from_slice(&data.to_le_bytes());
        b
    }

    fn root_valid<D: Device>(&self, fs: &Engine<D>) -> Result<()> {
        let dir = optional(fs, fs.root_id(), ROOT_NAME)?.ok_or(Error::Invalid(UNVERIFIED))?;
        if dir.id != self.id || dir.kind != Kind::Directory {
            return Err(Error::Invalid(UNVERIFIED));
        }
        let seal = optional(fs, self.id, SEAL)?.ok_or(Error::Invalid(UNVERIFIED))?;
        if seal.id != self.seal
            || seal.kind != Kind::File
            || seal.size != WIDTH as u64
            || fs.inspect_id(seal.id, 0, WIDTH)? != self.record(fs, &[0; 32], seal.id, self.id)
        {
            return Err(Error::Invalid(UNVERIFIED));
        }
        Ok(())
    }

    pub fn mount<D: Device>(fs: &mut Engine<D>) -> Result<Self> {
        if fs.atime_policy().is_none() {
            return Err(Error::Invalid("tmp exige P9"));
        }
        let root = fs.root_id();
        let store = match optional(fs, root, ROOT_NAME)? {
            Some(dir) => {
                if dir.kind != Kind::Directory {
                    return Err(Error::Invalid(UNVERIFIED));
                }
                let seal = optional(fs, dir.id, SEAL)?.ok_or(Error::Invalid(UNVERIFIED))?;
                Self {
                    id: dir.id,
                    seal: seal.id,
                }
            }
            None => {
                // RENAME sans écrasement publie uniquement un registre entièrement initialisé.
                // Une coupure préalable laisse un dossier caché et inatteignable.
                let mut free_stage = None;
                for stage in STAGES {
                    if optional(fs, root, stage)?.is_none() {
                        free_stage = Some(stage);
                        break;
                    }
                }
                let stage = free_stage.ok_or(Error::Limit("préparations tmp en quarantaine"))?;
                room_for_record(fs)?;
                let dir = fs.create_at(root, stage, Kind::Directory)?;
                let seal = fs.create_at(dir.id, SEAL, Kind::File)?;
                let store = Self {
                    id: dir.id,
                    seal: seal.id,
                };
                let record = store.record(fs, &[0; 32], seal.id, dir.id);
                fs.prepare_write_id(seal.id, 0, &record)?.commit()?;
                fs.rename_at(root, stage, root, ROOT_NAME)?;
                store
            }
        };
        store.root_valid(fs)?;
        Ok(store)
    }

    /// Compter aussi les objets incomplets ou inconnus avant de réserver. Le
    /// curseur ne garde que huit noms en mémoire et ne lit pas tout un registre
    /// importé dépassant la capacité du service.
    fn check_room<D: Device>(&self, fs: &Engine<D>, needed: usize) -> Result<()> {
        let mut count = needed;
        let mut after = None;
        loop {
            let mut names = fs.inspect_dir(self.id, after.as_deref(), 8)?;
            let n = names.len();
            count += n;
            if count > MAX_NAMES {
                return Err(Error::Limit("capacité du registre tmp"));
            }
            if n < 8 {
                return Ok(());
            }
            after = names.pop();
        }
    }

    /// Une incarnation nouvelle reçoit seulement l'OID du répertoire vide. Le
    /// service attribue sa capacité après le retour du dernier commit durable.
    pub fn create<D: Device>(&self, fs: &mut Engine<D>, owner: [u8; 32]) -> Result<u64> {
        if !valid_identity(&owner) {
            return Err(Error::Invalid("identité d’instance"));
        }
        self.root_valid(fs)?;
        let entry = name_of('r', &owner);
        let directory = name_of('t', &owner);
        if optional(fs, self.id, &entry)?.is_some() || optional(fs, self.id, &directory)?.is_some()
        {
            return Err(Error::Exists);
        }
        self.check_room(fs, 2)?;
        room_for_record(fs)?;
        let file = fs.create_at(self.id, &entry, Kind::File)?;
        // Le compteur d'OID ne peut pas avancer entre ce point et la création
        // du dossier. Le moteur sérialise l’écrivain et les écritures d’un
        // fichier existant n’allouent aucun nouvel objet de catalogue.
        let next = fs.manifest().next_oid;
        let record = self.record(fs, &owner, file.id, next);
        fs.prepare_write_id(file.id, 0, &record)?.commit()?;
        let dir = fs.create_at(self.id, &directory, Kind::Directory)?;
        if dir.id != next {
            return Err(Error::Invalid(UNVERIFIED));
        }
        Ok(dir.id)
    }

    fn owned_root<D: Device>(&self, fs: &Engine<D>, owner: &[u8; 32], file: Stat) -> Result<u64> {
        if file.kind != Kind::File || file.size != WIDTH as u64 {
            return Err(Error::Invalid(UNVERIFIED));
        }
        let b = fs.inspect_id(file.id, 0, WIDTH)?;
        let data = u64::from_le_bytes(
            b.get(96..)
                .and_then(|s| s.try_into().ok())
                .ok_or(Error::Invalid(UNVERIFIED))?,
        );
        if file.id.checked_add(1) != Some(data) || b != self.record(fs, owner, file.id, data) {
            return Err(Error::Invalid(UNVERIFIED));
        }
        Ok(data)
    }

    /// Une opération par feuille, avec le registre en place jusqu’à la fin de
    /// la purge. Après coupure les arêtes restantes sont relues depuis P9.
    pub fn purge<D: Device>(
        &self,
        fs: &mut Engine<D>,
        owner: [u8; 32],
        expected: u64,
    ) -> Result<()> {
        if !valid_identity(&owner) || expected == 0 {
            return Err(Error::Invalid("identité de purge"));
        }
        self.root_valid(fs)?;
        let entry = name_of('r', &owner);
        let name = name_of('t', &owner);
        let dir = optional(fs, self.id, &name)?;
        let Some(file) = optional(fs, self.id, &entry)? else {
            return if dir.is_none() {
                collect(fs)
            } else {
                Err(Error::Invalid(UNVERIFIED))
            };
        };
        if self.owned_root(fs, &owner, file)? != expected {
            return Err(Error::Invalid(UNVERIFIED));
        }
        if let Some(st) = dir {
            if st.kind != Kind::Directory || st.id != expected {
                return Err(Error::Invalid(UNVERIFIED));
            }
            remove_tree(fs, self.id, name, st)?;
        }
        fs.unlink_at(self.id, &entry, Kind::File)?;
        collect(fs)
    }

}

fn remove_tree<D: Device>(fs: &mut Engine<D>, parent: u64, name: String, st: Stat) -> Result<()> {
    let mut stack = Vec::new();
    stack
        .try_reserve_exact(Engine::<D>::MAX_DIRECTORY_DEPTH + 1)
        .map_err(|_| Error::NoMemory)?;
    stack.push((parent, name, st));
    while let Some((parent, name, st)) = stack.last() {
        if st.kind == Kind::Directory
            && let Some(child) = fs.inspect_dir(st.id, None, 1)?.pop()
        {
            if stack.len() > Engine::<D>::MAX_DIRECTORY_DEPTH {
                return Err(Error::Corrupt("profondeur tmp"));
            }
            let next = fs.lookup_at(st.id, &child)?;
            stack.push((st.id, child, next));
        } else {
            fs.unlink_at(*parent, name, st.kind)?;
            stack.pop();
        }
    }
    Ok(())
}

fn collect<D: Device>(fs: &mut Engine<D>) -> Result<()> {
    // Chaque pas P9 retire au moins un bloc de données mort. Aucun écrivain ne
    // peut en ajouter pendant la collecte. La capacité physique borne les pas.
    let mut remaining = fs.manifest().blocks;
    while fs.collection_pending()? {
        remaining = remaining
            .checked_sub(1)
            .ok_or(Error::Corrupt("collecte tmp sans progrès"))?;
        if !fs.collect()? {
            return Err(Error::Corrupt("collecte tmp sans extent"));
        }
    }
    Ok(())
}
