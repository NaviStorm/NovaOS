//! Oracle indépendant de la traduction CoW et des compteurs du pool.
use novafs::device::{Event, Persistence, SimDevice, SimFault, SimSnapshot};
use novafs::shared::{ObjectKind, SharedPool};
use novafs::{Device, Error, Ids, Result};
use std::collections::BTreeMap;

type Pool = SharedPool<SimDevice>;
const IDS: Ids = Ids {
    fs: [11; 16],
    lv: [12; 16],
};
const SUPPORT: u64 = 8192;
const F: &str = "/témoin";

fn support(sector: u32) -> Result<(Pool, novafs::device::SimControl)> {
    let d = SimDevice::new(SUPPORT, sector)?;
    let ctl = d.control();
    Ok((SharedPool::format(d, [45; 16])?, ctl))
}

fn origin(p: &mut Pool) -> Result<u64> {
    let id = p.create([1; 16], IDS, 256)?;
    p.create_file(id, F)?;
    p.write_file(id, F, 0, &[0x35; 4096])?;
    Ok(id)
}

#[test]
fn survie_des_descendants_et_tous_les_ordres_de_suppression() -> Result<()> {
    for sector in [512, 4096] {
        let (mut p, ctl) = support(sector)?;
        let o = origin(&mut p)?;
        let s = p.snapshot(o)?;
        let a = p.clone_snapshot(s, [2; 16])?;
        let b = p.clone_snapshot(s, [3; 16])?;
        let pristine = p.receipt(s)?;
        assert!(matches!(p.write_file(s, F, 0, &[9]), Err(Error::ReadOnly)));
        assert!(matches!(p.create_file(s, "/autre"), Err(Error::ReadOnly)));
        p.write_file(o, F, 0, &[0x13; 4096])?;
        p.write_file(a, F, 0, &[0xa1; 4096])?;
        p.write_file(b, F, 0, &[0xb2; 4096])?;
        let second = p.snapshot(o)?;
        assert_ne!(s, second);
        p.remove(second)?;
        p.advance()?;
        p.verify_receipt(&pristine)?;
        let views = [(o, 0x13), (s, 0x35), (a, 0xa1), (b, 0xb2)];
        let base = ctl.snapshot();
        let mut permutations = 0;
        for i in 0..4 {
            for j in 0..4 {
                for k in 0..4 {
                    if i == j || i == k || j == k {
                        continue;
                    }
                    let last = (0..4).find(|x| ![i, j, k].contains(x)).unwrap();
                    let mut p = SharedPool::open(base.device())?;
                    let mut alive = [true; 4];
                    for &v in &[i, j, k, last] {
                        p.remove(views[v].0)?;
                        p.advance()?;
                        alive[v] = false;
                        // Réemploi effectif des adresses libres après rotation.
                        let n = p.create(
                            [91; 16],
                            Ids {
                                fs: [124; 16],
                                lv: [123; 16],
                            },
                            256,
                        )?;
                        p.create_file(n, F)?;
                        p.write_file(n, F, 0, &[0xec; 4096])?;
                        p.remove(n)?;
                        p.advance()?;
                        p = SharedPool::open(p.into_device())?;
                        assert_eq!(p.list().len(), alive.iter().filter(|x| **x).count());
                        for (idx, (id, byte)) in views.iter().enumerate() {
                            if alive[idx] {
                                assert_eq!(p.read_file(*id, F, 0, 8192)?, [*byte; 4096]);
                            }
                        }
                    }
                    permutations += 1;
                }
            }
        }
        assert_eq!(permutations, 24);
        println!("PERMUTATIONS secteur={sector} ordres={permutations}");
    }
    Ok(())
}

#[test]
fn anciens_ecrivains_refuses_et_donnees_partagees_controlees() -> Result<()> {
    for sector in [512, 4096] {
        let (mut p, ctl) = support(sector)?;
        let o = origin(&mut p)?;
        let s = p.snapshot(o)?;
        let clone = p.clone_snapshot(s, [2; 16])?;
        p.remove(o)?;
        p.advance()?;
        assert_eq!(p.read_file(clone, F, 0, 4096)?, [0x35; 4096]);
        let stable = ctl.snapshot();
        let scan = stable.device();
        let old = stable.device();
        let old_ctl = old.control();
        old_ctl.trace(None, None);
        assert!(novafs::pool::Pool::import(old).is_err());
        assert!(old_ctl.events().is_empty());
        let mut marker = None;
        for b in 0..SUPPORT {
            if scan.read(b)? == [0x35; 4096] {
                assert!(marker.replace(b).is_none());
            }
        }
        let d = stable.device();
        let ctrl = d.control();
        ctrl.replace_stable(marker.expect("données partagées"), [0x67; 4096])?;
        assert!(SharedPool::open(d).is_err());
        let ro = SharedPool::open(stable.device())?;
        assert_eq!(ro.read_file(s, F, 0, 4096)?, [0x35; 4096]);
    }
    Ok(())
}

#[derive(Clone, Copy, Debug)]
enum Op {
    Snapshot,
    Clone,
    Ecrire,
    Retirer,
    Collecter,
}

#[derive(Clone)]
struct Oracle {
    // Les valeurs viennent uniquement des opérations soumises par le client.
    views: BTreeMap<u64, (ObjectKind, u8)>,
}

impl Oracle {
    fn matches(&self, p: &Pool) -> Result<bool> {
        if p.list().len() != self.views.len() {
            return Ok(false);
        }
        for obj in p.list() {
            let Some(&(kind, byte)) = self.views.get(&obj.id) else {
                return Ok(false);
            };
            if kind != obj.kind || p.read_file(obj.id, F, 0, 8192)? != [byte; 4096] {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

fn apply(p: &mut Pool, op: Op, id: u64, byte: u8) -> Result<Option<u64>> {
    match op {
        Op::Snapshot => p.snapshot(id).map(Some),
        Op::Clone => p.clone_snapshot(id, [3; 16]).map(Some),
        Op::Ecrire => p.write_file(id, F, 0, &[byte; 4096]).map(|_| None),
        Op::Retirer => p.remove(id).map(|_| None),
        Op::Collecter => p.advance().map(|_| None),
    }
}

fn expected(old: &Oracle, op: Op, id: u64, byte: u8, created: Option<u64>) -> Oracle {
    let mut new = old.clone();
    match op {
        Op::Snapshot | Op::Clone => {
            let kind = if matches!(op, Op::Snapshot) {
                ObjectKind::Snapshot
            } else {
                ObjectKind::Clone
            };
            assert!(
                new.views
                    .insert(created.unwrap(), (kind, old.views[&id].1))
                    .is_none()
            );
        }
        Op::Ecrire => new.views.get_mut(&id).unwrap().1 = byte,
        Op::Retirer => {
            new.views.remove(&id).unwrap();
        }
        Op::Collecter => {}
    }
    new
}

struct Mutation {
    base: SimSnapshot,
    old: Oracle,
    id: u64,
    op: Op,
}

impl Mutation {
    fn verify(&self) -> Result<()> {
        let d = self.base.device();
        let ctl = d.control();
        let mut p = SharedPool::open(d)?;
        assert!(self.old.matches(&p)?);
        ctl.trace(None, None);
        let created = apply(&mut p, self.op, self.id, 0x65)?;
        let new = expected(&self.old, self.op, self.id, 0x65, created);
        assert!(new.matches(&p)?);
        let events = ctl.events();
        // F0 stabilise le dernier reçu importé avant réutilisation des blocs.
        assert_eq!(events.first(), Some(&Event::FlushSubmitted(1)));
        assert_eq!(events.get(2), Some(&Event::FlushCompleted(1)));
        let f1 = events
            .iter()
            .position(|e| *e == Event::FlushCompleted(2))
            .unwrap();
        let f2 = events
            .iter()
            .position(|e| *e == Event::FlushEffect(3))
            .unwrap();
        assert_eq!(events.last(), Some(&Event::FlushCompleted(3)));
        let writes: Vec<_> = events
            .iter()
            .enumerate()
            .filter_map(|(idx, e)| match e {
                Event::WriteSubmitted(b) if idx > f1 => Some((idx, *b)),
                _ => None,
            })
            .collect();
        assert_eq!(writes.len(), 1);
        let (publish, anchor) = writes[0];
        assert!(publish < f2);
        let assert_recovery = |d: SimDevice, which: bool| -> Result<()> {
            let mut p = SharedPool::open(d)?;
            let oracle = if which { &new } else { &self.old };
            assert!(
                oracle.matches(&p)?,
                "{:?}: état mixte ou données perdues",
                self.op
            );
            p.advance()?;
            let checked = SharedPool::open(p.into_device())?;
            assert!(oracle.matches(&checked)?);
            Ok(())
        };
        for pos in 0..events.len() {
            for retain in [0, 1, usize::MAX] {
                let d = self.base.device();
                let ctl = d.control();
                let mut p = SharedPool::open(d)?;
                ctl.trace(Some(pos), None);
                assert!(
                    apply(&mut p, self.op, self.id, 0x65).is_err(),
                    "{:?} {pos}",
                    self.op
                );
                drop(p);
                ctl.power_cut(Persistence::Newest(retain))?;
                assert_recovery(
                    ctl.snapshot().device(),
                    pos >= f2 || (pos >= publish && retain > 0),
                )?;
            }
        }
        // Une écriture rejetée peut tout de même avoir atteint le cache.
        let targets: Vec<_> = events
            .iter()
            .filter_map(|e| match e {
                Event::BeforeWrite(b) => Some(*b),
                _ => None,
            })
            .collect();
        for block in &targets {
            for prefix in [0, 4096] {
                let d = self.base.device();
                let ctl = d.control();
                let mut p = SharedPool::open(d)?;
                ctl.inject(SimFault::Write {
                    block: *block,
                    prefix,
                })?;
                assert!(apply(&mut p, self.op, self.id, 0x65).is_err());
                ctl.power_cut(Persistence::Newest(usize::MAX))?;
                assert_recovery(ctl.snapshot().device(), *block == anchor && prefix == 4096)?;
            }
        }
        for ordinal in 1..=3 {
            for retained in [0, usize::MAX] {
                let d = self.base.device();
                let ctl = d.control();
                let mut p = SharedPool::open(d)?;
                ctl.inject(SimFault::Flush {
                    ordinal,
                    persist_newest: retained,
                })?;
                assert!(apply(&mut p, self.op, self.id, 0x65).is_err());
                ctl.power_cut(Persistence::None)?;
                assert_recovery(ctl.snapshot().device(), ordinal == 3 && retained != 0)?;
            }
        }
        // Déchirure de l'ancre: différence réelle par rapport aux deux enveloppes.
        let old_anchor = self.base.device().read(anchor)?;
        let new_anchor = ctl.snapshot().device().read(anchor)?;
        let mut prefix = None;
        for n in 1..4096 {
            if old_anchor[..n] != new_anchor[..n] && old_anchor[n..] != new_anchor[n..] {
                prefix = Some(n);
                break;
            }
        }
        let d = self.base.device();
        let ctl = d.control();
        let mut p = SharedPool::open(d)?;
        ctl.inject(SimFault::Write {
            block: anchor,
            prefix: prefix.unwrap(),
        })?;
        assert!(apply(&mut p, self.op, self.id, 0x65).is_err());
        ctl.power_cut(Persistence::Newest(usize::MAX))?;
        // Le sélecteur déchiré désigne deux catalogues immédiatement consécutifs.
        // Le candidat complet protégé par F1 est alors récupéré.
        assert_recovery(ctl.snapshot().device(), true)?;
        // Un troisième code ou une signature inconnue restent des corruptions.
        let medium = ctl.snapshot().device();
        let fail = medium.control();
        let mut invalid = medium.read(anchor)?;
        invalid[0] ^= 0x01;
        fail.replace_stable(anchor, invalid)?;
        assert!(SharedPool::open(medium).is_err());
        println!(
            "PANNE {:?} frontières={} coupures={} EIO={} déchirures=1",
            self.op,
            events.len(),
            3 * events.len(),
            2 * targets.len() + 6
        );
        Ok(())
    }
}

#[test]
fn coupures_sur_chaque_frontiere_et_eio() -> Result<()> {
    for sector in [512, 4096] {
        let (mut p, ctl) = support(sector)?;
        let o = origin(&mut p)?;
        let s = p.snapshot(o)?;
        let c = p.clone_snapshot(s, [2; 16])?;
        p.write_file(c, F, 0, &[0xc2; 4096])?;
        let mut old = Oracle {
            views: BTreeMap::from([
                (o, (ObjectKind::Origin, 0x35)),
                (s, (ObjectKind::Snapshot, 0x35)),
                (c, (ObjectKind::Clone, 0xc2)),
            ]),
        };
        for (op, id) in [
            (Op::Snapshot, o),
            (Op::Clone, s),
            (Op::Ecrire, c),
            (Op::Retirer, o),
            (Op::Retirer, s),
        ] {
            Mutation {
                base: ctl.snapshot(),
                old: old.clone(),
                id,
                op,
            }
            .verify()?;
        }
        p.remove(c)?;
        old.views.remove(&c);
        Mutation {
            base: ctl.snapshot(),
            old,
            id: o,
            op: Op::Collecter,
        }
        .verify()?;
    }
    Ok(())
}

#[test]
fn bilan_physique_quota_et_saturation_cow() -> Result<()> {
    for sector in [512, 4096] {
        let (mut p, ctl) = support(sector)?;
        let o = origin(&mut p)?;
        // Le domaine ne peut pas promettre plus de quatre grains.
        let s = p.snapshot(o)?;
        let c = p.clone_snapshot(s, [1; 16])?;
        let first = p.receipt(s)?;
        let initial = p.usage(c)?;
        assert!(initial.shared > 0);
        assert_eq!(initial.logical_blocks, 256);
        assert_eq!(initial.occupied, p.usage(s)?.occupied);
        let mut clones = vec![c];
        let mut refused = false;
        for _ in 0..4 {
            match p.clone_snapshot(s, [1; 16]) {
                Ok(id) => clones.push(id),
                Err(Error::QuotaExceeded) => {
                    refused = true;
                    break;
                }
                Err(e) => return Err(e),
            }
        }
        assert!(refused);
        let mut refused = false;
        for n in 0..300u64 {
            let before = p.receipt(c)?;
            ctl.trace(None, None);
            match p.write_file(c, F, n * 4096, &[0xb5; 4096]) {
                Ok(()) => {}
                Err(Error::NoSpace | Error::PoolFull | Error::QuotaExceeded) => {
                    p.verify_receipt(&before)?;
                    assert_eq!(p.read_file(s, F, 0, 8192)?, [0x35; 4096]);
                    assert!(
                        ctl.events().is_empty(),
                        "publication après refus d'admission"
                    );
                    refused = true;
                    break;
                }
                Err(e) => return Err(e),
            }
        }
        assert!(refused, "la charge doit saturer le clone");
        p.verify_receipt(&first)?;
        p.remove(o)?;
        for id in clones {
            p.remove(id)?;
        }
        p.advance()?;
        let state = p.usage(s)?;
        assert_eq!(state.shared, 0);
        assert_eq!(state.deferred, 0);
        assert!(state.occupied <= initial.occupied);
        assert_eq!(p.read_file(s, F, 0, 8192)?, [0x35; 4096]);
        let p = SharedPool::open(p.into_device())?;
        p.verify_receipt(&first)?;
    }
    Ok(())
}

#[test]
fn modele_aleatoire_avec_coupures_et_reouverture() -> Result<()> {
    for sector in [512, 4096] {
        for seed in [0x519c_0428_7dab_33a1u64, 0xf3a0_1417_6825_914d] {
            let (mut p, mut ctl) = support(sector)?;
            let o = origin(&mut p)?;
            let mut model = Oracle {
                views: BTreeMap::from([(o, (ObjectKind::Origin, 0x35))]),
            };
            let mut rng = seed;
            let mut cuts = 0;
            for step in 0..120 {
                rng ^= rng << 13;
                rng ^= rng >> 7;
                rng ^= rng << 17;
                let idx = rng as usize % model.views.len();
                let (&id, &(kind, _)) = model.views.iter().nth(idx).unwrap();
                let byte = (rng >> 32) as u8;
                let op = match (rng >> 12) % 6 {
                    0 | 1 if kind != ObjectKind::Snapshot && model.views.len() < 6 => Op::Snapshot,
                    2 if kind == ObjectKind::Snapshot && model.views.len() < 6 => Op::Clone,
                    3 if id != o => Op::Retirer,
                    _ if kind != ObjectKind::Snapshot => Op::Ecrire,
                    _ => Op::Collecter,
                };
                let fault = step % 4 == 2;
                let persist = step % 8 == 2;
                if fault {
                    // Cibler le sélecteur après F1 indépendamment de la stabilisation F0
                    // du premier écrivain après un remontage.
                    ctl.inject(SimFault::Write {
                        block: 2,
                        prefix: 4096,
                    })?;
                }
                // L'identité future, nécessaire au modèle après une perte d'ACK, est celle
                // de la même opération sur une copie isolée. Aucun octet de fichier n'est lu.
                let reference = if fault && persist && matches!(op, Op::Snapshot | Op::Clone) {
                    let mut twin = SharedPool::open(ctl.snapshot().device())?;
                    apply(&mut twin, op, id, byte).ok().flatten()
                } else {
                    None
                };
                let outcome = apply(&mut p, op, id, byte);
                let confirmed = match outcome {
                    Ok(new) => {
                        assert!(!fault);
                        model = expected(&model, op, id, byte, new);
                        true
                    }
                    Err(
                        Error::PoolFull | Error::QuotaExceeded | Error::NoSpace | Error::Limit(_),
                    ) => false,
                    Err(_) if fault => {
                        cuts += 1;
                        if persist {
                            model = expected(&model, op, id, byte, reference);
                        }
                        false
                    }
                    Err(e) => return Err(e),
                };
                if !confirmed || step % 5 == 0 {
                    drop(p);
                    ctl.power_cut(if fault && persist {
                        Persistence::Newest(1)
                    } else {
                        Persistence::None
                    })?;
                    // Reprendre le même contrôleur pour les injections des opérations suivantes.
                    let next = ctl.snapshot().device();
                    let next_ctl = next.control();
                    p = SharedPool::open(next)?;
                    ctl = next_ctl;
                }
                assert!(model.matches(&p)?, "graine {seed} étape {step}");
            }
            assert!(cuts >= 10);
            println!(
                "ALEATOIRE secteur={sector} graine={seed:016x} opérations=120 coupures={cuts}"
            );
        }
    }
    Ok(())
}

#[test]
fn pool_sans_credit_libre_pendant_divergence_et_suppression() -> Result<()> {
    // Dimensionner le PD à la promesse exacte du profil de publication.
    let (mut trial, _) = support(4096)?;
    let id = origin(&mut trial)?;
    let snap = trial.snapshot(id)?;
    let clone = trial.clone_snapshot(snap, [2; 16])?;
    let minimum = trial.usage(clone)?.reserved;
    for sector in [512, 4096] {
        let mut pool = SharedPool::format(SimDevice::new(minimum, sector)?, [45; 16])?;
        let id = origin(&mut pool)?;
        let snap = pool.snapshot(id)?;
        let clone = pool.clone_snapshot(snap, [2; 16])?;
        assert_eq!(pool.usage(clone)?.reserved, minimum);
        let before = pool.receipt(clone)?;
        assert!(matches!(
            pool.clone_snapshot(snap, [3; 16]),
            Err(Error::PoolFull)
        ));
        pool.verify_receipt(&before)?;
        pool.write_file(clone, F, 0, &[0xac; 4096])?;
        assert_eq!(pool.read_file(snap, F, 0, 8192)?, [0x35; 4096]);
        pool.remove(id)?;
        pool.remove(snap)?;
        pool.advance()?;
        assert_eq!(pool.read_file(clone, F, 0, 8192)?, [0xac; 4096]);
        assert_eq!(pool.usage(clone)?.shared, 0);
        assert_eq!(pool.usage(clone)?.deferred, 0);
        let replacement = pool.create(
            [8; 16],
            Ids {
                fs: [98; 16],
                lv: [99; 16],
            },
            256,
        )?;
        pool.create_file(replacement, F)?;
        pool.write_file(replacement, F, 0, &[0xce; 4096])?;
        let pool = SharedPool::open(pool.into_device())?;
        assert_eq!(pool.read_file(clone, F, 0, 8192)?, [0xac; 4096]);
        println!(
            "SATURATION secteur={sector} capacité={minimum} crédits_libres=0 divergence_réussie"
        );
    }
    Ok(())
}

#[test]
fn recus_non_interchangeables_et_ids_non_reemployes() -> Result<()> {
    for sector in [512, 4096] {
        let (mut p, ctl) = support(sector)?;
        let o = origin(&mut p)?;
        let s = p.snapshot(o)?;
        let c = p.clone_snapshot(s, [2; 16])?;
        let d = p.clone_snapshot(s, [2; 16])?;
        let receipt = p.receipt(c)?;
        let mut forged = receipt;
        forged.id = d;
        assert!(p.verify_receipt(&forged).is_err());
        forged = receipt;
        forged.pool[0] ^= 1;
        assert!(p.verify_receipt(&forged).is_err());
        let (s_before, d_before) = (p.receipt(s)?, p.receipt(d)?);
        let snapshot_blocks = p.usage(s)?.shared;
        ctl.trace(None, None);
        let another = p.snapshot(c)?;
        let io = ctl.events();
        assert_eq!(
            io.iter()
                .filter(|e| matches!(e, Event::BeforeWrite(_)))
                .count(),
            2
        );
        assert_eq!(p.usage(s)?.shared, snapshot_blocks);
        p.write_file(c, F, 0, "modifié".as_bytes())?;
        assert!(p.verify_receipt(&receipt).is_err());
        p.remove(c)?;
        p.remove(s)?;
        p.advance()?;
        let p = SharedPool::open(p.into_device())?;
        assert!(p.verify_receipt(&s_before).is_err());
        p.verify_receipt(&d_before)?;
        assert_eq!(p.read_file(another, F, 0, 8192)?, [0x35; 4096]);
        assert!(another > d && d > c && c > s && s > o);
    }
    Ok(())
}
