//! Diagnostic hors ligne. Aucun chemin, montage écrivain, écriture de bloc ou flush.
use novafs::device::{FileImage, Geometry};
use novafs::pool::{Type, check_pool};
use novafs::{BLOCK_BYTES, Error, Ids, IntegrityReport, PairStatus, check_image};
use serde_json::{Value, json};
use std::fs::{File, Metadata};
use std::io::{self, Write};
use std::os::{fd::AsFd, unix::fs::MetadataExt};

pub(super) fn help(command: &str) -> String {
    let usage = if command == "check" {
        "check BLOCS SECTEUR FS_ID LV_ID"
    } else {
        "check-pool BLOCS SECTEUR"
    };
    format!(
        "novafs {usage} < image.raw\nImage hors ligne sur fichier ordinaire ; secteurs 512 ou 4096. Sortie JSON.\nCodes : 0 cohérent, 1 incohérent, 2 usage, 3 analyse incomplète."
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
    let usage = |e: Error| (Status::Usage, e.to_string());
    let interrupted = |e: io::Error| (Status::Incomplete, e.to_string());
    let is_lv = args[0] == "check";
    if args.len() != if is_lv { 5 } else { 3 } {
        return Err((Status::Usage, help(&args[0])));
    }
    let blocks = super::num(&args[1]).map_err(usage)?;
    let sector = args[2]
        .parse::<u32>()
        .map_err(|_| (Status::Usage, "secteur invalide".into()))?;
    Geometry::new(blocks, sector).map_err(usage)?;
    let ids = if is_lv {
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
    let result = match ids {
        Some(ids) => fs_report(check_image(&dev, ids)),
        None => diagnose_pool(&dev),
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
    result["schema"] = json!("novafs-check-1");
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
