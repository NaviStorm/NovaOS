//! Reproduction sur des images simulées jetables avec le moteur P8.
//! Les secrets ci-dessous servent exclusivement à ce banc.
use novafs::crypto::{Credential, Entropy, HostEntropy, Secret};
use novafs::device::{Event, Persistence, SimControl, SimDevice, SimSnapshot};
use novafs::secure_pool::SecurePool;
use novafs::{Compression, Error, Ids, Result};
use std::cell::Cell;

type Pool = SecurePool<SimDevice>;
const PHRASE: &[u8] = b"phrase du banc de migration P9";
const OWNER: [u8; 16] = [40; 16];

fn pool(sector: u32, id: u8, admin: &Secret) -> Result<(Pool, SimControl)> {
    let disk = SimDevice::new(3000, sector)?;
    let control = disk.control();
    Ok((Pool::format(disk, [id; 16], admin)?, control))
}

fn volume(pool: &mut Pool, id: u8, recovery: &Secret) -> Result<u64> {
    pool.create(
        OWNER,
        Ids {
            fs: [id; 16],
            lv: [id + 1; 16],
        },
        256,
        Compression::Lz4,
        Some((PHRASE, recovery)),
    )
}

fn has_submission(trace: &SimControl) -> bool {
    trace
        .events()
        .iter()
        .any(|e| matches!(e, Event::WriteSubmitted(_) | Event::FlushSubmitted(_)))
}

struct Nonces<'a>(&'a Cell<u64>);

impl Entropy for Nonces<'_> {
    fn fill(&mut self, out: &mut [u8]) -> Result<()> {
        HostEntropy.fill(out)?;
        if out.len() == 24 {
            self.0.set(self.0.get() + 1);
        }
        Ok(())
    }
}

fn essai_abandonne(
    image: &SimSnapshot,
    admin: &Secret,
    domaines: [([u8; 16], &Secret); 2],
    origine: u64,
    attendu: &[u8],
) -> Result<()> {
    let disk = image.device();
    let ctl = disk.control();
    let tirages = Cell::new(0);
    let mut p = SecurePool::open_with_entropy(disk, [5; 16], admin, Nonces(&tirages))?;
    for (d, cle) in domaines {
        p.unlock(d, Credential::Recovery(cle))?;
    }
    // Première mutation acquittée sur cette copie indépendante, drain de reprise inclus.
    p.rename(origine, "/renomme", "/avant")?;
    let stable = ctl.snapshot();
    let generation = p.generation();
    let avant = tirages.get();
    // Le moteur scelle l'unité avant de présenter son en-tête au support.
    ctl.trace(Some(0), None);
    assert!(p.rename(origine, "/avant", "/apres").is_err());
    assert!(matches!(ctl.events().as_slice(), [Event::BeforeWrite(_)]));
    assert_eq!(tirages.get() - avant, 1);
    drop(p);
    ctl.power_cut(Persistence::None)?;
    assert_eq!(stable.bytes()?, ctl.snapshot().bytes()?);
    let mut p = Pool::open(ctl.snapshot().device(), [5; 16], admin)?;
    for (d, cle) in domaines {
        p.unlock(d, Credential::Recovery(cle))?;
    }
    assert_eq!(p.generation(), generation);
    assert_eq!(p.read_file(origine, "/avant", 0, attendu.len())?, attendu);
    assert!(matches!(
        p.read_file(origine, "/apres", 0, 1),
        Err(Error::NotFound)
    ));
    p.check()
}

fn audit(sector: u32) -> Result<()> {
    let admin = Secret::from_bytes([1; 32]);
    let recovery_a = Secret::from_bytes([2; 32]);
    let recovery_b = Secret::from_bytes([3; 32]);
    let backup = Secret::from_bytes([4; 32]);
    let (mut source, source_ctl) = pool(sector, 5, &admin)?;
    let a = volume(&mut source, 10, &recovery_a)?;
    let b = volume(&mut source, 20, &recovery_b)?;
    let domain_a = source.domain(a)?;
    let domain_b = source.domain(b)?;
    let initial = vec![0x51; 16 * 1024];
    source.create_file(a, "/f")?;
    source.write_file(a, "/f", 0, &initial)?;
    source.create_file(b, "/temoin")?;
    source.write_file(b, "/temoin", 0, b"B")?;
    source.lock(domain_b);
    let before = source.generation();
    let stable = source_ctl.snapshot();
    source_ctl.trace(None, None);
    assert_eq!(source.read_file(a, "/f", 0, initial.len())?, initial);
    assert!(matches!(
        source.read_file(b, "/temoin", 0, 1),
        Err(Error::ReadOnly)
    ));
    // Une mutation de métadonnées existante emprunte le chemin de publication du pool.
    assert!(matches!(
        source.rename(a, "/f", "/renomme"),
        Err(Error::ReadOnly)
    ));
    assert_eq!(source.generation(), before);
    assert!(!has_submission(&source_ctl));
    assert_eq!(stable.bytes()?, source_ctl.snapshot().bytes()?);
    source.unlock(domain_b, Credential::Recovery(&recovery_b))?;
    source.rename(a, "/f", "/renomme")?;
    assert_eq!(source.read_file(a, "/renomme", 0, initial.len())?, initial);
    assert!(source.generation() > before && has_submission(&source_ctl));

    let snapshot = source.snapshot(a)?;
    let clone = source.clone_snapshot(snapshot, OWNER)?;
    let mut diverged = initial.clone();
    diverged[4095..4098].copy_from_slice(b"XYZ");
    source.write_file(clone, "/renomme", 4095, b"XYZ")?;
    assert_eq!(source.domain(snapshot)?, domain_a);
    assert_eq!(source.domain(clone)?, domain_a);
    source.check()?;
    let snap_receipt = source.receipt(snapshot)?;
    let clone_receipt = source.receipt(clone)?;
    let stable = source_ctl.snapshot();
    source_ctl.trace(None, None);
    let mut snap_archive = Vec::new();
    let mut clone_archive = Vec::new();
    source.export(snapshot, sector, &backup, &mut snap_archive)?;
    source.export(clone, sector, &backup, &mut clone_archive)?;
    essai_abandonne(
        &stable,
        &admin,
        [(domain_a, &recovery_a), (domain_b, &recovery_b)],
        a,
        &initial,
    )?;

    for reverse in [false, true] {
        let (first, expected, second) = if reverse {
            (&clone_archive, &diverged, &snap_archive)
        } else {
            (&snap_archive, &initial, &clone_archive)
        };
        // Dans un des témoins, faire coïncider toutes les identités publiques
        // du snapshot, jusqu'au numéro de vue. Les supports restent indépendants.
        let pool_id = if reverse { 6 } else { 5 };
        let (mut dst, dst_ctl) = pool(sector, pool_id, &admin)?;
        if !reverse {
            for i in 1..snapshot {
                let n = 80 + 2 * u8::try_from(i).expect("petit catalogue de test");
                let ids = Ids {
                    fs: [n; 16],
                    lv: [n + 1; 16],
                };
                assert_eq!(dst.create(OWNER, ids, 256, Compression::None, None)?, i);
            }
        }
        let vues_avant = dst.list()?.len();
        let first_id = dst.restore(
            &first[..],
            &backup,
            OWNER,
            PHRASE,
            &recovery_a,
            Compression::Lz4,
        )?;
        let restored_domain = dst.domain(first_id)?;
        assert_ne!(restored_domain, domain_a);
        if !reverse {
            let info = |vues: Vec<novafs::shared::ObjectInfo>| {
                vues.into_iter()
                    .find(|v| v.id == snapshot)
                    .expect("snapshot présent")
            };
            assert_eq!(info(source.list()?), info(dst.list()?));
            let nouveau = dst.receipt(first_id)?;
            assert_eq!(
                (nouveau.pool, nouveau.id),
                (snap_receipt.pool, snap_receipt.id)
            );
            assert_ne!(nouveau.root, snap_receipt.root);
            assert!(matches!(
                dst.verify_receipt(&snap_receipt),
                Err(Error::Invalid(_))
            ));
        }
        let before = dst.generation();
        let first_image = dst_ctl.snapshot();
        dst_ctl.trace(None, None);
        assert!(matches!(
            dst.restore(
                &second[..],
                &backup,
                OWNER,
                PHRASE,
                &recovery_a,
                Compression::Lz4
            ),
            Err(Error::Exists)
        ));
        assert_eq!(dst.generation(), before);
        assert!(!has_submission(&dst_ctl));
        assert_eq!(first_image.bytes()?, dst_ctl.snapshot().bytes()?);
        drop(dst);
        let mut reopened = Pool::open(dst_ctl.snapshot().device(), [pool_id; 16], &admin)?;
        reopened.unlock(restored_domain, Credential::Recovery(&recovery_a))?;
        assert_eq!(reopened.list()?.len(), vues_avant + 1);
        assert_eq!(
            reopened.read_file(first_id, "/renomme", 0, expected.len())?,
            *expected
        );
        reopened.check()?;
    }
    assert!(!has_submission(&source_ctl));
    assert_eq!(stable.bytes()?, source_ctl.snapshot().bytes()?);
    drop(source);
    let mut reopened = Pool::open(source_ctl.snapshot().device(), [5; 16], &admin)?;
    reopened.unlock(domain_a, Credential::Recovery(&recovery_a))?;
    reopened.unlock(domain_b, Credential::Recovery(&recovery_b))?;
    reopened.verify_receipt(&snap_receipt)?;
    reopened.verify_receipt(&clone_receipt)?;
    assert_eq!(
        reopened.read_file(snapshot, "/renomme", 0, initial.len())?,
        initial
    );
    assert_eq!(
        reopened.read_file(clone, "/renomme", 0, diverged.len())?,
        diverged
    );
    reopened.check()?;
    println!(
        "secteur={sector} lecture_cible=ok mutation_partielle=ReadOnly soumissions_refus=0 restaurations_isolees=2 second_import=Exists ordres=2 ancien_recu_a_identifiants_egaux=Invalid source_et_recus_intacts=oui scellement_abandonne=1 octets_stables_modifies=0"
    );
    Ok(())
}

fn main() -> Result<()> {
    for sector in [512, 4096] {
        audit(sector)?;
    }
    Ok(())
}
