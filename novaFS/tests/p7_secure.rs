//! Parcours publics P7 sur les mêmes dispositifs à secteurs 512 et 4096.
//! Les oracles conservent séparément les octets et les générations acquittés.
#[path = "../lab/p6_donnees.rs"]
mod donnees;

use donnees::{donnees, suivant};
use novafs::crypto::{Credential, Secret};
use novafs::device::{Event, Persistence, SimControl, SimDevice, SimFault, SimSnapshot};
use novafs::secure_pool::SecurePool;
use novafs::{Block, Compression, Device, Error, Ids, Result};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Mutex;

const K: usize = 4096;
const POOL: [u8; 16] = [14; 16];
const OWNER: [u8; 16] = [33; 16];
type Pool = SecurePool<SimDevice>;
const PHRASE: &[u8] = b"mot de passe de la fixture P7";
static KDF: Mutex<()> = Mutex::new(());
fn admin() -> Secret {
    Secret::from_bytes([0x54; 32])
}
fn recovery() -> Secret {
    Secret::from_bytes([0xb9; 32])
}
fn ids(n: u8) -> Ids {
    Ids {
        fs: [n; 16],
        lv: [n + 1; 16],
    }
}

fn volume(p: &mut Pool, n: u8, policy: Compression) -> Result<(u64, [u8; 16])> {
    let _guard = KDF.lock().unwrap();
    let v = p.create(OWNER, ids(n), 256, policy, Some((PHRASE, &recovery())))?;
    let domain = p.domain(v)?;
    p.create_file(v, "/f")?;
    Ok((v, domain))
}
fn pool(blocks: u64, sector: u32) -> Result<(Pool, SimControl)> {
    let d = SimDevice::new(blocks, sector)?;
    let ctl = d.control();
    Ok((SecurePool::format(d, POOL, &admin())?, ctl))
}
fn reouvrir(stable: &SimSnapshot, domains: &[[u8; 16]]) -> Result<Pool> {
    let mut p = SecurePool::open(stable.device(), POOL, &admin())?;
    for id in domains {
        p.unlock(*id, Credential::Recovery(&recovery()))?;
    }
    Ok(p)
}
fn lire(p: &mut Pool, v: u64) -> Result<Vec<u8>> {
    p.read_file(v, "/f", 0, 16 * K)
}
fn obj(d: &SimDevice, domain: [u8; 16], g: u64) -> Result<Vec<(u64, Block)>> {
    let mut result = Vec::new();
    for b in (46..d.blocks()).step_by(2) {
        let h = d.read(b)?;
        if &h[..8] == b"NP7UNIT1" && h[32..48] == domain && h[64..72] == g.to_le_bytes() {
            result.push((b, h));
        }
    }
    Ok(result)
}

#[test]
fn volumes_independants_contenus_identiques_verrouillage_et_coexistence() -> Result<()> {
    for sector in [512, 4096] {
        let (mut p, ctl) = pool(3000, sector)?;
        let (a, da) = volume(&mut p, 40, Compression::None)?;
        let (b, db) = volume(&mut p, 50, Compression::None)?;
        let c = p.create(OWNER, ids(80), 256, Compression::None, None)?;
        p.create_file(c, "/f")?;
        let data = donnees(4 * K, "aleatoire", 0xc1c7);
        p.write_file(a, "/f", 0, &data)?;
        let ga = p.generation();
        p.write_file(b, "/f", 0, &data)?;
        let gb = p.generation();
        p.write_file(c, "/f", 0, b"fichier du volume public")?;
        assert_ne!(da, db);
        assert_eq!(lire(&mut p, a)?, data);
        assert_eq!(lire(&mut p, b)?, data);
        let disk = ctl.snapshot().device();
        let public = novafs::secure_pool::structural_check(&disk)?;
        assert_eq!(public.pool, POOL);
        assert_eq!(public.generation, p.generation());
        assert_eq!(public.protected_generations, 2);
        assert!(public.encrypted_root_references >= 2);
        assert_eq!(public.active_views.len(), 3);
        novafs::secure_pool::verify_commit_replicas(&disk, POOL, &admin())?;
        assert!(
            novafs::secure_pool::verify_commit_replicas(&disk, POOL, &Secret::from_bytes([99; 32]))
                .is_err()
        );
        let x = obj(&disk, da, ga)?;
        let y = obj(&disk, db, gb)?;
        let mut matched = 0;
        for (bx, hx) in x.iter().filter(|(_, h)| h[11] == 1) {
            if let Some((by, hy)) = y
                .iter()
                .find(|(_, h)| h[11] == 1 && h[48..56] == hx[48..56])
            {
                matched += 1;
                assert_ne!(hx[88..112], hy[88..112]);
                assert_ne!(disk.read(*bx + 1)?, disk.read(*by + 1)?);
            }
        }
        assert!(matched >= 4);
        // Les séquences cherchées sont privées aux deux domaines, absentes du
        // contenu public. Elles pourraient aussi exposer un stockage temporaire.
        for b in 0..disk.blocks() {
            let page = disk.read(b)?;
            assert!(!page.windows(64).any(|x| x == &data[..64]));
            assert!(!page.windows(PHRASE.len()).any(|x| x == PHRASE));
        }
        p.lock(da);
        assert!(lire(&mut p, a).is_err());
        assert_eq!(lire(&mut p, b)?, data);
        assert_eq!(lire(&mut p, c)?, b"fichier du volume public");
        assert!(p.write_file(b, "/f", 0, b"x").is_err());
        assert!(
            p.unlock(da, Credential::Recovery(&Secret::from_bytes([0xaf; 32])))
                .is_err()
        );
        {
            let _guard = KDF.lock().unwrap();
            assert!(p.unlock(da, Credential::Passphrase(b"incorrect")).is_err());
            p.unlock(da, Credential::Passphrase(PHRASE))?;
        }
        p.check()?;
        drop(p);
        let mut mounted = SecurePool::open(ctl.snapshot().device(), POOL, &admin())?;
        let g = mounted.generation();
        assert!(lire(&mut mounted, a).is_err());
        mounted.unlock(da, Credential::Recovery(&recovery()))?;
        assert_eq!(lire(&mut mounted, a)?, data);
        assert!(lire(&mut mounted, b).is_err());
        assert_eq!(lire(&mut mounted, c)?, b"fichier du volume public");
        mounted.unlock(db, Credential::Recovery(&recovery()))?;
        mounted.check()?;
        assert_eq!(g, mounted.generation());
        assert!(
            SecurePool::open(ctl.snapshot().device(), POOL, &Secret::from_bytes([18; 32])).is_err()
        );
    }
    Ok(())
}

#[test]
fn clones_compression_export_restauration_et_divergence_de_deux_images() -> Result<()> {
    for sector in [512, 4096] {
        let (mut p, ctl) = pool(2200, sector)?;
        let (v, d) = volume(&mut p, 11, Compression::Lz4)?;
        let bytes = donnees(12 * K, "repetitif", 0x12345);
        p.write_file(v, "/f", 0, &bytes)?;
        let original = obj(&ctl.snapshot().device(), d, p.generation())?;
        assert!(original.iter().any(|(_, h)| h[11] == 3 && h[10] == 1));
        let snapshot = p.snapshot(v)?;
        let a = p.clone_snapshot(snapshot, [8; 16])?;
        let b = p.clone_snapshot(snapshot, [9; 16])?;
        assert_eq!(p.domain(a)?, d);
        assert_eq!(p.domain(b)?, d);
        assert!(matches!(
            p.write_file(snapshot, "/f", 0, b"!"),
            Err(Error::ReadOnly)
        ));
        let base = ctl.snapshot();
        let mut nonces = BTreeSet::new();
        let random = donnees(K, "aleatoire", 0x234ab);
        for branch in 0..2 {
            let mut p2 = reouvrir(&base, &[d])?;
            let mut expected = bytes.clone();
            let mut delta = random.clone();
            delta[9] = branch;
            expected[K - 13..2 * K - 13].copy_from_slice(&delta);
            p2.write_file(a, "/f", (K - 13) as u64, &delta)?;
            assert_eq!(lire(&mut p2, a)?, expected);
            assert_eq!(lire(&mut p2, b)?, bytes);
            assert_eq!(lire(&mut p2, snapshot)?, bytes);
            let g = p2.generation();
            let dsk = p2.into_device();
            let changed = obj(&dsk, d, g)?;
            assert!(!changed.is_empty());
            for (_, h) in changed {
                assert!(nonces.insert(<[u8; 24]>::try_from(&h[88..112]).unwrap()));
            }
        }
        p.remove(v)?;
        p.write_file(a, "/f", 9, b"un clone independant")?;
        assert_eq!(lire(&mut p, snapshot)?, bytes);
        assert_eq!(lire(&mut p, b)?, bytes);
        let backup = Secret::from_bytes([57; 32]);
        let mut archive = Vec::new();
        p.export(snapshot, sector, &backup, &mut archive)?;
        drop(p);
        drop(ctl);
        drop(base);
        let bad = Secret::from_bytes([13; 32]);
        assert!(novafs::secure_export::check_export(&archive[..], &bad).is_err());
        let mut out_domains = BTreeSet::new();
        for policy in [Compression::Lz4, Compression::None] {
            let mut dst = SecurePool::format(SimDevice::new(2200, sector)?, [78; 16], &admin())?;
            let before = dst.generation();
            for at in [0, archive.len() / 2, archive.len() - 1] {
                assert!(
                    dst.restore(&archive[..at], &backup, OWNER, PHRASE, &recovery(), policy)
                        .is_err()
                );
                let mut forged = archive.clone();
                forged[at] ^= 1;
                assert!(
                    dst.restore(&forged[..], &backup, OWNER, PHRASE, &recovery(), policy)
                        .is_err()
                );
                assert_eq!(dst.generation(), before);
                assert!(dst.list()?.is_empty());
            }
            let id = {
                let _guard = KDF.lock().unwrap();
                dst.restore(&archive[..], &backup, OWNER, PHRASE, &recovery(), policy)?
            };
            let dest_domain = dst.domain(id)?;
            assert_ne!(dest_domain, d);
            assert!(out_domains.insert(dest_domain));
            assert_eq!(lire(&mut dst, id)?, bytes);
            dst.lock(dest_domain);
            assert!(lire(&mut dst, id).is_err());
            dst.unlock(dest_domain, Credential::Recovery(&recovery()))?;
            dst.check()?;
        }
    }
    Ok(())
}

#[test]
fn falsifications_reelles_dans_le_graphe_et_fermeture_des_mutations() -> Result<()> {
    for sector in [512, 4096] {
        let (mut p, ctl) = pool(2200, sector)?;
        let (v, domain) = volume(&mut p, 14, Compression::None)?;
        let (other, foreign) = volume(&mut p, 24, Compression::None)?;
        p.write_file(v, "/f", 0, &donnees(6 * K, "aleatoire", 1))?;
        let gv = p.generation();
        p.write_file(other, "/f", 0, &donnees(6 * K, "aleatoire", 2))?;
        let go = p.generation();
        let base = ctl.snapshot();
        let origin = base.device();
        let objects = obj(&origin, domain, gv)?;
        let src: Vec<_> = objects.iter().filter(|(_, h)| h[11] == 1).collect();
        assert!(src.len() >= 2);
        let (addr, h) = src[0];
        let (same_domain, _) = src[1];
        let foreign_objects = obj(&origin, foreign, go)?;
        let (other_addr, _) = foreign_objects.iter().find(|(_, h)| h[11] == 1).unwrap();
        for b in [*addr, *addr + 1] {
            let disk = base.device();
            let ctl = disk.control();
            let mut check = opened(disk, domain)?;
            check.unlock(foreign, Credential::Recovery(&recovery()))?;
            ctl.inject(SimFault::Read { block: b })?;
            assert!(matches!(check.check(), Err(Error::Io(_))));
            assert!(check.write_file(v, "/f", 0, b"invalide").is_err());
        }
        for case in 0..15 {
            let d = base.device();
            let ctl = d.control();
            if case < 10 {
                let offset = [8, 10, 11, 32, 48, 56, 72, 76, 88, 112][case];
                let mut altered = *h;
                altered[offset] ^= 1;
                ctl.replace_stable(*addr, altered)?;
            } else if case == 10 {
                let mut ciphertext = origin.read(*addr + 1)?;
                ciphertext[K - 1] ^= 17;
                ctl.replace_stable(*addr + 1, ciphertext)?;
            } else if case <= 12 {
                let from = if case == 11 {
                    *same_domain
                } else {
                    *other_addr
                };
                ctl.replace_stable(*addr, origin.read(from)?)?;
                ctl.replace_stable(*addr + 1, origin.read(from + 1)?)?;
            } else if case == 13 {
                let (root, _) = objects.iter().find(|(_, h)| h[11] == 2).unwrap();
                let mut meta = origin.read(*root + 1)?;
                meta[11] ^= 0x80;
                ctl.replace_stable(*root + 1, meta)?;
            } else {
                let index = 10 + go % 4 * 9;
                let mut dir = origin.read(index)?;
                dir[64] ^= 0x04;
                ctl.replace_stable(index, dir)?;
            }
            let disk_before = ctl.snapshot().bytes()?;
            match SecurePool::open(d, POOL, &admin()) {
                Err(Error::Corrupt(_)) => assert_eq!(case, 14),
                Err(err) => return Err(err),
                Ok(mut opened) => {
                    opened.unlock(domain, Credential::Recovery(&recovery()))?;
                    opened.unlock(foreign, Credential::Recovery(&recovery()))?;
                    assert!(opened.check().is_err(), "défaut {case}");
                    assert!(opened.write_file(v, "/f", 0, b"interdit").is_err());
                }
            }
            assert_eq!(ctl.snapshot().bytes()?, disk_before);
        }
    }
    Ok(())
}

fn opened(device: SimDevice, domain: [u8; 16]) -> Result<Pool> {
    let mut p = SecurePool::open(device, POOL, &admin())?;
    p.unlock(domain, Credential::Recovery(&recovery()))?;
    Ok(p)
}

#[test]
fn frontieres_de_transaction_chiffree_et_eio() -> Result<()> {
    for sector in [512, 4096] {
        let (mut p, ctl) = pool(1600, sector)?;
        let (id, dom) = volume(&mut p, 10, Compression::Lz4)?;
        let old = donnees(2 * K, "repetitif", 19);
        let new = donnees(2 * K, "aleatoire", 17);
        p.write_file(id, "/f", 0, &old)?;
        let g = p.generation();
        let initial = ctl.snapshot();
        let d = initial.device();
        let ctl = d.control();
        let mut reference = opened(d, dom)?;
        ctl.trace(None, None);
        reference.write_file(id, "/f", 0, &new)?;
        let trace = ctl.events();
        assert_eq!(
            trace
                .iter()
                .filter(|e| matches!(e, Event::FlushCompleted(_)))
                .count(),
            3
        );
        let cert = 2 + 2 * ((g + 1) % 4);
        let first_auth = trace
            .iter()
            .position(|e| *e == Event::BeforeWrite(cert))
            .unwrap();
        let last_flush = trace
            .iter()
            .position(|e| *e == Event::FlushEffect(3))
            .unwrap();
        for cut in 0..=trace.len() {
            for persistence in [
                Persistence::None,
                Persistence::Newest(2),
                Persistence::Prefix {
                    block: cert,
                    bytes: K - 7,
                },
            ] {
                let d = initial.device();
                let ctl = d.control();
                let mut trial = opened(d, dom)?;
                ctl.trace(Some(cut), None);
                let outcome = trial.write_file(id, "/f", 0, &new);
                assert_eq!(outcome.is_ok(), cut == trace.len());
                ctl.power_cut(persistence)?;
                let mut recovered = reouvrir(&ctl.snapshot(), &[dom])?;
                let current = recovered.generation();
                assert!(current == g || current == g + 1);
                if cut <= first_auth {
                    assert_eq!(current, g);
                }
                if cut >= last_flush {
                    assert_eq!(current, g + 1);
                }
                assert_eq!(
                    lire(&mut recovered, id)?,
                    if current == g { &old } else { &new }.as_slice()
                );
                recovered.check()?;
            }
        }
        let wrote: BTreeSet<u64> = trace
            .iter()
            .filter_map(|e| match e {
                Event::BeforeWrite(b) => Some(*b),
                _ => None,
            })
            .collect();
        let mut faults = Vec::new();
        for b in &wrote {
            faults.push(SimFault::Write {
                block: *b,
                prefix: 2715,
            });
        }
        for n in 1..=3 {
            for persist_newest in [0, 1] {
                faults.push(SimFault::Flush {
                    ordinal: n,
                    persist_newest,
                });
            }
        }
        for fault in &faults {
            let d = initial.device();
            let ctl = d.control();
            let mut trial = opened(d, dom)?;
            ctl.inject(*fault)?;
            assert!(trial.write_file(id, "/f", 0, &new).is_err());
            ctl.power_cut(Persistence::None)?;
            let mut recovered = reouvrir(&ctl.snapshot(), &[dom])?;
            let committed = recovered.generation() == g + 1;
            let may_commit = matches!(
                fault,
                SimFault::Flush {
                    ordinal: 3,
                    persist_newest: 1
                }
            );
            assert_eq!(committed, may_commit, "{fault:?}");
            assert_eq!(
                lire(&mut recovered, id)?,
                if committed { &new } else { &old }.as_slice()
            );
            recovered.check()?;
        }
        eprintln!(
            "P7 secteur={sector} coupures={} erreurs_ecriture_flush={}",
            (trace.len() + 1) * 3,
            faults.len()
        );
    }
    Ok(())
}

#[test]
fn campagnes_a_oracle_externe_et_saturation() -> Result<()> {
    for sector in [512, 4096] {
        for seed in [0xeddfa3, 0x9108ab] {
            let (mut p, _) = pool(2500, sector)?;
            let (id, domain) = volume(&mut p, 32, Compression::Lz4)?;
            let mut rng = seed;
            let mut model = BTreeMap::new();
            let data = donnees(3 * K, "mixte", seed);
            p.write_file(id, "/f", 0, &data)?;
            model.insert(id, data);
            let mut snapshots = BTreeSet::new();
            for step in 0..48 {
                let roll = suivant(&mut rng);
                let nodes: Vec<_> = model.keys().copied().collect();
                let target = nodes[(roll as usize / 4) % nodes.len()];
                match roll % 5 {
                    0 if model.len() < 6 && !snapshots.contains(&target) => {
                        let new = p.snapshot(target)?;
                        model.insert(new, model[&target].clone());
                        snapshots.insert(new);
                    }
                    1 if model.len() < 6 && snapshots.contains(&target) => {
                        let new = p.clone_snapshot(target, OWNER)?;
                        model.insert(new, model[&target].clone());
                    }
                    2 if target != id => {
                        p.remove(target)?;
                        model.remove(&target);
                        snapshots.remove(&target);
                    }
                    _ if !snapshots.contains(&target) => {
                        let offset = suivant(&mut rng) as usize % (2 * K);
                        let delta =
                            donnees(100 + (roll as usize % 500), "aleatoire", suivant(&mut rng));
                        p.write_file(target, "/f", offset as u64, &delta)?;
                        model.get_mut(&target).unwrap()[offset..offset + delta.len()]
                            .copy_from_slice(&delta);
                    }
                    _ => {
                        assert!(p.write_file(target, "/f", 0, b"refus").is_err());
                    }
                }
                if step % 8 == 0 {
                    p = opened(p.into_device(), domain)?;
                    p.check()?;
                }
                for (&id, expected) in &model {
                    assert_eq!(&lire(&mut p, id)?, expected);
                }
            }
        }
        // Capacité P6 effective de 1 024 blocs, sous le coût de 16 crédits de clone.
        let (mut p, ctl) = pool(46 + 2 * (1024 - 7), sector)?;
        let (id, domain) = volume(&mut p, 60, Compression::Lz4)?;
        let bytes = donnees(4 * K, "aleatoire", 9183);
        p.write_file(id, "/f", 0, &bytes)?;
        let snap = p.snapshot(id)?;
        let mut funded = Vec::new();
        loop {
            match p.clone_snapshot(snap, OWNER) {
                Ok(n) => funded.push(n),
                Err(Error::PoolFull) => break,
                Err(err) => return Err(err),
            }
        }
        assert!(!funded.is_empty());
        let generation = p.generation();
        assert!(matches!(
            p.write_file(id, "/f", 256 * K as u64, b"!"),
            Err(Error::QuotaExceeded)
        ));
        assert_eq!(generation, p.generation());
        let mut mounted = reouvrir(&ctl.snapshot(), &[domain])?;
        mounted.check()?;
        assert_eq!(lire(&mut mounted, snap)?, bytes);
        // Le crédit déjà admis permet une vraie divergence CoW même lorsque
        // toute nouvelle demande de crédit de clone a été refusée.
        mounted.write_file(id, "/f", 0, b"saturation")?;
        let mut expected = bytes.clone();
        expected[..10].copy_from_slice(b"saturation");
        assert_eq!(lire(&mut mounted, id)?, expected);
        assert_eq!(lire(&mut mounted, snap)?, bytes);
        mounted.remove(funded[0])?;
        // Une génération de rotation retire l'ancien descripteur du graphe protégé.
        mounted.write_file(id, "/f", 0, b"R")?;
        let next = mounted.clone_snapshot(snap, OWNER)?;
        assert_eq!(lire(&mut mounted, next)?, bytes);
    }
    Ok(())
}

#[test]
fn entropie_indisponible_pendant_une_transaction() -> Result<()> {
    use novafs::crypto::{Entropy, HostEntropy};
    struct Fail {
        at: usize,
        calls: usize,
    }
    impl Entropy for Fail {
        fn fill(&mut self, out: &mut [u8]) -> Result<()> {
            self.calls += 1;
            if self.calls == self.at {
                return Err(Error::Unavailable);
            }
            HostEntropy.fill(out)
        }
    }
    for sector in [512, 4096] {
        let (mut p, ctl) = pool(1500, sector)?;
        let (id, domain) = volume(&mut p, 10, Compression::None)?;
        p.write_file(id, "/f", 0, b"dernier etat durable")?;
        let g = p.generation();
        let base = ctl.snapshot();
        for at in [1, 2, 3] {
            let mut copy = SecurePool::open_with_entropy(
                base.device(),
                POOL,
                &admin(),
                Fail { at, calls: 0 },
            )?;
            copy.unlock(domain, Credential::Recovery(&recovery()))?;
            assert!(copy.write_file(id, "/f", 0, &[17; K]).is_err());
            let disk = copy.into_device();
            disk.control().power_cut(Persistence::Newest(10))?;
            let mut recovered = opened(disk, domain)?;
            assert_eq!(recovered.generation(), g);
            assert_eq!(lire(&mut recovered, id)?, b"dernier etat durable");
            recovered.check()?;
        }
    }
    Ok(())
}

#[test]
fn dommage_combine_aucune_reparation_avant_controle_du_graphe() -> Result<()> {
    for sector in [512, 4096] {
        let (mut p, ctl) = pool(1800, sector)?;
        let (v, domain) = volume(&mut p, 50, Compression::None)?;
        p.write_file(v, "/f", 0, &donnees(3 * K, "aleatoire", 18))?;
        let generation = p.generation();
        let disk = p.into_device();
        let (address, _) = obj(&disk, domain, generation)?
            .into_iter()
            .find(|(_, h)| h[11] == 1)
            .unwrap();
        let cert = 2 + 2 * (generation % 4) + 1;
        let mut body = disk.read(address + 1)?;
        body[384] ^= 1;
        ctl.replace_stable(address + 1, body)?;
        ctl.replace_stable(cert, [0; K])?;
        let before = ctl.snapshot().bytes()?;
        let mut reloaded = opened(disk, domain)?;
        ctl.trace(None, None);
        assert!(reloaded.write_file(v, "/f", 0, b"tentative").is_err());
        assert_eq!(
            ctl.events()
                .iter()
                .filter(|e| matches!(e, Event::BeforeWrite(_)))
                .count(),
            0
        );
        assert_eq!(ctl.snapshot().bytes()?, before);
    }
    Ok(())
}

fn refus_sans_ecriture(p: &mut Pool, ctl: &SimControl, volume: u64) -> Result<()> {
    let generation = p.generation();
    let avant = ctl.snapshot().bytes()?;
    ctl.trace(None, None);
    assert!(matches!(
        p.write_file(volume, "/f", 0, b"tentative"),
        Err(Error::Corrupt(_) | Error::Io(_))
    ));
    assert!(matches!(
        p.write_file(volume, "/f", 0, b"bis"),
        Err(Error::Unavailable)
    ));
    assert_eq!(p.generation(), generation);
    assert!(
        !ctl.events()
            .iter()
            .any(|e| matches!(e, Event::BeforeWrite(_)))
    );
    assert_eq!(avant, ctl.snapshot().bytes()?);
    Ok(())
}

fn alterer(ctl: &SimControl, bloc: u64, octet: usize) -> Result<()> {
    let mut page = ctl.snapshot().device().read(bloc)?;
    page[octet] ^= 1;
    ctl.replace_stable(bloc, page)
}

#[test]
fn p71_entete_apres_ouverture_avant_reparation() -> Result<()> {
    for secteur in [512, 4096] {
        let (mut p, ctl) = pool(1800, secteur)?;
        let (v, domaine) = volume(&mut p, 21, Compression::None)?;
        p.write_file(v, "/f", 0, &[71; 8192])?;
        let certificat = 3 + 2 * (p.generation() % 4);
        let disk = p.into_device();
        ctl.replace_stable(certificat, [0; K])?;
        let mut p = opened(disk, domaine)?;
        p.check()?;
        alterer(&ctl, 1, 80)?;
        refus_sans_ecriture(&mut p, &ctl, v)?;
    }
    Ok(())
}

#[test]
fn p71_banque_apres_publication_en_mode_inscriptible() -> Result<()> {
    for secteur in [512, 4096] {
        let (mut p, ctl) = pool(1800, secteur)?;
        let (v, _) = volume(&mut p, 21, Compression::None)?;
        p.write_file(v, "/f", 0, &[71; 8192])?;
        p.advance()?;
        p.check()?;
        alterer(&ctl, 10 + 9 * (p.generation() % 4) + 8, 0)?;
        refus_sans_ecriture(&mut p, &ctl, v)?;
    }
    Ok(())
}

#[test]
fn p71_dommages_combines_et_erreurs_de_lecture() -> Result<()> {
    for secteur in [512, 4096] {
        let (mut p, ctl) = pool(1800, secteur)?;
        let (v, domaine) = volume(&mut p, 21, Compression::None)?;
        p.write_file(v, "/f", 0, &[71; 8192])?;
        let naissance = p.generation();
        let snapshot = p.snapshot(v)?;
        let clone = p.clone_snapshot(snapshot, OWNER)?;
        let g = p.generation();
        let base = ctl.snapshot();
        let objets = obj(&base.device(), domaine, naissance)?;
        let donnees = objets.iter().find(|(_, h)| h[11] == 1).unwrap().0;
        let racine = objets
            .iter()
            .find(|(_, h)| h[48..56] == (1u64 << 32).to_le_bytes())
            .unwrap()
            .0;
        let tete = 10 + 9 * (g % 4);
        let parent = 10 + 9 * ((g - 1) % 4);
        let certificat = 2 + 2 * (g % 4);
        for dommages in [
            vec![(1, 80), (certificat, 4095)],
            vec![(tete, 32), (certificat, 4095)],
            vec![(tete + 8, 0), (donnees + 1, 19)],
            vec![(racine + 1, 7), (donnees, 48)],
            vec![(donnees, 112), (racine, 88)],
            vec![(parent + 8, 1), (tete + 8, 2)],
            vec![(certificat, 4095), (certificat + 1, 4095)],
            vec![(0, 80), (1, 80)],
        ] {
            let d = base.device();
            let ctl = d.control();
            let mut p = opened(d, domaine)?;
            for (bloc, offset) in dommages {
                alterer(&ctl, bloc, offset)?;
            }
            refus_sans_ecriture(&mut p, &ctl, clone)?;
        }
        for bloc in [0, 1, certificat, parent, tete + 8, racine, donnees + 1] {
            let d = base.device();
            let ctl = d.control();
            ctl.replace_stable(certificat + 1, [0; K])?;
            let mut p = opened(d, domaine)?;
            ctl.inject(SimFault::Read { block: bloc })?;
            refus_sans_ecriture(&mut p, &ctl, clone)?;
        }
    }
    Ok(())
}

#[test]
fn p71_interruption_de_la_reparation_et_publication() -> Result<()> {
    for secteur in [512, 4096] {
        let (mut p, ctl) = pool(1800, secteur)?;
        let (v, domaine) = volume(&mut p, 21, Compression::None)?;
        p.write_file(v, "/f", 0, b"avant")?;
        let g = p.generation();
        drop(p);
        let tete = 3 + 2 * (g % 4);
        let parent = 3 + 2 * ((g - 1) % 4);
        ctl.replace_stable(tete, [0; K])?;
        ctl.replace_stable(parent, [0; K])?;
        let base = ctl.snapshot();
        let d = base.device();
        let ctl = d.control();
        let mut p = opened(d, domaine)?;
        ctl.trace(None, None);
        p.write_file(v, "/f", 0, b"apres")?;
        let trace = ctl.events();
        let cibles: Vec<_> = trace
            .iter()
            .filter_map(|e| match e {
                Event::BeforeWrite(b) => Some(*b),
                _ => None,
            })
            .collect();
        assert_eq!(&cibles[..2], &[tete, parent]);
        assert_eq!(
            trace
                .iter()
                .filter(|e| matches!(e, Event::FlushCompleted(_)))
                .count(),
            4
        );
        for point in 0..=trace.len() {
            for persistance in [Persistence::None, Persistence::Newest(2)] {
                let d = base.device();
                let ctl = d.control();
                let mut p = opened(d, domaine)?;
                ctl.trace(Some(point), None);
                let confirme = p.write_file(v, "/f", 0, b"apres").is_ok();
                assert_eq!(confirme, point == trace.len());
                ctl.power_cut(persistance)?;
                let mut reprise = reouvrir(&ctl.snapshot(), &[domaine])?;
                reprise.check()?;
                let contenu = lire(&mut reprise, v)?;
                match reprise.generation() {
                    n if n == g => {
                        assert!(!confirme);
                        assert_eq!(contenu, b"avant");
                    }
                    n if n == g + 1 => assert_eq!(contenu, b"apres"),
                    n => panic!("génération inattendue {n}"),
                }
            }
        }
        eprintln!(
            "P71_REPRISE secteur={secteur} coupures={}",
            2 * (trace.len() + 1)
        );
    }
    Ok(())
}

#[test]
fn p71_ecrivains_lecteur_et_modification_externe() -> Result<()> {
    use std::sync::{Arc, mpsc};
    use std::time::Duration;
    // Les descripteurs du banc partagent réellement le support et son verrou.
    struct Session {
        disk: Arc<Mutex<SimDevice>>,
        rendez_vous: Arc<Mutex<Option<(mpsc::Sender<()>, mpsc::Receiver<()>)>>>,
    }
    impl Device for Session {
        fn mutation_lock(&self) -> Option<Arc<Mutex<()>>> {
            self.disk.lock().unwrap().mutation_lock()
        }
        fn blocks(&self) -> u64 {
            self.disk.lock().unwrap().blocks()
        }
        fn read(&self, b: u64) -> Result<Block> {
            if let Some((arrive, continuer)) = self.rendez_vous.lock().unwrap().take() {
                arrive.send(()).unwrap();
                continuer.recv_timeout(Duration::from_secs(10)).unwrap();
            }
            self.disk.lock().unwrap().read(b)
        }
        fn write(&mut self, b: u64, p: &Block) -> Result<()> {
            self.disk.lock().unwrap().write(b, p)
        }
        fn flush(&mut self) -> Result<()> {
            self.disk.lock().unwrap().flush()
        }
    }
    for secteur in [512, 4096] {
        let (p, ctl) = pool(1800, secteur)?;
        let disk = Arc::new(Mutex::new(p.into_device()));
        let session = || Session {
            disk: disk.clone(),
            rendez_vous: Arc::default(),
        };
        let mut lecteur = SecurePool::open(session(), POOL, &admin())?;
        let mut second = SecurePool::open(session(), POOL, &admin())?;
        let (arrive, attende) = mpsc::channel();
        let (libere, continuer) = mpsc::channel();
        // Armer après le montage garantit que la pause survient dans change.
        let dev = session();
        let rendez_vous = dev.rendez_vous.clone();
        let mut premier = SecurePool::open(dev, POOL, &admin())?;
        *rendez_vous.lock().unwrap() = Some((arrive, continuer));
        let travail = std::thread::spawn(move || premier.advance());
        attende.recv_timeout(Duration::from_secs(10)).unwrap();
        let bloque = second.advance();
        let lecture = lecteur.check();
        let externe = ctl.replace_stable(0, [0; K]);
        libere.send(()).unwrap();
        travail.join().unwrap()?;
        assert!(matches!(bloque, Err(Error::Busy)));
        assert!(matches!(lecture, Err(Error::Busy)));
        assert!(matches!(externe, Err(Error::Busy)));
        ctl.trace(None, None);
        assert!(matches!(second.advance(), Err(Error::Corrupt(_))));
        assert!(matches!(lecteur.check(), Err(Error::Corrupt(_))));
        assert!(
            !ctl.events()
                .iter()
                .any(|e| matches!(e, Event::BeforeWrite(_)))
        );
        SecurePool::open(session(), POOL, &admin())?.check()?;
    }
    Ok(())
}

#[test]
fn p71_descripteurs_externes_sans_exclusivite() -> Result<()> {
    use novafs::device::FileImage;
    for secteur in [512, 4096] {
        let fichier = tempfile::NamedTempFile::new()?;
        fichier.as_file().set_len(1800 * 4096)?;
        let import = || FileImage::from_file(fichier.reopen()?, 1800, secteur);
        let vide = std::fs::read(fichier.path())?;
        assert!(matches!(
            SecurePool::format(import()?, POOL, &admin()),
            Err(Error::ReadOnly)
        ));
        assert_eq!(std::fs::read(fichier.path())?, vide);
        let prive = SecurePool::format(FileImage::create(1800, secteur)?, POOL, &admin())?;
        std::io::copy(&mut prive.into_device().into_file(), &mut fichier.reopen()?)?;
        let base = std::fs::read(fichier.path())?;
        let mut lecteur = SecurePool::open(import()?, POOL, &admin())?;
        let mut second = SecurePool::open(import()?, POOL, &admin())?;
        lecteur.check()?;
        assert!(matches!(second.advance(), Err(Error::ReadOnly)));
        assert_eq!(std::fs::read(fichier.path())?, base);
        // Descripteur indépendant sur le même inode. Le changement survient
        // entre deux diagnostics en lecture seule, sans course pendant la lecture.
        use std::os::unix::fs::FileExt;
        let offset = K + 80;
        fichier
            .as_file()
            .write_all_at(&[base[offset] ^ 1], offset as u64)?;
        fichier.as_file().sync_all()?;
        let alteree = std::fs::read(fichier.path())?;
        assert!(matches!(lecteur.check(), Err(Error::Corrupt(_))));
        assert!(matches!(lecteur.advance(), Err(Error::Unavailable)));
        assert_eq!(std::fs::read(fichier.path())?, alteree);
    }
    Ok(())
}

#[test]
fn p71_corruption_avant_drain_du_cache_volatil() -> Result<()> {
    for secteur in [512, 4096] {
        let (mut p, ctl) = pool(1800, secteur)?;
        let (v, domaine) = volume(&mut p, 21, Compression::None)?;
        p.write_file(v, "/f", 0, &[71; 8192])?;
        let g = p.generation();
        let mut disk = p.into_device();
        let (objet, _) = obj(&disk, domaine, g)?
            .into_iter()
            .find(|(_, h)| h[11] == 1)
            .unwrap();
        alterer(&ctl, objet + 1, 384)?;
        ctl.replace_stable(3 + 2 * (g % 4), [0; K])?;
        // Écriture acceptée avant reprise sur un bloc libre de cette petite vue.
        let libre = disk.blocks() - 1;
        assert_eq!(disk.read(libre)?, [0; K]);
        disk.write(libre, &[0x52; K])?;
        let mut p = opened(disk, domaine)?;
        refus_sans_ecriture(&mut p, &ctl, v)?;
        assert!(
            !ctl.events()
                .iter()
                .any(|e| matches!(e, Event::FlushSubmitted(_)))
        );
    }
    Ok(())
}

/// Compte les accès à une racine étrangère. La sonde n'accède pas à l'index
/// du moteur et reste indépendante des horloges et des features de mesure.
struct RootReads {
    disk: SimDevice,
    body: u64,
    count: std::rc::Rc<std::cell::Cell<usize>>,
}
impl Device for RootReads {
    fn mutation_lock(&self) -> Option<std::sync::Arc<Mutex<()>>> {
        self.disk.mutation_lock()
    }
    fn blocks(&self) -> u64 {
        self.disk.blocks()
    }
    fn read(&self, b: u64) -> Result<Block> {
        if b == self.body {
            self.count.set(self.count.get() + 1);
        }
        self.disk.read(b)
    }
    fn write(&mut self, b: u64, page: &Block) -> Result<()> {
        self.disk.write(b, page)
    }
    fn flush(&mut self) -> Result<()> {
        self.disk.flush()
    }
}

#[test]
fn p72_racine_etrangere_controlee_dans_chaque_transaction() -> Result<()> {
    for sector in [512, 4096] {
        let (mut p, ctl) = pool(2600, sector)?;
        let (v, d) = volume(&mut p, 20, Compression::Lz4)?;
        let (other, foreign) = volume(&mut p, 40, Compression::Lz4)?;
        let bytes = donnees(8 * K, "mixte", 12);
        p.write_file(other, "/f", 0, &bytes)?;
        let body = obj(&ctl.snapshot().device(), foreign, p.generation())?
            .into_iter()
            .find(|(_, h)| h[48..56] == (1u64 << 32).to_le_bytes())
            .expect("racine")
            .0
            + 1;
        let snap = p.snapshot(other)?;
        let clone = p.clone_snapshot(snap, OWNER)?;
        p.remove(other)?;
        let count = std::rc::Rc::new(std::cell::Cell::new(0));
        let mut p = SecurePool::open(
            RootReads {
                disk: p.into_device(),
                body,
                count: count.clone(),
            },
            POOL,
            &admin(),
        )?;
        for domain in [d, foreign] {
            p.unlock(domain, Credential::Recovery(&recovery()))?;
        }
        p.advance()?;
        for value in [11, 12] {
            count.set(0);
            p.write_file(v, "/f", 0, &[value; K])?;
            // Une authentification lors du pin complet, puis une énumération
            // indépendante des LBA. Le remontage FS utilise leurs arêtes vérifiées.
            assert_eq!(count.get(), 2);
            assert_eq!(p.read_file(clone, "/f", 0, bytes.len())?, bytes);
        }
        // Même sur un écrivain déjà actif, une réparation met fin au prêt
        // du premier index. Le montage physique indépendant relit la racine.
        let certificate = 2 + 2 * (p.generation() % 4);
        ctl.replace_stable(certificate, [0; K])?;
        count.set(0);
        ctl.trace(None, None);
        p.write_file(v, "/f", 0, b"apres reparation")?;
        assert!(count.get() > 2);
        assert_eq!(
            ctl.events()
                .iter()
                .filter(|e| matches!(e, Event::FlushCompleted(_)))
                .count(),
            3
        );
        let g = p.generation();
        alterer(&ctl, body, 40)?;
        let stable = ctl.snapshot().bytes()?;
        ctl.trace(None, None);
        assert!(matches!(
            p.write_file(v, "/f", 0, b"refus"),
            Err(Error::Corrupt(_))
        ));
        assert_eq!(p.generation(), g);
        assert!(
            !ctl.events()
                .iter()
                .any(|e| matches!(e, Event::BeforeWrite(_)))
        );
        assert_eq!(ctl.snapshot().bytes()?, stable);
    }
    Ok(())
}

#[test]
fn p72_banques_hors_rotation_et_predecesseur() -> Result<()> {
    for sector in [512, 4096] {
        let (mut p, ctl) = pool(1800, sector)?;
        let (v, domain) = volume(&mut p, 21, Compression::Lz4)?;
        p.write_file(v, "/f", 0, &[77; K])?;
        p.advance()?;
        let g = p.generation();
        // Cette banque ne protège aucune vue. La prochaine publication la
        // remplace en entier sous F1 avant d'émettre ses attestations.
        alterer(&ctl, 10 + 9 * ((g + 1) % 4) + 8, 12)?;
        p.write_file(v, "/f", 0, b"suivant")?;
        let mut reopened = reouvrir(&ctl.snapshot(), &[domain])?;
        reopened.check()?;
        assert_eq!(reopened.read_file(v, "/f", 0, 7)?, b"suivant");
        // La génération g n'est plus la tête, mais reste un parent protégé.
        alterer(&ctl, 10 + 9 * (g % 4) + 8, 12)?;
        refus_sans_ecriture(&mut p, &ctl, v)?;
    }
    Ok(())
}
