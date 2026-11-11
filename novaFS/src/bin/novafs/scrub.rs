//! Interface sur fichier ordinaire hors ligne. La réparation produit un autre
//! flux image, après vérification et remontage de la copie privée.
use super::*;
use novafs::device::Geometry;
use novafs::secure_pool::scrub::{Authority, Mode, Report, Status, scrub};
use serde_json::{Value, json};

const USAGE: &str = concat!(
    "novafs scrub BLOCS SECTEUR [--full | --repair] [--cles-fd3] [--json] < image.raw\n",
    "Diagnostic hors ligne. --full demande toutes les clés nécessaires sur le pipe anonyme FD 3.\n",
    "--repair valide et répare une copie privée. Rediriger stdout vers une nouvelle image, rapport sur stderr.\n",
    "Codes 0 sain, 1 corruption, 2 usage, 3 incomplet, 4 réparé, 5 non réparable, 6 E/S, 7 clés indisponibles.",
);

fn json_report(r: &Report) -> Value {
    let issues: Vec<_> = r
        .issues
        .iter()
        .map(|i| {
            json!({
                "structure": i.stage,
                "generation": i.generation,
                "objet": i.view,
                "volume": i.volume.map(|id| label(&id)),
                "plage_blocs": i.blocks,
                "raison": i.reason,
                "corruption": i.corruption,
            })
        })
        .collect();
    json!({
        "schema": "novafs-scrub-1",
        "execution": label(&r.run),
        "statut": r.status.label(),
        "format": r.format,
        "generation": r.generation,
        "authenticite_verifiee": r.authenticated,
        "objets_examines": r.views,
        "lectures_blocs": r.reads,
        "blocs_distincts_examines": r.distinct_blocks,
        "vues_chiffrees_non_verifiees": r.locked_views,
        "ecritures_blocs": r.writes,
        "synchronisations": r.flushes,
        "duree_ns": r.elapsed_ns,
        "anomalies": issues,
        "nombre_anomalies": r.issues.len(),
        "constats_corruption": r.issues.iter().filter(|i| i.corruption).count(),
        "reparations_possibles": r.possible,
        "reparations_effectuees": r.repaired,
        "defauts_non_reparables": r.irreparable,
    })
}

fn execute(args: &[String]) -> Result<u8> {
    let [blocks, sector, flags @ ..] = args else {
        return Err(Error::Invalid(USAGE));
    };
    let blocks = num(blocks)?;
    let sector = sector.parse().map_err(|_| Error::Invalid(USAGE))?;
    Geometry::new(blocks, sector)?;
    let mut mode = Mode::Diagnostic;
    let (mut with_keys, mut json) = (false, false);
    for flag in flags {
        match flag.as_str() {
            "--full" if mode == Mode::Diagnostic => mode = Mode::Full,
            "--repair" if mode == Mode::Diagnostic => mode = Mode::Repair,
            "--json" if !json => json = true,
            "--cles-fd3" if !with_keys => with_keys = true,
            _ => return Err(Error::Invalid(USAGE)),
        }
    }
    if mode == Mode::Repair {
        image_stream(&io::stdout())?;
    }
    let fd = std::fs::File::from(io::stdin().as_fd().try_clone_to_owned()?);
    let metadata = fd.metadata()?;
    if !metadata.is_file() || metadata.len() != blocks * 4096 {
        return Err(Error::Invalid("fichier hors ligne de taille exacte requis"));
    }
    let before = check::fingerprint(&metadata);
    let mut image = if mode == Mode::Repair {
        let mut copy = FileImage::create(blocks, sector)?;
        load(&mut copy, &mut &fd)?;
        copy
    } else {
        FileImage::from_file(fd.try_clone()?, blocks, sector)?
    };
    let frame = with_keys.then(check::key_pipe::read).transpose()?;
    let domains: Vec<_> = frame
        .iter()
        .flat_map(|f| f.domains.iter())
        .map(|(id, c)| (*id, c.borrowed()))
        .collect();
    let auth = frame.as_ref().map(|f| Authority {
        pool: f.pool,
        admin: &f.admin,
        domains: &domains,
    });
    let mut r = scrub(&mut image, mode, auth.as_ref(), |_| true)?;
    if check::fingerprint(&fd.metadata()?) != before {
        r.status = Status::Incomplete;
        r.authenticated = false;
        r.repaired = 0;
        r.possible = 0;
        r.issues.push(novafs::secure_pool::scrub::Issue {
            stage: "entrée",
            corruption: false,
            generation: None,
            view: None,
            volume: None,
            blocks: None,
            reason: "image externe modifiée pendant le contrôle",
        });
    }
    if mode == Mode::Repair && r.status == Status::Repaired {
        emit(&image, io::stdout().lock())?
    }
    let mut report: Box<dyn Write> = if mode == Mode::Repair {
        Box::new(io::stderr().lock())
    } else {
        Box::new(io::stdout().lock())
    };
    if json {
        serde_json::to_writer(&mut report, &json_report(&r)).map_err(io::Error::other)?;
        writeln!(report)?;
    } else {
        writeln!(
            report,
            "{} · {} · exécution {} · génération {:?}",
            r.format,
            r.status.label(),
            label(&r.run),
            r.generation
        )?;
        writeln!(
            report,
            "{} observations de vues · {} lectures sur {} blocs distincts · {} réparations · {:.3} ms",
            r.views,
            r.reads,
            r.distinct_blocks,
            r.repaired,
            r.elapsed_ns as f64 / 1_000_000.0
        )?;
        if matches!(r.format, "P7" | "P9") && !r.authenticated {
            writeln!(
                report,
                "Authenticité complète du pool {} non établie.",
                r.format
            )?;
        }
        for (generation, view) in &r.locked_views {
            writeln!(
                report,
                "Vue {view}, génération {generation} : clé absente, tags et contenu chiffré non vérifiés."
            )?;
        }
        for issue in &r.issues {
            writeln!(
                report,
                "{} génération={:?} objet={:?} blocs={:?} {}",
                issue.stage, issue.generation, issue.view, issue.blocks, issue.reason
            )?;
        }
    }
    report.flush()?;
    Ok(r.status as u8)
}
pub(super) fn run(args: &[String]) -> u8 {
    let binary = args.iter().any(|a| a == "--repair");
    if !check::separate_outputs().unwrap_or(false)
        || (binary && !check::separate_binary_diagnostics().unwrap_or(false))
    {
        return 2;
    }
    if args == ["--help"] {
        println!("{USAGE}");
        return 0;
    }
    match execute(args) {
        Ok(code) => code,
        Err(e) => {
            let (code, status) = match e {
                Error::Io(_) => (6, "erreur_io"),
                Error::Busy | Error::NoMemory => (3, "incomplet"),
                Error::ReadOnly | Error::Unavailable => (7, "authentification_indisponible"),
                _ => (2, "usage"),
            };
            let emitted = if args.iter().any(|a| a == "--json") {
                let error =
                    json!({"schema":"novafs-scrub-1", "statut":status, "erreur":e.to_string()});
                if binary {
                    writeln!(io::stderr().lock(), "{error}")
                } else {
                    writeln!(io::stdout().lock(), "{error}")
                }
            } else {
                writeln!(io::stderr().lock(), "NovaFS scrub : {e}")
            };
            if emitted.is_err() {
                return Status::Io as u8;
            }
            code
        }
    }
}
