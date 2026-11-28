use super::*;
use alloc::{vec, vec::Vec};

const fn id(bus: u8, dev: u8) -> Adresse {
    Adresse {
        segment: 9,
        rid: ((bus as u16) << 8) | ((dev as u16) << 3),
    }
}
const PONT: Adresse = id(0, 2);
const USB: Adresse = id(4, 3);
const ALIAS: Adresse = Adresse {
    segment: 9,
    rid: USB.rid + 1,
};
const NIC: Adresse = id(0, 9);
const SATA: Adresse = id(0, 12);
const U1: u64 = 0x120000;
const U2: u64 = 0x340000;
const Z: u64 = 0x800000;

fn pci(a: Adresse, classe: u32) -> Fonction {
    Fonction {
        adresse: a,
        titulaire: Some(a),
        classe,
        pont: None,
    }
}
fn fonctions() -> Vec<Fonction> {
    vec![
        Fonction {
            pont: Some((4, 4)),
            ..pci(PONT, 0x060400)
        },
        pci(USB, 0x0c0330),
        Fonction {
            titulaire: Some(USB),
            ..pci(ALIAS, 0xff0000)
        },
        pci(NIC, 0x020000),
        pci(SATA, 0x010601),
    ]
}
fn scope(kind: u8, bus: u8, path: &[[u8; 2]]) -> Vec<u8> {
    let mut b = vec![kind, (6 + path.len() * 2) as u8, 0, 0, 0, bus];
    for step in path {
        b.extend(step);
    }
    b
}
fn record(typ: u16, flags: u8, seg: u16, base: u64, fin: Option<u64>, scope: &[u8]) -> Vec<u8> {
    let mut b = vec![0; 8];
    b[..2].copy_from_slice(&typ.to_le_bytes());
    b[4] = flags;
    b[6..8].copy_from_slice(&seg.to_le_bytes());
    b.extend(base.to_le_bytes());
    if let Some(l) = fin {
        b.extend(l.to_le_bytes());
    }
    b.extend(scope);
    let len = b.len() as u16;
    b[2..4].copy_from_slice(&len.to_le_bytes());
    b
}
fn explicite() -> Vec<u8> {
    record(0, 0, 9, U1, None, &scope(2, 0, &[[2, 0]]))
}
fn defaut() -> Vec<u8> {
    record(0, 1, 9, U2, None, &[])
}
fn table(rows: &[Vec<u8>]) -> Vec<u8> {
    let mut t = vec![0; 48];
    t[..4].copy_from_slice(b"DMAR");
    t[8] = 1;
    t[36] = 47;
    for row in rows {
        t.extend(row);
    }
    let n = t.len() as u32;
    t[4..8].copy_from_slice(&n.to_le_bytes());
    checksum(&mut t);
    t
}
fn checksum(t: &mut [u8]) {
    t[9] = 0;
    t[9] = 0u8.wrapping_sub(t.iter().fold(0u8, |s, b| s.wrapping_add(*b)));
}
fn domaine(unite: u64, did: u16) -> Domaine {
    Domaine { unite, did }
}
fn page(debut: u64) -> Plage {
    Plage {
        debut,
        octets: 4096,
    }
}
fn bail(a: Adresse, p: u64, d: Droits) -> Bail {
    Bail {
        titulaire: a,
        traduction: Traduction {
            iova: p,
            ram: page(p),
            droits: d,
        },
    }
}
struct Etat {
    u: Vec<EtatUnite>,
    c: Vec<Contexte>,
    m: Vec<Mapping>,
    sata: EtatSata,
}
impl Etat {
    fn vue(&self) -> Cliche<'_> {
        Cliche {
            unites: &self.u,
            contextes: &self.c,
            mappings: &self.m,
            sata: self.sata,
        }
    }
    fn neuf(b: &[Bail]) -> Self {
        let ctx = |source, unite, did| Contexte {
            source,
            domaine: domaine(unite, did),
            traduit: true,
            ats_actif: false,
        };
        let c = vec![
            ctx(PONT, U1, 11),
            ctx(USB, U1, 3),
            ctx(ALIAS, U1, 3),
            ctx(NIC, U2, 3),
            ctx(SATA, U2, 7),
        ];
        let m = b
            .iter()
            .map(|b| Mapping {
                domaine: c.iter().find(|x| x.source == b.titulaire).unwrap().domaine,
                traduction: b.traduction,
            })
            .collect();
        Self {
            u: vec![
                EtatUnite {
                    base: U1,
                    racine: Z,
                    tes: true,
                },
                EtatUnite {
                    base: U2,
                    racine: Z + 4096,
                    tes: true,
                },
            ],
            c,
            m,
            sata: EtatSata {
                controleur: SATA,
                generation: 7,
                commande_pci: 6,
                registres: page(0xa00000),
                lectures: 5,
                ecritures: 0,
                flush: 0,
            },
        }
    }
}
fn baux() -> Vec<Bail> {
    vec![
        bail(USB, 0x440000, Droits::Ecriture),
        bail(USB, 0x441000, Droits::Lecture),
        bail(SATA, 0x500000, Droits::LectureEcriture),
    ]
}
const MEM: [Plage; 1] = [Plage {
    debut: Z,
    octets: 8 * 4096,
}];
fn profil(b: &[Bail]) -> Profil<'_> {
    Profil {
        xhci: USB,
        nic: NIC,
        sata: SATA,
        protegees: &MEM,
        baux: b,
    }
}
fn refus(t: &[u8], f: &[Fonction]) -> Refus {
    Inventaire::decouvrir(&[t], f).err().unwrap()
}
fn compare(change: impl FnOnce(&mut Etat)) -> Resultat<()> {
    let t = table(&[explicite(), defaut()]);
    let fs = fonctions();
    let b = baux();
    let a = Etat::neuf(&b);
    let mut z = Etat::neuf(&b);
    change(&mut z);
    Inventaire::decouvrir(&[&t], &fs)?.comparer(&profil(&b), &a.vue(), &z.vue())
}
#[test]
fn deux_unites_et_domaines_locaux_ordre_indifferent() {
    let t = table(&[explicite(), defaut()]);
    let mut f = fonctions();
    f.reverse();
    let inv = Inventaire::decouvrir(&[&t], &f).unwrap();
    assert_eq!(inv.nombre_unites(), 2);
    for (a, u) in [(USB, U1), (ALIAS, U1), (SATA, U2), (NIC, U2), (PONT, U1)] {
        assert_eq!(inv.unite(a), Ok(u));
    }
    assert_eq!(
        compare(|e| {
            e.u.reverse();
            e.c.reverse();
            e.m.reverse();
        }),
        Ok(())
    );
}
#[test]
fn segments_et_unite_sans_fonction_dma() {
    let mut fs = fonctions();
    let autre = Adresse {
        segment: 10,
        ..SATA
    };
    fs.push(pci(autre, 0x010601));
    let irq = record(0, 0, 12, 0x77000, None, &scope(3, 0, &[[4, 0]]));
    let t = table(&[
        irq,
        explicite(),
        defaut(),
        record(0, 1, 10, 0x88000, None, &[]),
    ]);
    let inv = Inventaire::decouvrir(&[&t], &fs).unwrap();
    assert_eq!(inv.nombre_unites(), 4);
    assert_eq!(inv.unite(autre), Ok(0x88000));
    assert_eq!(inv.unite(SATA), Ok(U2));
}
#[test]
fn chemin_multisaut_et_pont_irresolu() {
    let mut fs = fonctions();
    fs[0].pont = Some((4, 7));
    let pont2 = id(4, 11);
    fs.push(Fonction {
        pont: Some((7, 7)),
        ..pci(pont2, 0x060400)
    });
    let aval = id(7, 5);
    fs.push(pci(aval, 0xff0000));
    let sc = scope(1, 0, &[[2, 0], [11, 0], [5, 0]]);
    let t = table(&[record(0, 0, 9, U1, None, &sc), defaut()]);
    assert_eq!(
        Inventaire::decouvrir(&[&t], &fs).unwrap().unite(aval),
        Ok(U1)
    );
    fs.retain(|f| f.adresse != pont2);
    assert_eq!(refus(&t, &fs), Refus::Portee);
}
#[test]
fn unite_par_defaut_unique_et_derniere_du_segment() {
    let f = fonctions();
    for rows in [
        vec![defaut(), explicite()],
        vec![defaut(), record(0, 1, 9, U1, None, &[])],
    ] {
        assert!(Inventaire::decouvrir(&[&table(&rows)], &f).is_err());
    }
    assert_eq!(refus(&table(&[explicite()]), &f), Refus::Couverture);
    assert_eq!(
        refus(
            &table(&[record(0, 1, 9, U1, None, &scope(1, 0, &[[9, 0]]))]),
            &f
        ),
        Refus::Portee
    );
}
#[test]
fn chevauchement_et_portee_absente() {
    let f = fonctions();
    let redondant = record(0, 0, 9, 0x88000, None, &scope(1, 4, &[[3, 0]]));
    assert_eq!(
        refus(&table(&[explicite(), redondant, defaut()]), &f),
        Refus::Ambigu
    );
    let absent = record(0, 0, 9, U1, None, &scope(1, 0, &[[18, 0]]));
    assert_eq!(refus(&table(&[absent, defaut()]), &f), Refus::Portee);
    let irq = record(0, 1, 9, U1, None, &scope(3, 0, &[[9, 0]]));
    assert_eq!(refus(&table(&[irq]), &f), Refus::Ambigu);
}
#[test]
fn alias_inconnu_absent_cyclique_ou_inter_unites() {
    let t = table(&[explicite(), defaut()]);
    for owner in [None, Some(id(2, 3)), Some(NIC)] {
        let mut f = fonctions();
        f[2].titulaire = owner;
        assert_eq!(refus(&t, &f), Refus::Alias);
    }
    let mut f = fonctions();
    f[1].titulaire = Some(ALIAS);
    assert_eq!(refus(&t, &f), Refus::Alias);
}
#[test]
fn limites_entete_tables_et_structures_inconnues() {
    let f = fonctions();
    let t = table(&[defaut()]);
    assert!(Inventaire::decouvrir(&[], &f).is_err());
    assert!(Inventaire::decouvrir(&[&t, &t], &f).is_err());
    for len in 0..t.len() {
        assert!(Inventaire::decouvrir(&[&t[..len]], &f).is_err());
    }
    let mut b = t.clone();
    b[9] ^= 1;
    assert_eq!(refus(&b, &f), Refus::Table);
    let mut b = t.clone();
    b[50] = 17;
    checksum(&mut b);
    assert_eq!(refus(&b, &f), Refus::Table);
    let b = table(&[record(2, 1, 9, U1, None, &[]), defaut()]);
    assert_eq!(refus(&b, &f), Refus::Portee);
    let b = vec![0; 16385];
    assert_eq!(refus(&b, &f), Refus::Table);
    assert_eq!(refus(&t, &vec![f[0]; 65]), Refus::Limite);
}
#[test]
fn rmrr_appliquee_a_un_alias_et_a_son_titulaire() {
    let f = fonctions();
    let b = baux();
    let p = profil(&b);
    let e = Etat::neuf(&b);
    for (pa, result) in [
        (0x440000, Ok(())),
        (Z, Err(Refus::Reservation)),
        (0x500000, Err(Refus::Reservation)),
    ] {
        let rmrr = record(1, 0, 9, pa, Some(pa + 4095), &scope(1, 4, &[[3, 1]]));
        let t = table(&[explicite(), rmrr, defaut()]);
        let inv = Inventaire::decouvrir(&[&t], &f).unwrap();
        assert_eq!(inv.comparer(&p, &e.vue(), &e.vue()), result);
    }
}
#[test]
fn domaine_partage_alias_divergent_et_unite_erronee() {
    assert_eq!(
        compare(|e| e.c[4].domaine = e.c[3].domaine),
        Err(Refus::Domaine)
    );
    assert_eq!(compare(|e| e.c[2].domaine.did += 1), Err(Refus::Domaine));
    assert_eq!(compare(|e| e.c[1].domaine.unite = U2), Err(Refus::Domaine));
}
#[test]
fn traductions_inactives_directes_ats_et_cliche_incomplet() {
    assert_eq!(
        compare(|e| e.u[1].racine = e.u[0].racine),
        Err(Refus::Unite)
    );
    assert_eq!(compare(|e| e.u[0].tes = false), Err(Refus::Unite));
    assert_eq!(compare(|e| e.c[0].traduit = false), Err(Refus::Domaine));
    assert_eq!(compare(|e| e.c[0].ats_actif = true), Err(Refus::Domaine));
    assert_eq!(
        compare(|e| {
            e.c.pop();
        }),
        Err(Refus::Couverture)
    );
    assert_eq!(compare(|e| e.c[2] = e.c[1]), Err(Refus::Domaine));
}
#[test]
fn permissions_directionnelles_et_mappings_fermes() {
    assert_eq!(
        compare(|e| e.m[0].traduction.droits = Droits::LectureEcriture),
        Err(Refus::Permission)
    );
    assert_eq!(compare(|e| e.m[0] = e.m[1]), Err(Refus::Permission));
    assert_eq!(
        compare(|e| e.m[0].domaine = e.c[4].domaine),
        Err(Refus::Permission)
    );
    assert_eq!(
        compare(|e| {
            e.m.push(e.m[0]);
        }),
        Err(Refus::Couverture)
    );
}
#[test]
fn conservation_racine_sata_et_identite_domaine() {
    assert_eq!(compare(|e| e.u[1].racine += 4096), Err(Refus::Continuite));
    assert_eq!(
        compare(|e| {
            e.c[4].domaine.did = 28;
            e.m[2].domaine.did = 28;
        }),
        Err(Refus::Continuite)
    );
    assert_eq!(
        compare(|e| e.m[2].traduction.iova += 4096),
        Err(Refus::Permission)
    );
    assert_eq!(
        compare(|e| e.u[1].racine = 0x900000),
        Err(Refus::Permission)
    );
}
#[test]
fn politique_ne_peut_preter_le_pe_les_tables_le_mmio_ni_le_stockage() {
    let t = table(&[explicite(), defaut()]);
    let f = fonctions();
    let inv = Inventaire::decouvrir(&[&t], &f).unwrap();
    for pa in [Z, Z + 4096, U1, U2, 0x500000, 0x441000] {
        let mut b = baux();
        b[0].traduction.ram = page(pa);
        let e = Etat::neuf(&b);
        assert_eq!(
            inv.comparer(&profil(&b), &e.vue(), &e.vue()),
            Err(Refus::Permission)
        );
    }
    let mut b = baux();
    b[0].traduction.ram.octets = 0;
    let e = Etat::neuf(&b);
    assert_eq!(
        inv.comparer(&profil(&b), &e.vue(), &e.vue()),
        Err(Refus::Plage)
    );
}
#[test]
fn roles_et_cliche_initial_verifies() {
    let t = table(&[explicite(), defaut()]);
    let f = fonctions();
    let inv = Inventaire::decouvrir(&[&t], &f).unwrap();
    let b = baux();
    let mut p = profil(&b);
    let mut e = Etat::neuf(&b);
    let z = Etat::neuf(&b);
    p.sata = NIC;
    assert_eq!(inv.comparer(&p, &e.vue(), &z.vue()), Err(Refus::Role));
    e.m[2].traduction.droits = Droits::Lecture;
    assert_eq!(
        inv.comparer(&profil(&b), &e.vue(), &z.vue()),
        Err(Refus::Permission)
    );
}

#[test]
fn refus_si_sata_change_ou_si_son_mmio_recouvre_un_pret() {
    for change in [
        (|e: &mut Etat| e.sata.flush += 1) as fn(&mut Etat),
        |e| e.sata.lectures += 1,
        |e| e.sata.ecritures += 1,
        |e| e.sata.generation += 1,
        |e| e.sata.commande_pci ^= 4,
        |e| e.sata.registres = page(0x440000),
        |e| e.sata.controleur = NIC,
    ] {
        assert_eq!(compare(change), Err(Refus::Stockage));
    }
}
#[test]
fn graphique_explicite_xhci_nic_et_sata_sur_unite_commune() {
    let mut fs = fonctions();
    let gpu = id(0, 6);
    fs.push(pci(gpu, 0x030000));
    let t = table(&[record(0, 0, 9, U1, None, &scope(1, 0, &[[6, 0]])), defaut()]);
    let inv = Inventaire::decouvrir(&[&t], &fs).unwrap();
    let b = baux();
    let mut e = Etat::neuf(&b);
    for c in &mut e.c {
        c.domaine.unite = U2;
    }
    e.c[3].domaine.did = 8;
    e.c.push(Contexte {
        source: gpu,
        domaine: domaine(U1, 3),
        traduit: true,
        ats_actif: false,
    });
    for m in &mut e.m {
        m.domaine.unite = U2;
    }
    assert_eq!(inv.unite(gpu), Ok(U1));
    assert_eq!(inv.comparer(&profil(&b), &e.vue(), &e.vue()), Ok(()));
    e.c[4].domaine = e.c[1].domaine;
    assert_eq!(
        inv.comparer(&profil(&b), &e.vue(), &e.vue()),
        Err(Refus::Domaine)
    );
}
#[test]
fn limites_inclusives_des_unites_et_fonctions() {
    let fs: Vec<_> = (0..64)
        .map(|i| pci(Adresse { segment: 9, rid: i }, 0xff0000))
        .collect();
    let mut rows: Vec<_> = (0..31)
        .map(|i| {
            record(
                0,
                0,
                9,
                U1 + 4096 * i,
                None,
                &scope(1, 0, &[[(i >> 3) as u8, (i & 7) as u8]]),
            )
        })
        .collect();
    rows.push(defaut());
    let t = table(&rows);
    let inv = Inventaire::decouvrir(&[&t], &fs).unwrap();
    assert_eq!(inv.nombre_unites(), 32);
    assert_eq!(inv.unite(fs[63].adresse), Ok(U2));
    rows.push(record(0, 1, 10, 0x900000, None, &[]));
    assert!(Inventaire::decouvrir(&[&table(&rows)], &fs).is_err());
}

#[test]
fn refuse_mmio_declare_ram_et_limites_arithmetiques() {
    let t = table(&[explicite(), defaut()]);
    let f = fonctions();
    let inv = Inventaire::decouvrir(&[&t], &f).unwrap();
    let b = baux();
    let e = Etat::neuf(&b);
    for z in [
        page(U2),
        page(1u64 << 48),
        Plage {
            debut: u64::MAX - 4095,
            octets: 8192,
        },
    ] {
        let zones = [MEM[0], z];
        let p = Profil {
            protegees: &zones,
            ..profil(&b)
        };
        assert_eq!(inv.comparer(&p, &e.vue(), &e.vue()), Err(Refus::Plage));
    }
}
