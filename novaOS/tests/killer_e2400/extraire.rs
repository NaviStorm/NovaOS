//! Extraction pour le banc SDK uniquement. L'autorité de sélection reste
//! le catalogue 4B. Aucune demande de réservation matérielle à cette étape.
mod support;
use novaos_gestionnaire_sim::catalogue::{Blob, Catalog};
use novaos_identity::distribution::{self, Machine};
use std::{fs::File, io::Write, path::PathBuf};
use support::*;

fn main() -> Result<(), String> {
    let dest = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .ok_or("Destination requise")?;
    let mut policy = politique(false, 3);
    policy
        .add_resource(RECU, PROFIL, SIG_RECU)
        .map_err(|e| format!("Profil {e:?}"))?;
    let mut cat = Catalog::new(&policy);
    let digest = cat
        .admit(
            MANIFESTE,
            SIG,
            &[Blob {
                isa: "x86_64",
                bytes: PKG,
            }],
        )
        .map_err(|e| format!("Admission {e:?}"))?;
    let plan = cat
        .select(&cible(), Some(digest))
        .map_err(|e| format!("Sélection {e:?}"))?;
    let key = distribution::public_key(include_bytes!(
        "../../libs/catalogue-pilotes/vecteurs/elf.pub"
    ))
    .map_err(|e| format!("Clé de laboratoire {e:?}"))?;
    let paquet =
        distribution::verify(&key, Machine::X86_64, PKG).map_err(|e| format!("Paquet {e:?}"))?;
    let [membre] = paquet.executables() else {
        return Err("Membre unique attendu".into());
    };
    if membre.sha256 != plan.image_digest() {
        return Err("Membre étranger au plan".into());
    }
    File::create_new(dest)
        .and_then(|mut f| f.write_all(membre.image))
        .map_err(|e| e.to_string())
}
