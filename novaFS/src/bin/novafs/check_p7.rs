//! Modes public et authentifié du vérificateur P7. Aucun secret sur argv ou stdout.
use super::{Failure, FileImage, Receipt, Status, Value, help, storage_failure};
use novafs::Error;
use novafs::secure_pool::{SecurePool, structural_check};
use serde_json::json;

pub(super) fn check(
    args: &[String],
    dev: FileImage,
    with_keys: bool,
) -> Result<(Status, Value), Failure> {
    let usage = |e: Error| (Status::Usage, e.to_string());
    let receipt = match args[0].as_str() {
        "check-pool" => None,
        "check" if args.len() == 6 && args[5].len() == 64 => Some(Receipt {
            pool: crate::identifier(&args[3]).map_err(usage)?,
            id: crate::num(&args[4]).map_err(usage)?,
            root: crate::hex(&args[5])
                .map_err(usage)?
                .try_into()
                .map_err(|_| (Status::Usage, help("check")))?,
        }),
        _ => return Err((Status::Usage, help(&args[0]))),
    };
    if with_keys {
        let mut pool = unlock_fd3(dev)?;
        pool.check().map_err(|e| {
            if matches!(e, Error::ReadOnly) {
                (
                    Status::Incomplete,
                    "il manque une clé nécessaire à la vérification complète".into(),
                )
            } else {
                storage_failure(e)
            }
        })?;
        if let Some(r) = &receipt {
            pool.verify_receipt(r).map_err(storage_failure)?;
        }
        return Ok((
            Status::Coherent,
            json!({"format":"P7","verification":"cryptographique",
            "authenticite_verifiee":true,"generation":pool.generation(),"recu_verifie":receipt.is_some()}),
        ));
    }
    let r = structural_check(&dev).map_err(storage_failure)?;
    if let Some(target) = receipt
        && (r.pool != target.pool || !r.active_views.contains(&target.id))
    {
        return Err((
            Status::Corrupt,
            "cible absente du répertoire structurel P7".into(),
        ));
    }
    Ok((
        Status::Incomplete,
        json!({"format":"P7","verification":"structurelle",
        "authenticite_verifiee":false,"recu_verifie":false,"pool":crate::label(&r.pool),
        "generation_declaree":r.generation,"generations_inspectees":r.protected_generations,
        "attestations_examinees":r.attestations,"vues":r.active_views,
        "copies_protegees_redondantes":r.redundant_copies,
        "references_racine_inspectees":r.checked_root_references,
        "references_racine_chiffrees":r.encrypted_root_references,
        "limites":"MAC des attestations, authenticité des catalogues, tags, contenu chiffré et reçu non vérifiés"}),
    ))
}

fn unlock_fd3(dev: FileImage) -> Result<SecurePool<FileImage>, Failure> {
    let keys = crate::check::key_pipe::read().map_err(|e| (Status::Usage, e.to_string()))?;
    novafs::secure_pool::verify_commit_replicas(&dev, keys.pool, &keys.admin)
        .map_err(storage_failure)?;
    let mut pool = SecurePool::open(dev, keys.pool, &keys.admin).map_err(storage_failure)?;
    for (id, credential) in keys.domains {
        pool.unlock(id, credential.borrowed())
            .map_err(storage_failure)?;
    }
    Ok(pool)
}
