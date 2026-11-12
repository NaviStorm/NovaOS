//! Échange de disques jetables avec le banc VirtIO. La transaction s'exécute
//! dans le moteur hôte ; le noyau de mesure ne reçoit que ses écritures et F1/F2.
use super::*;
use std::{fs, io::Write, path::Path};

fn creer(p: &Path, contenu: &[u8]) {
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(p)
        .unwrap()
        .write_all(contenu)
        .unwrap();
}

#[test]
#[ignore = "piloté par novaOS/tests/atomicite_ancres.py sur des fichiers de laboratoire"]
fn images_du_banc_utm() -> Result<()> {
    let racine = std::env::var_os("NOVAFS_ANCRES_UTM").expect("répertoire du banc");
    let racine = Path::new(&racine);
    let verification = racine.join("relectures.tsv");
    for s in [512, 4096] {
        for copie in [false, true] {
            let cas = Cas::neuf(s, copie, copie)?;
            let dossier = racine.join(format!("{s}-{}", if copie { "cp" } else { "mv" }));
            let (mut moteur, ctl) = cas.ouvrir()?;
            let ancien = moteur.receipt();
            ctl.trace(None, None);
            remplacer(&mut moteur, cas.source, cas.destination)?;
            let nouveau = moteur.receipt();
            cas.verifier(&moteur, true)?;
            if !verification.exists() {
                fs::create_dir(&dossier).unwrap();
                creer(&dossier.join("avant.raw"), &cas.image.bytes()?);
                creer(&dossier.join("apres.raw"), &ctl.snapshot().bytes()?);
                let mut ordre = String::new();
                let mut blocs = alloc::collections::BTreeSet::new();
                for e in ctl.events() {
                    match e {
                        Event::WriteCompleted(b) => {
                            // La page finale est bien le contenu de l'unique WRITE à cette adresse.
                            assert!(blocs.insert(b));
                            ordre.push_str(&format!("W {b}\n"));
                        }
                        Event::FlushCompleted(n) => ordre.push_str(&format!("F{n}\n")),
                        _ => (),
                    }
                }
                assert!(ordre.ends_with(&format!("F1\nW {}\nF2\n", nouveau.birth % 2)));
                creer(&dossier.join("ordre.txt"), ordre.as_bytes());
                continue;
            }
            let mut nombre = 0;
            for ligne in fs::read_to_string(&verification).unwrap().lines() {
                let champs: Vec<_> = ligne.split('\t').collect();
                assert_eq!(champs.len(), 3);
                if dossier.file_name().unwrap() != champs[0] {
                    continue;
                }
                let octets = fs::read(dossier.join(champs[1])).unwrap();
                for mode in [Mode::ReadOnly, Mode::ReadWrite] {
                    let d = SimDevice::from_bytes(&octets, s)?;
                    let controle = d.control();
                    controle.trace(None, None);
                    let montage = Engine::mount_current(d, IDS, mode);
                    match champs[2] {
                        "refus" => {
                            assert!(matches!(montage, Err(Error::Corrupt(_))));
                            assert!(controle.events().is_empty());
                            let secours = Engine::salvage(
                                controle.snapshot().device(),
                                IDS,
                                ancien.birth % 2,
                            )?;
                            assert_eq!(secours.receipt(), ancien);
                            cas.verifier(&secours, false)?;
                        }
                        "ancien" | "nouveau" => {
                            let publie = champs[2] == "nouveau";
                            let relu = montage?;
                            assert_eq!(relu.receipt(), if publie { nouveau } else { ancien });
                            cas.verifier(&relu, publie)?;
                        }
                        _ => panic!("état d'ancre inattendu"),
                    }
                    assert!(controle.snapshot().bytes()? == octets);
                }
                nombre += 1;
            }
            assert!(nombre > 0);
            println!("RELECTURES_UTM secteur={s} copie={copie} images={nombre}");
        }
    }
    Ok(())
}
