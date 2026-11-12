//! Adaptateur du banc PC1. Les images à contrôler arrivent uniquement sur stdin.
use novafs::crypto::{Entropy, HostEntropy, Secret};
#[cfg(feature = "latency-probes")]
use novafs::latency_probes;
use novafs::secure_unit::{self as unite, Context, Keys, Kind};
use novafs::{BLOCK_BYTES, Block, Error, Result};
use novafs::{Engine, Ids, Mode, device::SimDevice};
use std::collections::BTreeSet;
use std::io::{Read, Write};
use std::{env, fs, io, path::Path};

// Réemploie le packer et le décodeur P6 privés, sans étendre l'API du moteur.
#[allow(dead_code)]
#[path = "../src/compression.rs"]
mod compression;
#[path = "../lab/pc1/vecteur.rs"]
mod vecteur;

#[derive(Default)]
struct Tirages {
    valeurs: BTreeSet<Vec<u8>>,
    appels: usize,
    panne: bool,
}
impl Entropy for Tirages {
    fn fill(&mut self, out: &mut [u8]) -> Result<()> {
        self.appels += 1;
        if self.panne {
            return Err(Error::Unavailable);
        }
        assert_eq!(out.len(), 24);
        HostEntropy.fill(out)?;
        assert!(self.valeurs.insert(out.to_vec()));
        Ok(())
    }
}

fn contexte(p: u64) -> (Context, Option<Keys>, Block) {
    assert!([32, 34, 36, 38].contains(&p));
    let scelle = p % 4 == 2;
    let domaine = if scelle { [9; 16] } else { [0; 16] };
    let keys = scelle.then(|| Keys::derive(&Secret::from_bytes([17; 32]), &domaine).unwrap());
    let pack = p >= 36;
    let mut plain = [p as u8; BLOCK_BYTES];
    if pack {
        let mut enc = compression::Packer::default();
        enc.push(0, &plain).unwrap();
        enc.push(1, &[p as u8 + 1; BLOCK_BYTES]).unwrap();
        plain = enc.finish().unwrap();
    }
    (
        Context {
            pool: [2; 16],
            domain: domaine,
            physical: p,
            logical: 0,
            generation: 1,
            kind: if pack { Kind::Packed } else { Kind::Data },
        },
        keys,
        plain,
    )
}

fn verifier(p: u64, h: &Block, d: &Block, hash: &[u8; 32]) -> Result<()> {
    let (ctx, keys, attendu) = contexte(p);
    let clair = unite::open(ctx, keys.as_ref(), h, d, hash)?;
    if clair != attendu {
        return Err(Error::Corrupt("contenu inattendu du banc"));
    }
    if ctx.kind == Kind::Packed {
        let pack = compression::inspect(&clair)?;
        pack.verify_all()?;
        assert_eq!(pack.page(0)?, [p as u8; BLOCK_BYTES]);
        assert_eq!(pack.page(1)?, [p as u8 + 1; BLOCK_BYTES]);
    }
    Ok(())
}

fn fabriquer(dir: &Path) {
    fs::create_dir(dir).unwrap();
    let mut rng = Tirages::default();
    for p in [32, 34, 36, 38] {
        let (ctx, keys, plain) = contexte(p);
        let (h, d, hash) = unite::seal(ctx, keys.as_ref(), &mut rng, &plain).unwrap();
        verifier(p, &h, &d, &hash).unwrap();
        let avant = rng.appels;
        for _ in 0..3 {
            verifier(p, &h, &d, &hash).unwrap();
        }
        assert_eq!(avant, rng.appels);
        if let Some(k) = keys.as_ref() {
            assert!(unite::open(ctx, None, &h, &d, &hash).is_err());
            let autre = Keys::derive(&Secret::from_bytes([18; 32]), &ctx.domain).unwrap();
            assert!(unite::open(ctx, Some(&autre), &h, &d, &hash).is_err());
            for at in [32, 72, 76, 88, 112] {
                let mut altere = h;
                altere[at] ^= 1;
                assert!(
                    unite::open(ctx, Some(k), &altere, &d, &unite::fingerprint(&altere, &d))
                        .is_err()
                );
            }
            // Écriture abandonnée et nouvel essai dans la même lignée.
            for _ in 0..2 {
                let (h2, d2, hash2) = unite::seal(ctx, Some(k), &mut rng, &plain).unwrap();
                verifier(p, &h2, &d2, &hash2).unwrap();
                assert_ne!(h[88..112], h2[88..112]);
            }
            // Export restauré et branche indépendante : nouveau domaine et nouvelle maîtresse.
            let mut domaines = BTreeSet::from([ctx.domain]);
            for _ in 0..2 {
                let mut domaine = [0; 16];
                HostEntropy.fill(&mut domaine).unwrap();
                assert!(domaines.insert(domaine));
                let keys =
                    Keys::derive(&Secret::generate(&mut HostEntropy).unwrap(), &domaine).unwrap();
                let branche = Context {
                    domain: domaine,
                    ..ctx
                };
                let (h2, d2, hash2) = unite::seal(branche, Some(&keys), &mut rng, &plain).unwrap();
                assert_eq!(
                    unite::open(branche, Some(&keys), &h2, &d2, &hash2).unwrap(),
                    plain
                );
                assert!(unite::open(branche, Some(k), &h2, &d2, &hash2).is_err());
                assert!(unite::open(ctx, Some(k), &h2, &d2, &hash2).is_err());
            }
            rng.panne = true;
            assert!(matches!(
                unite::seal(ctx, Some(k), &mut rng, &plain),
                Err(Error::Unavailable)
            ));
            rng.panne = false;
        }
        assert!(
            unite::open(
                Context {
                    physical: p + 1,
                    ..ctx
                },
                keys.as_ref(),
                &h,
                &d,
                &hash
            )
            .is_err()
        );
        let mut file = fs::File::create_new(dir.join(format!("{p}.paire"))).unwrap();
        file.write_all(&h).unwrap();
        file.write_all(&d).unwrap();
    }
    assert_eq!((rng.appels, rng.valeurs.len()), (12, 10));
    println!(
        "4 paires, 12 demandes, 10 nonces distincts, 2 pannes d'entropie, partage sans tirage"
    );
}

fn ancien(data: &[u8], s: u32) {
    let mut dev = SimDevice::from_bytes(data, s).unwrap();
    let ctrl = dev.control();
    ctrl.trace(None, None);
    let ids = Ids {
        fs: [1; 16],
        lv: [2; 16],
    };
    for mode in [Mode::ReadOnly, Mode::ReadWrite] {
        assert!(matches!(
            Engine::mount_current(&mut dev, ids, mode),
            Err(Error::Corrupt(_))
        ));
    }
    for slot in [0, 1] {
        assert!(matches!(
            Engine::salvage(&mut dev, ids, slot),
            Err(Error::Corrupt(_))
        ));
    }
    assert!(ctrl.events().is_empty());
}

fn main() {
    let args: Vec<_> = env::args().collect();
    if args[1] == "créer" {
        fabriquer(Path::new(&args[2]));
        return;
    }
    let mut data = Vec::new();
    io::stdin().read_to_end(&mut data).unwrap();
    match args[1].as_str() {
        "vecteur" => {
            let (etat, rang) = vecteur::relire(&data);
            println!("{etat} {rang}");
        }
        "p9" => ancien(&data, args[2].parse().unwrap()),
        "ouvrir" => {
            assert_eq!(data.len(), 2 * BLOCK_BYTES);
            let hash: [u8; 32] = core::array::from_fn(|i| {
                u8::from_str_radix(&args[3][2 * i..2 * i + 2], 16).unwrap()
            });
            if verifier(
                args[2].parse().unwrap(),
                data[..BLOCK_BYTES].try_into().unwrap(),
                data[BLOCK_BYTES..].try_into().unwrap(),
                &hash,
            )
            .is_err()
            {
                std::process::exit(1);
            }
        }
        _ => panic!("opération du banc inconnue"),
    }
}
