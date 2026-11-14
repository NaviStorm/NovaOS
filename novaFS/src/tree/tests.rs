use super::*;
use std::collections::{BTreeMap, BTreeSet};

const IDS: Ids = Ids {
    fs: [0x11; 16],
    lv: [0x22; 16],
};

struct Store {
    next: u64,
    birth: u64,
    disk: BTreeMap<u64, Block>,
}

impl Store {
    fn new() -> Self {
        Self {
            next: 2,
            birth: 1,
            disk: BTreeMap::new(),
        }
    }
}
impl ReadTree for Store {
    fn read(&self, r: Ref, family: Family) -> Result<Node> {
        Node::decode(
            self.disk.get(&r.block).ok_or(Error::Corrupt("adresse"))?,
            IDS,
            r,
            family,
        )
    }
}
impl WriteTree for Store {
    fn store(&mut self, old: Option<Ref>, node: &Node) -> Result<Ref> {
        let address = if let Some(r) = old.filter(|r| r.birth == self.birth) {
            r.block
        } else {
            let address = self.next;
            self.next += 1;
            address
        };
        let (r, b) = node.encode(IDS, address, self.birth)?;
        self.disk.insert(address, b);
        Ok(r)
    }
    fn retire(&mut self, r: Ref) -> Result<()> {
        if r.birth == self.birth {
            self.disk.remove(&r.block);
        }
        Ok(())
    }
}

fn name(i: u64) -> Vec<u8> {
    let text = format!("{i:06x}{}", "a".repeat(249));
    let mut key = 11u64.to_le_bytes().to_vec();
    key.extend(255u16.to_le_bytes());
    key.extend(text.bytes());
    key
}

#[test]
fn versions_immuables_scissions_fusions_contraction() {
    let f = Family::Names;
    let mut s = Store::new();
    let mut root = Ref::ZERO;
    for i in 0..900u64 {
        root = put(&mut s, root, f, &name((i * 331) % 900), &i.to_le_bytes()).unwrap();
    }
    let level = s.read(root, f).unwrap().level;
    assert!(level >= 2);
    let previous = root;
    let allocated = s.next;
    s.birth += 1;
    root = put(&mut s, root, f, &name(205), &123456u64.to_le_bytes()).unwrap();
    assert_eq!(s.next - allocated, u64::from(level) + 1);
    assert_ne!(
        get(&s, previous, f, &name(205)).unwrap(),
        get(&s, root, f, &name(205)).unwrap()
    );
    let mut visited = BTreeSet::new();
    let mut names = Vec::new();
    walk(
        &s,
        root,
        f,
        |r| {
            assert!(visited.insert(r.block));
            Ok(())
        },
        |k, _| {
            names.push(k.to_vec());
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(names, (0..900).map(name).collect::<Vec<_>>());
    for i in 0..900u64 {
        root = remove(&mut s, root, f, &name((i * 263) % 900)).unwrap();
        if i % 113 == 0 {
            walk(&s, root, f, |_| Ok(()), |_, _| Ok(())).unwrap();
        }
    }
    assert_eq!(root, Ref::ZERO);
    let mut old_count = 0;
    walk(
        &s,
        previous,
        f,
        |_| Ok(()),
        |_, _| {
            old_count += 1;
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(old_count, 900);
}

#[test]
fn operations_et_bornes_differentielles() {
    let mut s = Store::new();
    let mut root = Ref::ZERO;
    let f = Family::Objects;
    let mut oracle = BTreeMap::<u64, u64>::new();
    let mut rng = 13u64;
    for step in 0..1500u64 {
        rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1);
        let key = (rng >> 32) % 540;
        let wire = key.to_le_bytes();
        if rng % 4 == 0 {
            oracle.remove(&key);
            root = remove(&mut s, root, f, &wire).unwrap();
        } else {
            oracle.insert(key, step);
            root = put(&mut s, root, f, &wire, &step.to_le_bytes()).unwrap();
        }
        let q = (rng >> 20) % 556;
        let wire = q.to_le_bytes();
        let native = |p: Record| (le64(&p.0), le64(&p.1));
        assert_eq!(
            floor(&s, root, f, &wire).unwrap().map(native),
            oracle.range(..=q).next_back().map(|(&k, &v)| (k, v))
        );
        assert_eq!(
            lower_bound(&s, root, f, &wire).unwrap().map(native),
            oracle.range(q..).next().map(|(&k, &v)| (k, v))
        );
        assert_eq!(
            next(&s, root, f, &wire).unwrap().map(native),
            oracle.range(q + 1..).next().map(|(&k, &v)| (k, v))
        );
    }
}

#[test]
fn noeuds_hostiles_resignes() {
    let node = Node {
        family: Family::Objects,
        level: 0,
        entries: vec![
            (1u64.to_le_bytes().to_vec(), vec![7]),
            (2u64.to_le_bytes().to_vec(), vec![8]),
        ],
    };
    let (_, bytes) = node.encode(IDS, 54, 2).unwrap();
    for (offset, bad) in [
        (98, 8),
        (99, 1),
        (100, 255),
        (102, 1),
        (104, 150),
        (108, 255),
        (4095, 17),
    ] {
        let mut corrupt = bytes;
        corrupt[offset] = bad;
        let (r, corrupt) = finish_meta(corrupt, TREE, IDS, 54, 2).unwrap();
        assert!(
            Node::decode(&corrupt, IDS, r, Family::Objects).is_err(),
            "offset {offset}"
        );
    }
    let mut bad = bytes;
    // La seconde cellule débute à 121 ; le remplacement de sa clé par la
    // première fabrique un doublon à structure et à empreinte valides.
    bad[125..133].copy_from_slice(&1u64.to_le_bytes());
    let (r, b) = finish_meta(bad, TREE, IDS, 54, 2).unwrap();
    assert!(Node::decode(&b, IDS, r, Family::Objects).is_err());

    let mut s = Store::new();
    let child = s.store(None, &node).unwrap();
    let wrong = Node {
        family: Family::Objects,
        level: 1,
        entries: vec![
            (0u64.to_le_bytes().to_vec(), child.to_bytes().to_vec()),
            (3u64.to_le_bytes().to_vec(), child.to_bytes().to_vec()),
        ],
    };
    let root = s.store(None, &wrong).unwrap();
    assert!(walk(&s, root, Family::Objects, |_| Ok(()), |_, _| Ok(())).is_err());
}

#[test]
fn cle_profonde_ne_deborde_pas_sur_le_sous_arbre_voisin() {
    fn node(level: u8, pairs: Vec<(u64, Vec<u8>)>) -> Node {
        Node {
            family: Family::Objects,
            level,
            entries: pairs
                .into_iter()
                .map(|(k, v)| (k.to_le_bytes().to_vec(), v))
                .collect(),
        }
    }
    let mut store = Store::new();
    let leaf = |s: &mut Store, keys: &[u64]| {
        s.store(None, &node(0, keys.iter().map(|&k| (k, vec![1])).collect()))
            .unwrap()
            .to_bytes()
            .to_vec()
    };
    let a = leaf(&mut store, &[1]);
    let b = leaf(&mut store, &[2, 99]);
    let c = leaf(&mut store, &[5]);
    let d = leaf(&mut store, &[6]);
    let left = store.store(None, &node(1, vec![(1, a), (2, b)])).unwrap();
    let right = store.store(None, &node(1, vec![(5, c), (6, d)])).unwrap();
    let root = store
        .store(
            None,
            &node(
                2,
                vec![
                    (1, left.to_bytes().to_vec()),
                    (5, right.to_bytes().to_vec()),
                ],
            ),
        )
        .unwrap();
    // Chaque nœud local et chaque digest sont valides. La clé 99 de la feuille
    // gauche dépasse pourtant la borne 5 qui n'existe que dans son grand-parent.
    assert!(walk(&store, root, Family::Objects, |_| Ok(()), |_, _| Ok(())).is_err());
}

#[cfg(all(feature = "prototype-es-groupees", not(target_os = "none")))]
#[test]
fn ajout_extents_sur_plusieurs_niveaux_et_lecture_par_intervalle() {
    fn cle(i: u64) -> [u8; 16] {
        let mut k = [0; 16];
        k[..8].copy_from_slice(&3u64.to_le_bytes());
        k[8..].copy_from_slice(&i.to_le_bytes());
        k
    }
    fn valeur(i: u64) -> Vec<u8> {
        let mut v = vec![i as u8; 18 + 16 * 32];
        v[..8].copy_from_slice(&(2 + 16 * i).to_le_bytes());
        v[8..10].copy_from_slice(&16u16.to_le_bytes());
        v[10..18].copy_from_slice(&1u64.to_le_bytes());
        v
    }
    let f = Family::Extents;
    let mut s = Store::new();
    let mut racine = Ref::ZERO;
    for i in 0..45 {
        racine = put(&mut s, racine, f, &cle(i), &valeur(i)).unwrap();
    }
    let ancienne = racine;
    s.birth += 1;
    let mut ajout = ajout::Ajout::ouvrir(&s, racine).unwrap();
    assert!(ajout.ajouter(&mut s, cle(44).to_vec(), valeur(44)).is_err());
    for i in 45..950 {
        ajout.ajouter(&mut s, cle(i).to_vec(), valeur(i)).unwrap();
    }
    racine = ajout.terminer(&mut s).unwrap();
    assert!(s.read(racine, f).unwrap().level >= 2);
    for (r, n) in [(ancienne, 45), (racine, 950)] {
        let mut i = 0;
        walk(
            &s,
            r,
            f,
            |_| Ok(()),
            |k, v| {
                assert_eq!((k, v), (cle(i).as_slice(), valeur(i).as_slice()));
                i += 1;
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(i, n);
    }
    for (lo, hi) in [(0, 1), (27, 901), (949, 960), (950, 999), (3, 3)] {
        let mut i = lo;
        ajout::intervalle(&s, racine, &cle(lo), &cle(hi), |k, v| {
            assert_eq!((k, v), (cle(i).as_slice(), valeur(i).as_slice()));
            i += 1;
            Ok(())
        })
        .unwrap();
        assert_eq!(i, hi.min(950));
    }
    // Sceller à nouveau une feuille dont la dernière clé dépasse le voisin du
    // grand-parent. Les références complètes et les minima restent vérifiables.
    let mut sommet = s.read(racine, f).unwrap();
    assert_eq!(sommet.level, 2);
    let (_, mut gauche) = load_child(&s, &sommet, 0).unwrap();
    let dernier = gauche.entries.len() - 1;
    let (_, mut feuille) = load_child(&s, &gauche, dernier).unwrap();
    assert!(feuille.entries.len() > 1);
    feuille.entries.last_mut().unwrap().0 = sommet.entries[1].0.clone();
    let ref_feuille = s.store(None, &feuille).unwrap();
    gauche.entries[dernier].1 = ref_feuille.to_bytes().to_vec();
    let ref_gauche = s.store(None, &gauche).unwrap();
    sommet.entries[0].1 = ref_gauche.to_bytes().to_vec();
    let incoherente = s.store(None, &sommet).unwrap();
    assert!(ajout::intervalle(&s, incoherente, &cle(0), &cle(950), |_, _| Ok(())).is_err());
}
