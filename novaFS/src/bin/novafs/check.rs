//! Diagnostic hors ligne. Aucun chemin, montage écrivain, écriture de bloc ou flush.
use novafs::device::{FileImage, Geometry};
use novafs::pool::{Type, check_pool, inspect_usage};
use novafs::shared::{Receipt, SharedPool, looks_like_pool};
use novafs::{
    BLOCK_BYTES, Engine, Error, Ids, IntegrityReport, Mode, PairStatus, VolumeAccounting,
    check_image,
};
use serde_json::{Value, json};
use std::fs::{File, Metadata};
use std::io::{self, Write};
use std::os::{fd::AsFd, unix::fs::MetadataExt};

pub(super) fn help(command: &str) -> String {
    if command == "check-export" {
        return "novafs check-export < sauvegarde.nvx\nRapport JSON de l’export autonome vérifié. Alias : novafs check --export.".into();
    }
    if command == "check" {
        return "novafs check BLOCS SECTEUR FS_ID LV_ID < lv-p0.raw\nnovafs check BLOCS SECTEUR POOL_ID OBJET EMPREINTE [--full] < pool.raw\nnovafs check --export < sauvegarde.nvx\nRapport JSON hors ligne. Codes : 0 cohérent, 1 incohérent, 2 usage, 3 analyse incomplète.".into();
    }
    let arguments = if command == "space" {
        "BLOCS SECTEUR FS_ID LV_ID (P0) ou BLOCS SECTEUR POOL_ID OBJET (P5 ou P6)"
    } else if command == "check-pool" {
        "BLOCS SECTEUR [--full]"
    } else {
        "BLOCS SECTEUR"
    };
    format!(
        "novafs {command} {arguments} < image.raw\nImage hors ligne sur fichier ordinaire ; secteurs 512 ou 4096. Sortie JSON.\nCodes : 0 cohérent, 1 incohérent, 2 usage, 3 analyse incomplète."
    )
}

fn metadata(fd: &impl AsFd) -> io::Result<Metadata> {
    File::from(fd.as_fd().try_clone_to_owned()?).metadata()
}

pub(super) fn separate_outputs() -> io::Result<bool> {
    let input = metadata(&io::stdin())?;
    if input.is_file() {
        for output in [metadata(&io::stdout())?, metadata(&io::stderr())?] {
            if output.dev() == input.dev() && output.ino() == input.ino() {
                return Ok(false);
            }
        }
    }
    Ok(true)
}

pub(super) fn separate_binary_diagnostics() -> io::Result<bool> {
    let bytes = metadata(&io::stdout())?;
    let text = metadata(&io::stderr())?;
    Ok((bytes.dev(), bytes.ino()) != (text.dev(), text.ino()))
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Status {
    Coherent,
    Corrupt,
    Usage,
    Incomplete,
}

impl Status {
    fn name(self) -> &'static str {
        match self {
            Self::Coherent => "coherent",
            Self::Corrupt => "incoherent",
            Self::Usage => "usage",
            Self::Incomplete => "incomplet",
        }
    }
    fn code(self) -> u8 {
        match self {
            Self::Coherent => 0,
            Self::Corrupt => 1,
            Self::Usage => 2,
            Self::Incomplete => 3,
        }
    }
    fn include(&mut self, error: &Error) {
        let severity = match error {
            Error::Io(_) | Error::NoMemory | Error::Limit(_) | Error::Unavailable => {
                Self::Incomplete
            }
            _ => Self::Corrupt,
        };
        *self = (*self).max(severity);
    }
}

type Failure = (Status, String);

pub(super) fn volume_space(s: &VolumeAccounting) -> Value {
    json!({
        "domaine":"fs","unite_bloc_octets":BLOCK_BYTES,"capacite_blocs":s.capacity_blocks,
        "alloues_blocs":s.allocated_blocks,"vivants_blocs":s.live_blocks,"recuperables_blocs":s.recoverable_blocks,
        "reserves_maintenance_blocs":s.maintenance_reserved,"reserves_transaction_blocs":s.transaction_reserved,
        "disponibles_blocs":s.available_blocks,"logique_octets":s.logical_bytes.to_string(),
        "quota_logique_octets":s.logical_quota_bytes,"cible_maintenance_blocs":s.maintenance_target,
        "metadonnees_allocation_ram_octets":s.allocation_metadata_bytes
    })
}

fn storage_failure(e: Error) -> Failure {
    let mut status = Status::Coherent;
    status.include(&e);
    (status, e.to_string())
}

fn fs_report(report: IntegrityReport) -> (Status, Value) {
    let mut status = Status::Coherent;
    let mut slots = Vec::with_capacity(2);
    for (i, slot) in report.slots.iter().enumerate() {
        let issue = slot.issue.as_ref().map(|issue| {
            status.include(&issue.error);
            json!({"etape":issue.stage, "bloc":issue.block, "raison":issue.error.to_string()})
        });
        let anchor = slot.anchor.as_ref().map(|a| {
            json!({
                "generation":a.manifest.birth, "manifeste":a.manifest.block,
                "sha256_manifeste":super::label(&a.manifest.hash),
                "fs":super::label(&a.ids.fs), "lv":super::label(&a.ids.lv)
            })
        });
        slots.push(
            json!({"slot":i,"ancre":anchor,"vue_verifiee":issue.is_none(), "anomalie":issue}),
        );
    }
    let pair = match report.pair {
        PairStatus::Verified { slot } => json!({"etat":"verifiee","slot":slot,
            "generation":report.slots[slot].anchor.as_ref().map(|a| a.manifest.birth)}),
        PairStatus::Unavailable => json!({"etat":"indisponible","slot":null}),
        PairStatus::Rejected(issue) => {
            status.include(&issue.error);
            json!({"etat":"rejetee","slot":null,"etape":issue.stage,"bloc":issue.block,
                "raison":issue.error.to_string()})
        }
    };
    (
        status,
        json!({"domaine":"lv","catalogue_pool":"hors_image","slots":slots,"paire":pair}),
    )
}

fn diagnose_pool(dev: &FileImage) -> (Status, Value) {
    let report = check_pool(dev);
    let mut status = Status::Coherent;
    let mut slots = Vec::with_capacity(2);
    for (slot, result) in report.slots.iter().enumerate() {
        slots.push(match result {
            Ok(generation) => json!({"slot":slot,"generation":generation,"catalogue_verifie":true}),
            Err(e) => {
                status.include(e);
                json!({"slot":slot,"catalogue_verifie":false,"raison":e.to_string()})
            }
        });
    }
    let pair = match report.pair {
        Some(Ok(g)) => json!({"etat":"verifiee","generation":g}),
        Some(Err(e)) => {
            status.include(&e);
            json!({"etat":"rejetee","raison":e.to_string()})
        }
        None => json!({"etat":"indisponible"}),
    };
    let mut volumes = Vec::with_capacity(report.volumes.len());
    for v in report.volumes {
        let kind = match v.kind {
            Type::Ready => "pret",
            Type::Preparing => "preparation",
            Type::Raw => "raw",
        };
        let content = match v.check {
            Some(report) => {
                let (state, mut diagnostic) = fs_report(report);
                diagnostic["statut"] = json!(state.name());
                status = status.max(state);
                json!({"etat":if state == Status::Coherent {"verifie"} else {"echec"},
                    "diagnostic":diagnostic})
            }
            None => {
                json!({"etat":if v.kind==Type::Raw {"donnees_sans_checksum_fs"} else {"non_publie"}})
            }
        };
        volumes.push(json!({"lv":super::label(&v.id),"fs":super::label(&v.fs),
            "type":kind,"blocs":v.blocks,"contenu":content}));
    }
    (
        status,
        json!({"domaine":"pool","slots":slots,"paire":pair,"volumes":volumes}),
    )
}

fn fingerprint(m: &Metadata) -> (u64, i64, i64, i64, i64) {
    (
        m.len(),
        m.mtime(),
        m.mtime_nsec(),
        m.ctime(),
        m.ctime_nsec(),
    )
}

fn evaluate(args: &[String]) -> Result<(Status, Value), Failure> {
    let args = if matches!(args[0].as_str(), "check" | "check-pool")
        && args.last().is_some_and(|a| a == "--full")
    {
        &args[..args.len() - 1]
    } else {
        args
    };
    let usage = |e: Error| (Status::Usage, e.to_string());
    let interrupted = |e: io::Error| (Status::Incomplete, e.to_string());
    if args == ["check-export"] || args == ["check", "--export"] {
        super::image_stream(&io::stdin()).map_err(usage)?;
        let before = metadata(&io::stdin()).map_err(interrupted)?;
        let info =
            novafs::shared_export::check_export(io::stdin().lock()).map_err(storage_failure)?;
        if before.is_file()
            && fingerprint(&before) != fingerprint(&metadata(&io::stdin()).map_err(interrupted)?)
        {
            return Err((
                Status::Incomplete,
                "export modifié pendant le contrôle".into(),
            ));
        }
        return Ok((Status::Coherent, super::shared::archive_info(&info)));
    }
    if args[0] == "check-export" {
        return Err((Status::Usage, help("check-export")));
    }
    let is_lv = matches!(args[0].as_str(), "check" | "space");
    let p5_receipt = args[0] == "check" && args.len() == 6;
    if !p5_receipt && args.len() != if is_lv { 5 } else { 3 } {
        return Err((Status::Usage, help(&args[0])));
    }
    let blocks = super::num(&args[1]).map_err(usage)?;
    let sector = args[2]
        .parse::<u32>()
        .map_err(|_| (Status::Usage, "secteur invalide".into()))?;
    Geometry::new(blocks, sector).map_err(usage)?;
    let p5_space = args[0] == "space" && args[4].len() < 32 && super::num(&args[4]).is_ok();
    let ids = if is_lv && !p5_receipt && !p5_space {
        let ids = Ids {
            fs: super::identifier(&args[3]).map_err(usage)?,
            lv: super::identifier(&args[4]).map_err(usage)?,
        };
        if ids.fs == [0; 16] || ids.lv == [0; 16] {
            return Err((Status::Usage, "identité attendue nulle".into()));
        }
        Some(ids)
    } else {
        None
    };
    let fd = File::from(
        io::stdin()
            .as_fd()
            .try_clone_to_owned()
            .map_err(interrupted)?,
    );
    let before = fd.metadata().map_err(interrupted)?;
    if !before.is_file() {
        return Err((
            Status::Usage,
            "rediriger un fichier ordinaire hors ligne sur stdin".into(),
        ));
    }
    let expected = blocks * BLOCK_BYTES as u64; // Geometry borne blocks à 2^21.
    if before.len() != expected {
        return Err((
            Status::Corrupt,
            format!(
                "longueur de l'image : {} octets, {expected} attendus",
                before.len()
            ),
        ));
    }
    let dev = FileImage::from_file(fd.try_clone().map_err(interrupted)?, blocks, sector)
        .map_err(|e| (Status::Incomplete, e.to_string()))?;
    let shared = match looks_like_pool(&dev) {
        Ok(shared) => shared,
        Err(error) => {
            // Les contrôles P2 examinent les deux ancrages indépendamment, y compris
            // lorsqu'un descripteur illisible empêche la sélection du format.
            let mut report = match (args[0].as_str(), ids) {
                ("check-pool", _) => diagnose_pool(&dev).1,
                ("check", Some(ids)) => fs_report(check_image(&dev, ids)).1,
                _ => json!({}),
            };
            report["format_indetermine"] = json!(error.to_string());
            return Ok((Status::Incomplete, report));
        }
    };
    let result = match (args[0].as_str(), ids) {
        (_, _) if shared => {
            let pool = SharedPool::open(dev).map_err(storage_failure)?;
            let mut report = json!({"domaine":"pool_partage","format":if pool.is_p6() {"P6"} else {"P5"},"pool":super::label(&pool.pool_id()),"vues":pool.list().len()});
            if pool.is_p6() {
                report["verification"] =
                    json!("graphe, décompression intégrale et deux générations FS");
            }
            if matches!(args[0].as_str(), "space" | "space-pool") {
                let (occupied, deferred, reserved) = pool.pool_usage().map_err(storage_failure)?;
                report["capacite_blocs"] = json!(blocks);
                report["occupes_blocs"] = json!(occupied);
                report["differes_blocs"] = json!(deferred);
                report["credit_admission_blocs"] = json!(reserved);
                if args[0] == "space" {
                    if !p5_space {
                        return Err((Status::Usage, help("space")));
                    }
                    if super::identifier(&args[3]).map_err(usage)? != pool.pool_id() {
                        return Err((Status::Corrupt, "identité de pool différente".into()));
                    }
                    let id = super::num(&args[4]).map_err(usage)?;
                    let usage = pool.usage(id).map_err(storage_failure)?;
                    report["objet"] = json!(id);
                    report["logique_blocs"] = json!(usage.logical_blocks);
                    report["exclusifs_blocs"] = json!(usage.exclusive);
                    report["partages_blocs"] = json!(usage.shared);
                }
            } else if args[0] == "check" {
                if !p5_receipt || args[5].len() != 64 {
                    return Err((Status::Usage, help("check")));
                }
                let receipt = Receipt {
                    pool: super::identifier(&args[3]).map_err(usage)?,
                    id: super::num(&args[4]).map_err(usage)?,
                    root: super::hex(&args[5])
                        .map_err(usage)?
                        .try_into()
                        .map_err(|_| (Status::Usage, help("check")))?,
                };
                pool.verify_receipt(&receipt).map_err(storage_failure)?;
                report["objet"] = json!(receipt.id);
                report["empreinte"] = json!(super::label(&receipt.root));
            }
            (Status::Coherent, report)
        }
        ("check", _) if p5_receipt => {
            return Err((Status::Corrupt, "format P5 attendu pour le reçu".into()));
        }
        ("space", Some(ids)) => {
            let fs = Engine::mount(dev, ids, Mode::ReadOnly).map_err(storage_failure)?;
            let stats = fs.accounting().map_err(storage_failure)?;
            (Status::Coherent, volume_space(&stats))
        }
        ("space-pool", None) => {
            let (s, entries) = inspect_usage(&dev).map_err(storage_failure)?;
            let volumes: Vec<_> = entries
                .iter()
                .map(|(e, usage)| {
                    let kind = match e.kind {
                        Type::Ready => "pret",
                        Type::Preparing => "preparation",
                        Type::Raw => "raw",
                    };
                    json!({"lv":super::label(&e.id),"type":kind,"domaine":super::label(&e.owner),
                    "garantis_blocs":e.guaranteed_blocks,"fs":usage.as_ref().map(volume_space)})
                })
                .collect();
            (
                Status::Coherent,
                json!({"domaine":"catalogue_pool","unite_bloc_octets":BLOCK_BYTES,
                "capacite_blocs":s.total_blocks,"grain_entete_blocs":s.metadata_blocks,"queue_blocs":s.tail_blocks,
                "attribues_blocs":s.assigned_blocks,"differes_blocs":s.deferred_blocks,
                "reserves_transaction_blocs":s.reserved_blocks,"disponibles_blocs":s.available_blocks,
                "partages_entre_lv_blocs":s.shared_blocks,"tickets":s.pending_volumes,"volumes":volumes}),
            )
        }
        ("check", Some(ids)) => fs_report(check_image(&dev, ids)),
        ("check-pool", None) => diagnose_pool(&dev),
        _ => return Err((Status::Usage, help(&args[0]))),
    };
    if fingerprint(&before) != fingerprint(&fd.metadata().map_err(interrupted)?) {
        return Err((
            Status::Incomplete,
            "image modifiée pendant le contrôle".into(),
        ));
    }
    Ok(result)
}

pub(super) fn run(args: &[String]) -> u8 {
    let (status, mut result) = evaluate(args).unwrap_or_else(|(s, e)| (s, json!({"raison":e})));
    result["schema"] = json!(
        if args == ["check-export"] || args == ["check", "--export"] {
            "novafs-export-1"
        } else if matches!(args[0].as_str(), "space" | "space-pool") {
            "novafs-space-1"
        } else {
            "novafs-check-1"
        }
    );
    result["statut"] = json!(status.name());
    let mut output = io::stdout().lock();
    if serde_json::to_writer(&mut output, &result).is_err()
        || output
            .write_all(b"\n")
            .and_then(|()| output.flush())
            .is_err()
    {
        return Status::Incomplete.code();
    }
    status.code()
}
