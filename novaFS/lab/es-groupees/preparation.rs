//! Publication expérimentale d'une candidate neuve par plages de la source épinglée.
//! Chaque page émise avant F1 est réservée dans le fork privé de Space.
use super::*;
use crate::{Block, codec::data_hash, time::Atime};
use zeroize::Zeroizing;

pub trait BlocGroupe: Device {
    fn lire_lot(&self, blocs: &[u64], pages: &mut [Block]) -> Result<()>;
    fn ecrire_lot(&self, blocs: &[u64], pages: &[Block]) -> Result<()>;
}

const PAGES: usize = 256;

// Les offsets concernent la source immuable. Les littéraux sont concaténés
// dans l'ordre des plages et lus à la demande dans le transport scellé.
type Plage = [u64; 3];

#[derive(Default)]
struct Texte {
    reste: [u8; 4],
    n: usize,
    cr: bool,
}
impl Texte {
    fn caracteres(&mut self, s: &str) -> Result<()> {
        for c in s.chars() {
            if (self.cr && c != '\n') || (c.is_control() && !matches!(c, '\n' | '\r' | '\t')) {
                return Err(Error::Invalid("contrôle dans le texte"));
            }
            self.cr = c == '\r';
        }
        Ok(())
    }
    fn lire(&mut self, mut s: &[u8]) -> Result<()> {
        // Huit octets imprimables ASCII sont huit scalaires UTF-8 valides.
        // Le bit de poids fort distingue le chemin Unicode. Les deux bits
        // suivants détectent un octet inférieur à 0x20. Une addition de 1
        // par octet ASCII (sans retenue) détecte les octets DEL à 0x7f.
        if self.n == 0 && !self.cr {
            const HAUT: u64 = 0x8080808080808080;
            const UN: u64 = 0x0101010101010101;
            let mut mots = s.chunks_exact(8);
            let rapide = mots.by_ref().all(|octets| {
                let v = u64::from_ne_bytes(octets.try_into().unwrap());
                let bas = !((v << 1) | (v << 2));
                if (v | (v.wrapping_add(UN)) | bas) & HAUT == 0 {
                    true
                } else {
                    octets
                        .iter()
                        .all(|b| matches!(b, b' '..=b'~' | b'\n' | b'\t'))
                }
            });
            if rapide
                && mots
                    .remainder()
                    .iter()
                    .all(|b| matches!(b, b' '..=b'~' | b'\n' | b'\t'))
            {
                return Ok(());
            }
        }
        while self.n != 0 && !s.is_empty() {
            self.reste[self.n] = s[0];
            self.n += 1;
            s = &s[1..];
            let tampon = self.reste;
            match core::str::from_utf8(&tampon[..self.n]) {
                Ok(v) => {
                    self.caracteres(v)?;
                    self.n = 0;
                }
                Err(e) if e.error_len().is_none() && self.n < 4 => (),
                Err(_) => return Err(Error::Invalid("UTF-8")),
            }
        }
        match core::str::from_utf8(s) {
            Ok(v) => self.caracteres(v),
            Err(e) if e.error_len().is_none() => {
                let (bon, fin) = s.split_at(e.valid_up_to());
                self.caracteres(core::str::from_utf8(bon).map_err(|_| Error::Invalid("UTF-8"))?)?;
                self.n = fin.len();
                self.reste[..self.n].copy_from_slice(fin);
                Ok(())
            }
            _ => Err(Error::Invalid("UTF-8")),
        }
    }
    fn fin(&self) -> Result<()> {
        if self.n == 0 && !self.cr {
            Ok(())
        } else {
            Err(Error::Invalid("fin de texte incomplète"))
        }
    }
}

fn lot(octets: u64) -> Result<Zeroizing<Vec<Block>>> {
    let n = octets.div_ceil(4096).min(PAGES as u64) as usize;
    let mut pages = Zeroizing::new(Vec::new());
    pages.try_reserve_exact(n).map_err(|_| Error::NoMemory)?;
    pages.resize(n, [0; 4096]);
    Ok(pages)
}

struct Candidate {
    pages: Zeroizing<Vec<Block>>,
    utilise: usize,
    premiere: u64,
    texte: Texte,
    id: u64,
    naissance: u64,
}
impl Candidate {
    fn vider<D: BlocGroupe>(&mut self, tx: &mut Tx<D>) -> Result<()> {
        let n = self.utilise.div_ceil(4096);
        if n == 0 {
            return Ok(());
        }
        let pages = &mut self.pages[..n];
        let fin = self.utilise % 4096;
        if fin != 0 {
            pages[n - 1][fin..].fill(0);
        }
        let mut adresses = [0u64; PAGES];
        for adresse in &mut adresses[..n] {
            *adresse = tx.reserve_stream_page()?;
        }
        tx.stream_device().ecrire_lot(&adresses[..n], pages)?;
        let mut i = 0;
        while i < n {
            let debut = i;
            let mut hashes = Vec::new();
            hashes
                .try_reserve_exact(MAX_RUN)
                .map_err(|_| Error::NoMemory)?;
            loop {
                hashes.push(data_hash(tx.ids(), adresses[i], self.naissance, &pages[i]));
                i += 1;
                if i == n || i - debut == MAX_RUN || adresses[i] != adresses[i - 1] + 1 {
                    break;
                }
            }
            let run = Run {
                first: adresses[debut],
                birth: self.naissance,
                hashes,
            };
            tx.put(
                Family::Extents,
                extent_key(self.id, self.premiere + debut as u64).to_vec(),
                run.encode(),
            )?;
        }
        self.premiere += n as u64;
        self.utilise = 0;
        Ok(())
    }
    fn ecrire<D: BlocGroupe>(
        &mut self,
        tx: &mut Tx<D>,
        mut n: u64,
        mut lire: impl FnMut(&mut [u8]) -> Result<()>,
    ) -> Result<()> {
        while n > 0 {
            let place = &mut self.pages.as_mut_slice().as_flattened_mut()[self.utilise..];
            let taille = n.min(place.len() as u64) as usize;
            lire(&mut place[..taille])?;
            self.texte.lire(&place[..taille])?;
            self.utilise += taille;
            n -= taille as u64;
            if self.utilise == self.pages.len() * 4096 {
                self.vider(tx)?;
            }
        }
        Ok(())
    }
}

impl<D: BlocGroupe> Engine<D> {
    pub fn sauvegarde_labo(
        &mut self,
        source: Stat,
        parent: u64,
        nom: &str,
        octet: u8,
    ) -> Result<()> {
        self.sauvegarde_plages(Some(source), parent, nom, &[[0, 0, 1]], 1, |_, buf| {
            if !buf.is_empty() {
                buf.copy_from_slice(&[octet]);
            }
            Ok(())
        })
    }

    pub fn sauvegarde_plages(
        &mut self,
        source: Option<Stat>,
        parent: u64,
        nom: &str,
        plages: &[Plage],
        litteraux: u64,
        mut lire: impl FnMut(u64, &mut [u8]) -> Result<()>,
    ) -> Result<()> {
        if self.atime_policy().is_none() || nom.contains('/') || !valid_name(nom) {
            return Err(Error::Invalid("volume ou nom"));
        }
        let longueur = source.map_or(0, |s| s.size);
        let (mut fin, mut ajouts, mut retraits) = (0, 0u64, 0u64);
        for &[at, retire, ajoute] in plages {
            if at < fin || at > longueur || retire > longueur - at || (retire == 0 && ajoute == 0) {
                return Err(Error::Invalid("plage source"));
            }
            fin = at + retire;
            retraits += retire;
            ajouts = ajouts
                .checked_add(ajoute)
                .ok_or(Error::Limit("littéraux"))?;
        }
        if ajouts != litteraux {
            return Err(Error::Invalid("longueur littérale"));
        }
        let taille = (longueur - retraits)
            .checked_add(ajouts)
            .filter(|&v| v <= i64::MAX as u64)
            .ok_or(Error::Limit("taille candidate"))?;
        self.observe(self.precharger_extents())?;
        self.transact(false, |tx| {
            let mut origine = source
                .map(|st| {
                    let o = by_id(tx, st.id)?;
                    if o.stat != st || st.kind != Kind::File {
                        return Err(Error::Invalid("révision source"));
                    }
                    Ok(o)
                })
                .transpose()?;
            let dossier = by_id(tx, parent)?;
            if dossier.stat.kind != Kind::Directory || depth(tx, &dossier, None)? >= MAX_DEPTH {
                return Err(Error::Invalid("répertoire destination"));
            }
            let key = name_key(parent, nom);
            if get(tx, Family::Names, &key)?.is_some() {
                return Err(Error::Exists);
            }
            let id = tx.manifest().next_oid;
            tx.manifest_mut().next_oid = id.checked_add(1).ok_or(Error::Limit("id"))?;
            tx.resize_logical(0, taille)?;
            let naissance = generation(tx)?;
            let mut sortie = Candidate {
                pages: lot(taille)?,
                utilise: 0,
                premiere: 0,
                texte: Texte::default(),
                id,
                naissance,
            };
            let mut entree = lot(longueur)?;
            let mut refs = Vec::new();
            refs.try_reserve_exact(entree.len())
                .map_err(|_| Error::NoMemory)?;
            let mut adresses = [0u64; PAGES];
            let mut texte = Texte::default();
            let (mut plage, mut literal, mut ignorer) = (0, 0, 0);
            let mut pos = 0;
            loop {
                let n = (longueur - pos).min((PAGES * 4096) as u64) as usize;
                let np = n.div_ceil(4096);
                refs.clear();
                if let Some(st) = source {
                    while refs.len() < np {
                        let index = pos / BLOCK_BYTES + refs.len() as u64;
                        let (start, run) = data::covering(tx, st.id, index)?
                            .ok_or(Error::Invalid("source creuse hors profil"))?;
                        let j = (index - start) as usize;
                        let k = (np - refs.len()).min(run.hashes.len() - j);
                        refs.extend((j..j + k).map(|i| run.reference(i)));
                    }
                }
                if np != 0 {
                    for (a, r) in adresses.iter_mut().zip(&refs) {
                        *a = r.block;
                    }
                    tx.stream_device()
                        .lire_lot(&adresses[..np], &mut entree[..np])?;
                    for (r, page) in refs.iter().zip(&entree[..np]) {
                        if data_hash(tx.ids(), r.block, r.birth, page) != r.hash {
                            return Err(Error::Corrupt("page source du lot"));
                        }
                    }
                    let pad = n % 4096;
                    if pad != 0 && entree[np - 1][pad..].iter().any(|&x| x != 0) {
                        return Err(Error::Corrupt("bourrage source"));
                    }
                }
                let data = &entree.as_slice().as_flattened()[..n];
                texte.lire(data)?;
                let fin = pos + n as u64;
                loop {
                    pos = ignorer.min(fin).max(pos);
                    let prochaine = plages.get(plage);
                    let limite = prochaine.map_or(longueur, |p| p[0]).min(fin);
                    if pos < limite {
                        let mut first = n - (fin - pos) as usize;
                        sortie.ecrire(tx, limite - pos, |buf| {
                            buf.copy_from_slice(&data[first..first + buf.len()]);
                            first += buf.len();
                            Ok(())
                        })?;
                        pos = limite;
                    }
                    let Some(&[at, retire, ajoute]) = prochaine.filter(|p| p[0] == pos) else {
                        break;
                    };
                    sortie.ecrire(tx, ajoute, |buf| {
                        lire(literal, buf)?;
                        literal += buf.len() as u64;
                        Ok(())
                    })?;
                    ignorer = at + retire;
                    plage += 1;
                }
                if pos == longueur {
                    break;
                }
            }
            texte.fin()?;
            sortie.texte.fin()?;
            sortie.vider(tx)?;
            // Point d’admission final après la dernière lecture du prêt et avant F1.
            lire(literal, &mut [])?;
            if let Some(o) = &mut origine
                && longueur != 0
                && let Some(p) = tx.manifest().atime
                && p != Atime::No
                && p.update(o.times, tx.now)
            {
                o.times.atime = tx.now;
                o.stat.revision = naissance;
                save_object(tx, o)?;
            }
            let mut objet = Object::new(id, Kind::File, parent, nom, naissance);
            objet.times = crate::time::Times::created(tx.now);
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

#[cfg(test)]
mod tests_texte {
    use super::Texte;

    #[test]
    fn ascii_utf8_aux_frontieres_des_mots() {
        for v in 0..=255u8 {
            for i in 0..8 {
                let mut mot = [b'a'; 8];
                mot[i] = v;
                let attendu = matches!(v, b' '..=b'~' | b'\n' | b'\t');
                let mut t = Texte::default();
                assert_eq!(
                    t.lire(&mot).and_then(|()| t.fin()).is_ok(),
                    attendu,
                    "{v:02x} à {i}"
                );
            }
        }
        let s = "abç中🦊\r\n\ttexte".as_bytes();
        for i in 0..=s.len() {
            let mut t = Texte::default();
            t.lire(&s[..i]).unwrap();
            t.lire(&s[i..]).unwrap();
            t.fin().unwrap();
        }
    }
}
