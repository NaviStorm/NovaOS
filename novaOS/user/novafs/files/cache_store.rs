//! Catalogue privé du cache, sur un montage P9 de confiance à écrivain unique.
//! Les preuves d'identité viennent du courtier, jamais du disque. Les sceaux
//! structurels ci-dessous lient les arêtes P9 ; ce ne sont ni des MAC ni une
//! protection contre un administrateur du volume ou le rollback intégral.
use alloc::{format, string::String, vec::Vec};
use novafs::{Device, Engine, Error, Kind, Result, Stat, View};

pub type Key = [u8; 32];
pub const ROOT: &str = "\x1fNovaOS-cache-v1";
const STAGE: &str = "\x1fNovaOS-cache-v1.new";
const SEAL: &str = "registre";
pub const UNVERIFIED: &str = "cache en quarantaine : provenance non vérifiée";
pub const STALE: &str = "génération du cache révoquée";
const WIDTH: usize = 104;
const INCOMING: &str = "\x1fpublication";
const PAGE: u64 = novafs::BLOCK_BYTES as u64;
// Limite du plan CoW P9 (Tx::reserve). Les opérations de catalogue ne
// disposent pas de prepare_create/rename : borner leur *prochain* commit.
const MAX_COW: u64 = 64;
pub fn hidden(name: &str) -> bool {
    matches!(name, ROOT | STAGE)
}

#[derive(Clone, Copy, Debug)]
pub struct Policy {
    pub cache: u64,
    pub user: u64,
    pub global: u64,
    pub reserve: u64,
}
impl Policy {
    pub fn validate(self) -> Result<Self> {
        if self.cache > self.user || self.user > self.global || self.reserve == 0 {
            Err(Error::Invalid("politique de cache"))
        } else {
            Ok(self)
        }
    }
    // Une part pour une seconde application, puis un second utilisateur.
    // Le service garde en plus la place d'un plan CoW pour le système.
    pub fn for_volume<D: Device>(fs: &Engine<D>) -> Result<Self> {
        let a = fs.accounting()?;
        let global = a
            .capacity_blocks
            .saturating_sub(a.maintenance_target)
            .saturating_sub(MAX_COW)
            .saturating_mul(PAGE)
            / 2;
        Ok(Self {
            cache: global / 4,
            user: global / 2,
            global,
            reserve: MAX_COW,
        })
    }
}
#[derive(Clone, Copy)]
pub struct Store {
    root: u64,
    seal: u64,
    policy: Policy,
}
#[derive(Default)]
pub struct Recovery {
    pub purged: u64,
    pub quarantined: u64,
}
fn opt<D: Device>(fs: &Engine<D>, dir: u64, name: &str) -> Result<Option<Stat>> {
    match fs.lookup_at(dir, name) {
        Ok(s) => Ok(Some(s)),
        Err(Error::NotFound) => Ok(None),
        Err(e) => Err(e),
    }
}
fn quarantine<T>() -> Result<T> {
    Err(Error::Invalid(UNVERIFIED))
}
fn valid(k: &Key) -> bool {
    k[..16] != [0; 16] && k[16..] != [0; 16]
}
fn name(k: &Key) -> String {
    let mut s = String::with_capacity(65);
    s.push('a');
    const HEX: &[u8] = b"0123456789abcdef";
    for &v in k {
        s.push(HEX[(v >> 4) as usize] as char);
        s.push(HEX[(v & 15) as usize] as char);
    }
    s
}
fn slot(prefix: char, generation: u64) -> String {
    format!("{prefix}{generation:016x}")
}
fn slot_id(s: &str, prefix: char) -> Option<u64> {
    let b = s.strip_prefix(prefix)?;
    (b.len() == 16
        && b.bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c)))
    .then(|| u64::from_str_radix(b, 16).ok())
    .flatten()
    .filter(|v| *v != 0)
}
fn leaf(s: &str) -> Result<()> {
    if s.is_empty()
        || s.len() > 48
        || matches!(s, "." | "..")
        || s.chars().any(|c| c == '/' || c == '\\' || c.is_control())
    {
        Err(Error::Invalid("nom privé de cache"))
    } else {
        Ok(())
    }
}
fn next<D: Device>(fs: &Engine<D>, dir: u64, after: &Option<String>) -> Result<Option<String>> {
    Ok(fs.inspect_dir(dir, after.as_deref(), 1)?.pop())
}
fn room<D: Device>(fs: &Engine<D>, p: Policy) -> Result<()> {
    let a = fs.accounting()?;
    if a.available_blocks.saturating_sub(MAX_COW) < p.reserve || a.available_blocks < MAX_COW {
        Err(Error::NoSpace)
    } else {
        Ok(())
    }
}

impl Store {
    fn record<D: Device>(
        &self,
        fs: &Engine<D>,
        key: &Key,
        marker: u64,
        generation: u64,
    ) -> [u8; WIDTH] {
        let mut b = [0u8; WIDTH];
        b[..8].copy_from_slice(b"NVCACH01");
        b[8..24].copy_from_slice(&fs.ids().fs);
        b[24..40].copy_from_slice(&fs.ids().lv);
        b[40..48].copy_from_slice(&self.root.to_le_bytes());
        b[48..56].copy_from_slice(&self.seal.to_le_bytes());
        b[56..88].copy_from_slice(key);
        b[88..96].copy_from_slice(&marker.to_le_bytes());
        b[96..104].copy_from_slice(&generation.to_le_bytes());
        b
    }
    fn check_root<D: Device>(&self, fs: &Engine<D>) -> Result<()> {
        let s = opt(fs, fs.root_id(), ROOT)?;
        let Some(s) = s.filter(|s| s.kind == Kind::Directory && s.id == self.root) else {
            return quarantine();
        };
        let Some(m) = opt(fs, s.id, SEAL)?.filter(|m| {
            m.kind == Kind::File
                && m.id == self.seal
                && (m.size == WIDTH as u64 || m.size == (2 * WIDTH) as u64)
        }) else {
            return quarantine();
        };
        if fs.inspect_id(m.id, 0, WIDTH)? != self.record(fs, &[0; 32], m.id, s.id) {
            return quarantine();
        }
        Ok(())
    }
    // Un second enregistrement, facultatif dans le sceau, journalise la portée
    // administrative et la première incarnation *exclue* du retrait.
    fn command<D: Device>(
        &self,
        fs: &Engine<D>,
        user: Option<[u8; 16]>,
        cutoff: u64,
    ) -> [u8; WIDTH] {
        let mut k = [0; 32];
        if let Some(u) = user {
            k[..16].copy_from_slice(&u);
            k[16] = 1;
        }
        let mut b = self.record(fs, &k, self.seal, cutoff);
        b[..8].copy_from_slice(b"NVCADM01");
        b
    }
    fn intent<D: Device>(&self, fs: &Engine<D>) -> Result<Option<(Option<[u8; 16]>, u64)>> {
        if fs.stat_id(self.seal)?.size == WIDTH as u64 {
            return Ok(None);
        }
        let b = fs.inspect_id(self.seal, WIDTH as u64, WIDTH)?;
        if b == [0; WIDTH] {
            return Ok(None);
        }
        if b.len() != WIDTH {
            return quarantine();
        }
        let user = match b[72] {
            0 => None,
            1 => Some(b[56..72].try_into().unwrap()),
            _ => return quarantine(),
        };
        let cutoff = u64::from_le_bytes(b[96..104].try_into().unwrap());
        if user == Some([0; 16])
            || cutoff <= self.seal
            || cutoff > fs.manifest().next_oid
            || b != self.command(fs, user, cutoff)
        {
            return quarantine();
        }
        Ok(Some((user, cutoff)))
    }
    fn available<D: Device>(&self, fs: &Engine<D>) -> Result<()> {
        self.check_root(fs)?;
        if self.intent(fs)?.is_some() {
            Err(Error::Busy)
        } else {
            Ok(())
        }
    }
    fn available_key<D: Device>(&self, fs: &Engine<D>, key: &Key) -> Result<()> {
        self.check_root(fs)?;
        if let Some((user, _)) = self.intent(fs)? {
            if user.is_none_or(|id| id == key[..16]) {
                return Err(Error::Busy);
            }
        }
        Ok(())
    }
    pub fn mount<D: Device>(fs: &mut Engine<D>, policy: Policy) -> Result<Self> {
        let policy = policy.validate()?;
        if fs.atime_policy().is_none() {
            return Err(Error::Invalid("cache exige P9"));
        }
        let root = fs.root_id();
        let me = if let Some(d) = opt(fs, root, ROOT)? {
            if d.kind != Kind::Directory {
                return quarantine();
            }
            let Some(m) = opt(fs, d.id, SEAL)? else {
                return quarantine();
            };
            Self {
                root: d.id,
                seal: m.id,
                policy,
            }
        } else {
            // Une préparation étrangère ou déchirée n'est jamais adoptée.
            if opt(fs, root, STAGE)?.is_some() {
                return quarantine();
            }
            room(fs, policy)?;
            let d = fs.create_at(root, STAGE, Kind::Directory)?;
            room(fs, policy)?;
            let m = fs.create_at(d.id, SEAL, Kind::File)?;
            let me = Self {
                root: d.id,
                seal: m.id,
                policy,
            };
            let b = me.record(fs, &[0; 32], m.id, d.id);
            let p = fs.prepare_write_id(m.id, 0, &b)?;
            if p.accounting().available_blocks < policy.reserve {
                return Err(Error::NoSpace);
            }
            p.commit()?;
            room(fs, policy)?;
            fs.rename_at(root, STAGE, root, ROOT)?;
            me
        };
        me.check_root(fs)?;
        Ok(me)
    }
    fn owner<D: Device>(&self, fs: &Engine<D>, marker: Stat, g: u64) -> Result<Key> {
        if marker.kind != Kind::File
            || marker.size != WIDTH as u64
            || marker.id.checked_add(1) != Some(g)
        {
            return quarantine();
        }
        let bytes = fs.inspect_id(marker.id, 0, WIDTH)?;
        let key: Key = bytes
            .get(56..88)
            .and_then(|s| s.try_into().ok())
            .ok_or(Error::Invalid(UNVERIFIED))?;
        if !valid(&key) || bytes != self.record(fs, &key, marker.id, g) {
            return quarantine();
        }
        Ok(key)
    }
    fn current<D: Device>(&self, fs: &Engine<D>, key: &Key, g: u64) -> Result<Stat> {
        self.check_root(fs)?;
        if !valid(key) || g == 0 {
            return Err(Error::Invalid(STALE));
        }
        let Some(d) = opt(fs, self.root, &name(key))?.filter(|s| s.id == g) else {
            return Err(Error::Invalid(STALE));
        };
        if d.kind != Kind::Directory {
            return quarantine();
        }
        let Some(m) = opt(fs, self.root, &slot('r', g))? else {
            return if opt(fs, self.root, &slot('d', g))?.is_some() {
                Err(Error::Invalid(STALE))
            } else {
                quarantine()
            };
        };
        if self.owner(fs, m, g)? != *key {
            return quarantine();
        }
        Ok(d)
    }
    // Facturer au moins une page par incarnation et par nom (même vide).
    // La réserve physique est contrôlée séparément sur le plan P9.
    fn usage<D: Device>(&self, fs: &Engine<D>, d: u64) -> Result<u64> {
        let mut cost = PAGE;
        let mut cursor = None;
        while let Some(n) = next(fs, d, &cursor)? {
            if n != INCOMING && leaf(&n).is_err() {
                return quarantine();
            }
            let st = fs.lookup_at(d, &n)?;
            if st.kind != Kind::File {
                return quarantine();
            }
            cost = cost
                .checked_add(PAGE)
                .and_then(|v| v.checked_add(st.size))
                .ok_or(Error::QuotaExceeded)?;
            cursor = Some(n);
        }
        Ok(cost)
    }
    fn admit<D: Device>(&self, fs: &Engine<D>, key: &Key, delta: u64) -> Result<()> {
        if delta == 0 {
            return Ok(());
        }
        let (mut by_cache, mut by_user, mut global) = (0u128, 0u128, 0u128);
        let mut cursor = None;
        while let Some(n) = next(fs, self.root, &cursor)? {
            if let Some(g) = slot_id(&n, 'r') {
                let m = fs.lookup_at(self.root, &n)?;
                let k = self.owner(fs, m, g)?;
                // Un marqueur orphelin ne donne aucun droit. Bloquer la
                // croissance tant que la comptabilité n'est pas vérifiable.
                let d = self.current(fs, &k, g)?;
                let charge = u128::from(self.usage(fs, d.id)?);
                global += charge;
                if k[..16] == key[..16] {
                    by_user += charge
                }
                if k == *key {
                    by_cache += charge
                }
            } else if n != SEAL {
                // Chaque répertoire actif doit avoir son propre registre.
                let d = fs.lookup_at(self.root, &n)?;
                if d.kind != Kind::Directory || !n.starts_with('a') {
                    return quarantine();
                }
                let Some(m) = opt(fs, self.root, &slot('r', d.id))? else {
                    return quarantine();
                };
                if name(&self.owner(fs, m, d.id)?) != n {
                    return quarantine();
                }
            }
            cursor = Some(n)
        }
        let n = u128::from(delta);
        let p = self.policy;
        if by_cache + n > p.cache.into()
            || by_user + n > p.user.into()
            || global + n > p.global.into()
        {
            Err(Error::QuotaExceeded)
        } else {
            Ok(())
        }
    }
    pub fn open<D: Device>(&self, fs: &mut Engine<D>, key: &Key) -> Result<u64> {
        self.available_key(fs, key)?;
        if !valid(key) {
            return Err(Error::Invalid("identité non attestée"));
        }
        let n = name(key);
        if let Some(d) = opt(fs, self.root, &n)? {
            return Ok(self.current(fs, key, d.id)?.id);
        }
        self.admit(fs, key, PAGE)?;
        room(fs, self.policy)?;
        let g = fs
            .manifest()
            .next_oid
            .checked_add(1)
            .ok_or(Error::Limit("OID cache épuisé"))?;
        let m = fs.create_at(self.root, &slot('r', g), Kind::File)?;
        let b = self.record(fs, key, m.id, g);
        let p = fs.prepare_write_id(m.id, 0, &b)?;
        if p.accounting().available_blocks < self.policy.reserve {
            return Err(Error::NoSpace);
        }
        p.commit()?;
        room(fs, self.policy)?;
        let d = fs.create_at(self.root, &n, Kind::Directory)?;
        if d.id != g {
            return quarantine();
        }
        Ok(g)
    }
    pub fn read<D: Device>(
        &self,
        fs: &Engine<D>,
        key: &Key,
        g: u64,
        n: &str,
        off: u64,
        len: usize,
    ) -> Result<Vec<u8>> {
        self.available_key(fs, key)?;
        leaf(n)?;
        if len > 64 {
            return Err(Error::Limit("datagramme cache"));
        }
        let d = self.current(fs, key, g)?;
        let st = fs.lookup_at(d.id, n)?;
        if st.kind != Kind::File {
            return quarantine();
        }
        fs.inspect_id(st.id, off, len)
    }
    // Le moteur reste empoisonné. L'issue physique d'une préparation
    // privée peut être inconnue alors que le nom client n'est pas publié.
    fn unpublished(e: Error) -> Error {
        match e {
            Error::UnknownOutcome | Error::Publication { .. } => Error::Unavailable,
            other => other,
        }
    }
    fn write_object<D: Device>(
        &self,
        fs: &mut Engine<D>,
        id: u64,
        off: u64,
        data: &[u8],
    ) -> Result<()> {
        let p = fs.prepare_write_id(id, off, data)?;
        if p.accounting().available_blocks < self.policy.reserve {
            return Err(Error::NoSpace);
        }
        p.commit()
    }
    pub fn write<D: Device>(
        &self,
        fs: &mut Engine<D>,
        key: &Key,
        g: u64,
        n: &str,
        off: u64,
        data: &[u8],
    ) -> Result<()> {
        self.available_key(fs, key).map_err(Self::unpublished)?;
        leaf(n)?;
        if data.is_empty() || data.len() > 64 {
            return Err(Error::Invalid("taille de fragment cache"));
        }
        let d = self.current(fs, key, g)?;
        let end = off
            .checked_add(data.len() as u64)
            .filter(|v| *v <= i64::MAX as u64)
            .ok_or(Error::Invalid("offset cache"))?;
        let st = opt(fs, d.id, n)?;
        if st.is_some_and(|s| s.kind != Kind::File) {
            return quarantine();
        }
        let delta = match st {
            Some(s) => end.saturating_sub(s.size),
            None => PAGE.checked_add(end).ok_or(Error::QuotaExceeded)?,
        };
        self.admit(fs, key, delta)?;
        if let Some(file) = st {
            // L'ancre de ce commit publie directement la nouvelle valeur.
            return self.write_object(fs, file.id, off, data);
        }
        let prepare = (|| -> Result<()> {
            if opt(fs, d.id, INCOMING)?.is_some() {
                fs.unlink_at(d.id, INCOMING, Kind::File)?;
            }
            room(fs, self.policy)?;
            let id = fs.create_at(d.id, INCOMING, Kind::File)?.id;
            self.write_object(fs, id, off, data)?;
            room(fs, self.policy)
        })();
        prepare.map_err(Self::unpublished)?;
        // Dès l'envoi de l'ancre de RENAME, les deux issues sont possibles.
        fs.rename_at(d.id, INCOMING, d.id, n)
    }
    /// Politique automatique explicite : l'incarnation vérifiée la plus
    /// ancienne. Le courtier révoque d'abord et soumet cette paire exacte.
    pub fn pressure<D: Device>(&self, fs: &Engine<D>) -> Result<Option<(Key, u64)>> {
        self.available(fs)?;
        if fs.accounting()?.available_blocks >= self.policy.reserve.saturating_add(MAX_COW) {
            return Ok(None);
        }
        let cursor = Some(String::from("r"));
        if let Some(n) = next(fs, self.root, &cursor)?
            && let Some(g) = slot_id(&n, 'r')
        {
            let k = self.owner(fs, fs.lookup_at(self.root, &n)?, g)?;
            self.current(fs, &k, g)?;
            return Ok(Some((k, g)));
        }
        Ok(None)
    }
    pub fn auto_purge<D: Device>(&self, fs: &mut Engine<D>, key: &Key, g: u64) -> Result<bool> {
        if self.pressure(fs)? != Some((*key, g)) {
            return Ok(false);
        }
        self.purge(fs, key, g)?;
        Ok(true)
    }
    pub fn begin_purge<D: Device>(&self, fs: &mut Engine<D>, key: &Key, g: u64) -> Result<()> {
        let d = self.current(fs, key, g)?;
        self.usage(fs, d.id)?; // Refuser une arête inconnue avant la première suppression.
        fs.rename_at(self.root, &slot('r', g), self.root, &slot('d', g))
    }
    fn finish<D: Device>(&self, fs: &mut Engine<D>, g: u64) -> Result<()> {
        self.check_root(fs)?;
        let tomb = slot('d', g);
        let marker = fs.lookup_at(self.root, &tomb)?;
        let key = self.owner(fs, marker, g)?;
        let n = name(&key);
        if let Some(d) = opt(fs, self.root, &n)? {
            if d.id != g || d.kind != Kind::Directory {
                return quarantine();
            }
            self.usage(fs, d.id)?;
            while let Some(f) = next(fs, d.id, &None)? {
                fs.unlink_at(d.id, &f, Kind::File)?
            }
            fs.unlink_at(self.root, &n, Kind::Directory)?;
        }
        fs.unlink_at(self.root, &tomb, Kind::File)?;
        Self::collect(fs)
    }
    fn collect<D: Device>(fs: &mut Engine<D>) -> Result<()> {
        let mut budget = fs.manifest().blocks;
        while fs.collection_pending()? {
            budget = budget
                .checked_sub(1)
                .ok_or(Error::Corrupt("collecte cache sans progrès"))?;
            if !fs.collect()? {
                return Err(Error::Corrupt("collecte cache bloquée"));
            }
        }
        Ok(())
    }
    pub fn purge<D: Device>(&self, fs: &mut Engine<D>, key: &Key, g: u64) -> Result<()> {
        self.available_key(fs, key)?;
        self.begin_purge(fs, key, g)?;
        self.finish(fs, g)
    }
    pub fn recover<D: Device>(&self, fs: &mut Engine<D>) -> Result<Recovery> {
        self.check_root(fs)?;
        let intent = self.intent(fs)?;
        let mut r = Recovery::default();
        let mut cursor = None;
        while let Some(n) = next(fs, self.root, &cursor)? {
            let outcome = if let Some(g) = slot_id(&n, 'd') {
                self.finish(fs, g).map(|()| {
                    r.purged += 1;
                })
            } else if let Some(g) = slot_id(&n, 'r') {
                let m = fs.lookup_at(self.root, &n)?;
                self.owner(fs, m, g)
                    .and_then(|k| self.current(fs, &k, g))
                    .and_then(|d| {
                        if let Some(st) = opt(fs, d.id, INCOMING)? {
                            if st.kind != Kind::File {
                                return quarantine();
                            }
                            fs.unlink_at(d.id, INCOMING, Kind::File)?;
                        }
                        Ok(())
                    })
            } else if n == SEAL {
                Ok(())
            } else {
                let d = fs.lookup_at(self.root, &n)?;
                // Un répertoire en cours de purge sera traité via son marqueur.
                let m =
                    opt(fs, self.root, &slot('r', d.id))?.or(opt(fs, self.root, &slot('d', d.id))?);
                if d.kind == Kind::Directory
                    && let Some(m) = m
                {
                    self.owner(fs, m, d.id)
                        .and_then(|k| if name(&k) == n { Ok(()) } else { quarantine() })
                } else {
                    quarantine()
                }
            };
            match outcome {
                Ok(()) => (),
                Err(Error::Invalid(UNVERIFIED | STALE)) => r.quarantined += 1,
                Err(e) => return Err(e),
            }
            cursor = Some(n)
        }
        if let Some((user, cutoff)) = intent {
            // Une perte de provenance postérieure à l'intention ne prouve
            // pas son achèvement. Garder la portée gelée, signaler la
            // quarantaine, permettre les lectures vérifiées des autres usagers.
            match self
                .preflight_scope(fs, user)
                .and_then(|()| self.sweep(fs, user, cutoff))
            {
                Ok(count) => r.purged += count,
                Err(Error::Invalid(UNVERIFIED | STALE)) => r.quarantined += 1,
                Err(e) => return Err(e),
            }
        }
        if r.quarantined == 0 {
            Self::collect(fs)?;
        }
        Ok(r)
    }
    // Opération administrative : itérer uniquement sur les preuves validées.
    fn preflight_scope<D: Device>(&self, fs: &Engine<D>, user: Option<[u8; 16]>) -> Result<()> {
        let mut cursor = None;
        while let Some(n) = next(fs, self.root, &cursor)? {
            if n != SEAL {
                let entry = fs.lookup_at(self.root, &n)?;
                if let Some(g) = slot_id(&n, 'r') {
                    let k = self.owner(fs, entry, g)?;
                    let d = self.current(fs, &k, g)?;
                    if user.is_none_or(|u| u == k[..16]) {
                        self.usage(fs, d.id)?;
                    }
                } else {
                    // Même un marqueur manquant ou attribué au mauvais
                    // utilisateur ne permet pas d'écarter cette incarnation.
                    if entry.kind != Kind::Directory {
                        return quarantine();
                    }
                    let Some(m) = opt(fs, self.root, &slot('r', entry.id))? else {
                        return quarantine();
                    };
                    if name(&self.owner(fs, m, entry.id)?) != n {
                        return quarantine();
                    }
                }
            }
            cursor = Some(n);
        }
        Ok(())
    }
    pub fn purge_scope<D: Device>(
        &self,
        fs: &mut Engine<D>,
        user: Option<[u8; 16]>,
    ) -> Result<u64> {
        let cutoff = self.begin_scope(fs, user)?;
        self.sweep(fs, user, cutoff)
    }
    pub(crate) fn begin_scope<D: Device>(
        &self,
        fs: &mut Engine<D>,
        user: Option<[u8; 16]>,
    ) -> Result<u64> {
        self.available(fs)?;
        if user == Some([0; 16]) {
            return Err(Error::Invalid("utilisateur de purge"));
        }
        self.preflight_scope(fs, user)?;
        let cutoff = fs.manifest().next_oid;
        let b = self.command(fs, user, cutoff);
        fs.prepare_write_id(self.seal, WIDTH as u64, &b)?.commit()?;
        Ok(cutoff)
    }
    fn sweep<D: Device>(
        &self,
        fs: &mut Engine<D>,
        user: Option<[u8; 16]>,
        cutoff: u64,
    ) -> Result<u64> {
        let mut count = 0;
        let mut cursor = None;
        while let Some(n) = next(fs, self.root, &cursor)? {
            if let Some(g) = slot_id(&n, 'r') {
                let k = self.owner(fs, fs.lookup_at(self.root, &n)?, g)?;
                if g < cutoff && user.is_none_or(|u| u == k[..16]) {
                    self.begin_purge(fs, &k, g)?;
                    self.finish(fs, g)?;
                    count += 1;
                }
            }
            cursor = Some(n)
        }
        fs.prepare_write_id(self.seal, WIDTH as u64, &[0; WIDTH])?
            .commit()?;
        Ok(count)
    }
}
