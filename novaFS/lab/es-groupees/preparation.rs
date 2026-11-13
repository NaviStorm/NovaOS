//! Profil hôte isolé. Candidate neuve et insertion d'un octet ASCII en tête.
//! Chaque page émise avant F1 est réservée dans le fork privé de Space.
use super::*;
use crate::{Block, codec::data_hash, time::Atime};
use zeroize::Zeroizing;

pub trait BlocGroupe: Device {
    fn lire_lot(&self, blocs: &[u64], pages: &mut [Block]) -> Result<()>;
    fn ecrire_lot(&self, blocs: &[u64], pages: &[Block]) -> Result<()>;
}

const PAGES: usize = 128;

// Série refusée hors du profil de texte testé, avant toute publication du nom.
// UTF-8 et CRLF peuvent chevaucher une page ; le profil ASCII/LF est explicite.
fn texte(s: &[u8]) -> bool {
    s.iter().all(|&b| matches!(b, b' '..=b'~' | b'\n' | b'\t'))
}

impl<D: BlocGroupe> Engine<D> {
    pub fn sauvegarde_labo(
        &mut self,
        source: Stat,
        parent: u64,
        nom: &str,
        octet: u8,
    ) -> Result<()> {
        if self.atime_policy().is_none() {
            return Err(Error::Invalid("volume P9 requis"));
        }
        if nom.contains('/') || !valid_name(nom) || !texte(&[octet]) {
            return Err(Error::Invalid("nom ou insertion de laboratoire"));
        }
        self.observe(self.precharger_extents())?;
        self.transact(false, |tx| {
            let mut origine = by_id(tx, source.id)?;
            if origine.stat != source || source.kind != Kind::File {
                return Err(Error::Invalid("révision source"));
            }
            let dossier = by_id(tx, parent)?;
            if dossier.stat.kind != Kind::Directory || depth(tx, &dossier, None)? >= MAX_DEPTH {
                return Err(Error::Invalid("répertoire destination"));
            }
            let key = name_key(parent, nom);
            if get(tx, Family::Names, &key)?.is_some() {
                return Err(Error::Exists);
            }
            let taille = source
                .size
                .checked_add(1)
                .filter(|&v| v <= i64::MAX as u64)
                .ok_or(Error::Limit("taille candidate"))?;
            let id = tx.manifest().next_oid;
            tx.manifest_mut().next_oid = id.checked_add(1).ok_or(Error::Limit("id"))?;
            tx.resize_logical(0, taille)?;
            let naissance = generation(tx)?;
            let ids = tx.ids();
            let mut objet = Object::new(id, Kind::File, parent, nom, naissance);
            objet.times = crate::time::Times::created(tx.now);
            let mut pages = Zeroizing::new(Vec::<Block>::new());
            pages
                .try_reserve_exact(PAGES)
                .map_err(|_| Error::NoMemory)?;
            let mut refs = Vec::new();
            let mut adresses = Vec::new();
            refs.try_reserve_exact(PAGES).map_err(|_| Error::NoMemory)?;
            adresses
                .try_reserve_exact(PAGES)
                .map_err(|_| Error::NoMemory)?;
            let mut premiere = 0;
            let mut retenu = octet;
            let total = taille.div_ceil(BLOCK_BYTES);
            let pages_source = source.size.div_ceil(BLOCK_BYTES);
            while premiere < total {
                let n = (total - premiere).min(PAGES as u64) as usize;
                let a_lire = pages_source.saturating_sub(premiere).min(n as u64) as usize;
                pages.resize(n, [0; 4096]);
                refs.clear();
                while refs.len() < a_lire {
                    let index = premiere + refs.len() as u64;
                    let (debut, run) = data::covering(tx, source.id, index)?
                        .ok_or(Error::Invalid("source creuse hors profil"))?;
                    let start = (index - debut) as usize;
                    let count = (a_lire - refs.len()).min(run.hashes.len() - start);
                    refs.extend((start..start + count).map(|i| run.reference(i)));
                }
                adresses.clear();
                adresses.extend(refs.iter().map(|r| r.block));
                if a_lire != 0 {
                    tx.stream_device()
                        .lire_lot(&adresses, &mut pages[..a_lire])?;
                }
                adresses.clear();
                for (j, page) in pages.iter_mut().enumerate() {
                    let index = premiere + j as u64;
                    if let Some(r) = refs.get(j) {
                        let valable = (source.size - index * BLOCK_BYTES).min(BLOCK_BYTES) as usize;
                        if data_hash(ids, r.block, r.birth, page) != r.hash
                            || page[valable..].iter().any(|&b| b != 0)
                        {
                            return Err(Error::Corrupt("page source du lot"));
                        }
                        if !texte(&page[..valable]) {
                            return Err(Error::Invalid("texte hors profil"));
                        }
                    } else {
                        page.fill(0);
                    }
                    let suivant = page[4095];
                    page.copy_within(..4095, 1);
                    page[0] = retenu;
                    retenu = suivant;
                    adresses.push(tx.reserve_stream_page()?);
                }
                tx.stream_device().ecrire_lot(&adresses, &pages)?;
                let mut j = 0;
                while j < n {
                    let first = j;
                    let mut hashes = Vec::new();
                    hashes
                        .try_reserve_exact(MAX_RUN)
                        .map_err(|_| Error::NoMemory)?;
                    loop {
                        hashes.push(data_hash(ids, adresses[j], naissance, &pages[j]));
                        j += 1;
                        if j == n || j - first == MAX_RUN || adresses[j] != adresses[j - 1] + 1 {
                            break;
                        }
                    }
                    let run = Run {
                        first: adresses[first],
                        birth: naissance,
                        hashes,
                    };
                    tx.put(
                        Family::Extents,
                        extent_key(id, premiere + first as u64).to_vec(),
                        run.encode(),
                    )?;
                }
                premiere += n as u64;
            }
            if source.size != 0
                && let Some(p) = tx.manifest().atime
                && p != Atime::No
                && p.update(origine.times, tx.now)
            {
                origine.times.atime = tx.now;
                origine.stat.revision = naissance;
                save_object(tx, &origine)?;
            }
            objet.stat.size = taille;
            save_object(tx, &objet)?;
            tx.put(Family::Names, key, id.to_le_bytes().to_vec())?;
            touch(tx, parent)
        })
    }
}

/// Précharge les enfants authentifiés de la racine avant de modifier l'arbre.
/// Le parcours normal continue de vérifier niveaux, séparateurs et références.
impl<D: BlocGroupe> Engine<D> {
    fn precharger_extents(&self) -> Result<()> {
        use crate::tree::{Node, ReadTree};
        let f = Family::Extents;
        let r = root(self, f);
        if r == Ref::ZERO {
            return Ok(());
        }
        let node = ReadTree::read(self, r, f)?;
        if node.level != 1 || node.entries.len() > PAGES {
            return Ok(());
        }
        let mut refs = Vec::new();
        refs.try_reserve_exact(node.entries.len())
            .map_err(|_| Error::NoMemory)?;
        for (_, value) in &node.entries {
            let child = Ref::from_bytes(value)?;
            child.validate(self.dev_labo().blocks(), self.generation())?;
            if self.manque_labo(f, child)? {
                refs.push(child);
            }
        }
        if refs.is_empty() {
            return Ok(());
        }
        let mut blocs = Vec::new();
        blocs
            .try_reserve_exact(refs.len())
            .map_err(|_| Error::NoMemory)?;
        blocs.extend(refs.iter().map(|r| r.block));
        let mut pages = Zeroizing::new(Vec::new());
        pages
            .try_reserve_exact(refs.len())
            .map_err(|_| Error::NoMemory)?;
        pages.resize(refs.len(), [0; 4096]);
        self.dev_labo().lire_lot(&blocs, &mut pages)?;
        for (r, page) in refs.into_iter().zip(pages.iter()) {
            Node::decode(page, self.ids(), r, f)?;
            self.cache_labo(f, r, *page)?;
        }
        Ok(())
    }
}
