//! Préparation privée de la branche droite pour les extents d'un nouvel objet.
use super::*;

/// Parcours borné. Une feuille est décodée une fois pour toute la fenêtre.
pub(crate) fn intervalle(
    s: &impl ReadTree,
    racine: Ref,
    debut: &[u8; 16],
    fin: &[u8; 16],
    mut lire: impl FnMut(&[u8], &[u8]) -> Result<()>,
) -> Result<()> {
    fn visiter(
        s: &impl ReadTree,
        n: &Node,
        debut: &[u8],
        fin: &[u8],
        borne: Option<&[u8]>,
        lire: &mut impl FnMut(&[u8], &[u8]) -> Result<()>,
    ) -> Result<()> {
        if borne.is_some_and(|k| n.family.cmp(&n.entries.last().unwrap().0, k) != Ordering::Less) {
            return Err(Error::Corrupt("sous-arbre hors intervalle"));
        }
        if n.level == 0 {
            for (k, v) in &n.entries[n.slot(debut)..n.slot(fin)] {
                lire(k, v)?;
            }
        } else {
            for i in n.child_slot(debut)..n.slot(fin) {
                let (_, fils) = load_child(s, n, i)?;
                let limite = n.entries.get(i + 1).map(|e| e.0.as_slice()).or(borne);
                visiter(s, &fils, debut, fin, limite, lire)?;
            }
        }
        Ok(())
    }
    let f = Family::Extents;
    if racine != Ref::ZERO && f.cmp(debut, fin) == Ordering::Less {
        visiter(s, &s.read(racine, f)?, debut, fin, None, &mut lire)?;
    }
    Ok(())
}

// Du sommet à la feuille. La dernière référence d'un parent désigne le fils
// ouvert. Sa valeur définitive est installée avant toute sérialisation.
pub(crate) struct Ajout {
    branche: Vec<(Option<Ref>, Node)>,
}

impl Ajout {
    pub(crate) fn ouvrir(s: &impl ReadTree, racine: Ref) -> Result<Self> {
        let mut branche = Vec::new();
        branche
            .try_reserve_exact(usize::from(MAX_LEVEL) + 1)
            .map_err(|_| Error::NoMemory)?;
        let f = Family::Extents;
        if racine == Ref::ZERO {
            branche.push((
                None,
                Node {
                    family: f,
                    level: 0,
                    entries: Vec::new(),
                },
            ));
        } else {
            let (mut ancien, mut n) = (racine, s.read(racine, f)?);
            while n.level != 0 {
                let fils = load_child(s, &n, n.entries.len() - 1)?;
                branche.push((Some(ancien), n));
                (ancien, n) = fils;
            }
            branche.push((Some(ancien), n));
        }
        Ok(Self { branche })
    }

    pub(crate) fn ajouter(
        &mut self,
        s: &mut impl WriteTree,
        cle: Vec<u8>,
        valeur: Vec<u8>,
    ) -> Result<()> {
        let mut i = self.branche.len() - 1;
        let f = Family::Extents;
        let feuille = &mut self.branche[i].1;
        if !f.valid_key(&cle, false)
            || valeur.len() > MAX_VALUE
            || feuille
                .entries
                .last()
                .is_some_and(|e| f.cmp(&e.0, &cle) != Ordering::Less)
        {
            return Err(Error::Corrupt("ajout d'extent hors ordre"));
        }
        feuille
            .entries
            .try_reserve(1)
            .map_err(|_| Error::NoMemory)?;
        feuille.entries.push((cle, valeur));
        while self.branche[i].1.encoded_len() > PAGE {
            let (ancien, n) = &mut self.branche[i];
            let level = n.level;
            if i == 0 && level == MAX_LEVEL {
                return Err(Error::Limit("hauteur du B+Tree"));
            }
            // pack impose les minima P9 et produit deux nœuds pour un débordement.
            let vide = Node {
                family: f,
                level,
                entries: Vec::new(),
            };
            let mut deux = pack(core::mem::replace(n, vide))?;
            *n = deux.pop().ok_or(Error::Corrupt("scission sans voisin"))?;
            let gauche = deux.pop().ok_or(Error::Corrupt("scission sans origine"))?;
            let r = s.store(ancien.take(), &gauche)?;
            let minimum = copy_field(&n.entries[0].0)?;
            if i == 0 {
                let parent = Node {
                    family: f,
                    level: level + 1,
                    entries: vec![
                        (copy_field(&gauche.entries[0].0)?, r.to_bytes().to_vec()),
                        (minimum, Ref::ZERO.to_bytes().to_vec()),
                    ],
                };
                self.branche.insert(0, (None, parent));
                break;
            }
            i -= 1;
            let parent = &mut self.branche[i].1;
            parent.entries.try_reserve(1).map_err(|_| Error::NoMemory)?;
            parent
                .entries
                .last_mut()
                .unwrap()
                .1
                .copy_from_slice(&r.to_bytes());
            parent
                .entries
                .push((minimum, Ref::ZERO.to_bytes().to_vec()));
        }
        Ok(())
    }

    pub(crate) fn terminer(self, s: &mut impl WriteTree) -> Result<Ref> {
        let mut fils: Option<Ref> = None;
        for (ancien, mut n) in self.branche.into_iter().rev() {
            if let Some(r) = fils {
                n.entries
                    .last_mut()
                    .unwrap()
                    .1
                    .copy_from_slice(&r.to_bytes());
            }
            fils = Some(s.store(ancien, &n)?);
        }
        fils.ok_or(Error::Corrupt("branche sans racine"))
    }
}
