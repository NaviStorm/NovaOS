//! Publication du même remplacement à travers le graphe CoW et le journal
//! authentifié du pool. Les deux attestations physiques ne sont pas les ancres du LV.
use super::*;
use crate::device::{Event, Persistence, SimDevice};
use crate::fs::remplacer_deux_fichiers_du_banc;

#[test]
fn remplacement_snapshot_clone_et_attestations_dechirees() -> Result<()> {
    let _kdf = crate::KDF_TEST.lock().unwrap();
    for secteur in [512, 4096] {
        let pool = [51; 16];
        let admin = Secret::generate(&mut HostEntropy)?;
        let cle = Secret::generate(&mut HostEntropy)?;
        let mut p =
            SecurePool::format_p9(SimDevice::new(1800, secteur)?, pool, &admin, HostEntropy)?;
        let origine = p.create(
            [1; 16],
            Ids {
                fs: [7; 16],
                lv: [9; 16],
            },
            256,
            Compression::Lz4,
            Some((b"phrase du banc", &cle)),
        )?;
        let domaine = p.domain(origine)?;
        for (nom, contenu) in [("/source", b"nouveau"), ("/cible", b"ancien!")] {
            p.create_file(origine, nom)?;
            p.write_file(origine, nom, 0, contenu)?;
        }
        let instantane = p.snapshot(origine)?;
        let clone = p.clone_snapshot(instantane, [2; 16])?;
        p.write_file(clone, "/cible", 0, b"clone!!")?;
        let g = p.generation();
        let base = p.into_device().control().snapshot();
        let ouvrir = || -> Result<_> {
            let d = base.device();
            let ctl = d.control();
            let mut p = SecurePool::open(d, pool, &admin)?;
            p.unlock(domaine, Credential::Recovery(&cle))?;
            Ok((p, ctl))
        };
        let remplacer = |p: &mut SecurePool<SimDevice>| {
            p.change(domaine, None, |v| {
                v.change(origine, |fs| remplacer_deux_fichiers_du_banc(fs))
            })
        };
        let (mut p, ctl) = ouvrir()?;
        ctl.trace(None, None);
        remplacer(&mut p)?;
        assert_eq!(p.generation(), g + 1);
        let trace = ctl.events();
        let cert = 2 + 2 * ((g + 1) % 4);
        let index = |e| trace.iter().position(|&v| v == e).unwrap();
        let f1 = index(Event::FlushCompleted(2)); // Le premier flush draine l'ancienne session.
        let a = index(Event::WriteCompleted(cert));
        let b = index(Event::WriteCompleted(cert + 1));
        let f2 = index(Event::FlushEffect(3));
        assert!(f1 < a && a < b && b < f2);
        let mut n = 0;
        for (point, bloc, taille, publie) in [
            (f1, cert, 4096, false),
            (a, cert, 96, false),
            (a, cert, 4096, true),
            (b, cert + 1, 96, false),
            (b, cert + 1, 4096, true),
            (f2, cert, 96, true),
        ] {
            let (mut branche, ctl) = ouvrir()?;
            ctl.trace(Some(point), None);
            assert!(remplacer(&mut branche).is_err());
            ctl.power_cut(Persistence::Prefix {
                block: bloc,
                bytes: taille,
            })?;
            let image = ctl.snapshot();
            let d = image.device();
            let lecture = d.control();
            lecture.trace(None, None);
            assert!(SecurePool::open(image.device(), pool, &cle).is_err());
            let mut relu = SecurePool::open(d, pool, &admin)?;
            assert!(relu.inspect_file(origine, "/cible", 0, 8).is_err());
            assert!(relu.unlock(domaine, Credential::Recovery(&admin)).is_err());
            relu.unlock(domaine, Credential::Recovery(&cle))?;
            relu.check()?;
            assert_eq!(relu.generation(), g + u64::from(publie));
            assert_eq!(relu.parent(clone)?, ViewParent::Exact(instantane));
            assert_eq!(relu.inspect_file(instantane, "/source", 0, 8)?, b"nouveau");
            assert_eq!(relu.inspect_file(instantane, "/cible", 0, 8)?, b"ancien!");
            assert_eq!(relu.inspect_file(clone, "/source", 0, 8)?, b"nouveau");
            assert_eq!(relu.inspect_file(clone, "/cible", 0, 8)?, b"clone!!");
            assert_eq!(
                relu.inspect_file(origine, "/cible", 0, 8)?,
                if publie { b"nouveau" } else { b"ancien!" }
            );
            if publie {
                assert!(matches!(
                    relu.inspect_file(origine, "/source", 0, 8),
                    Err(Error::NotFound)
                ));
            } else {
                assert_eq!(relu.inspect_file(origine, "/source", 0, 8)?, b"nouveau");
            }
            assert!(lecture.events().is_empty());
            n += 1;
        }
        println!(
            "POOL_ANCRE secteur={secteur} injections={n} origine_snapshot_clone=vérifiés clés_erronées=refus"
        );
    }
    Ok(())
}
