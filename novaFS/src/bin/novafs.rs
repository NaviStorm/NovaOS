//! L'outil ne reçoit aucun chemin du Mac. Les octets d'une image exportée
//! transitent par stdout, les diagnostics par stderr.
use novafs::{Block, Device, Engine, Error, Ids, Mode, Result, codec::Anchor, device::FileImage};
use std::io::{self, BufRead, IsTerminal, Read, Write};
use std::os::{
    fd::AsFd,
    unix::fs::{FileTypeExt, MetadataExt},
};
#[path = "novafs/check.rs"]
mod check;
#[cfg(feature = "check-metrics")]
#[path = "../../lab/heap_meter.rs"]
mod heap_meter;
#[path = "novafs/secure.rs"]
mod secure;
#[path = "novafs/shared.rs"]
mod shared;

const HELP: &str = concat!(
    "NovaFS v",
    env!("CARGO_PKG_VERSION"),
    " — formats P0 et P5, pools P6 LZ4 et P7 chiffrés, images jetables, uniquement sur flux
novafs create BLOCS SECTEUR                        émet une image vierge sur stdout
novafs script BLOCS SECTEUR                        lit les commandes puis émet l’image sur stdout
novafs inspect BLOCS SECTEUR                       décrit les deux ancres de l’image sur stdin
novafs verify BLOCS SECTEUR FS_ID LV_ID            vérifie intégralement les deux générations
novafs check BLOCS SECTEUR FS_ID LV_ID             contrôle RO d’un LV, rapport JSON
novafs check BLOCS SECTEUR POOL_ID OBJET EMPREINTE   vérifie une branche P5/P6 et son reçu externe
novafs check-pool BLOCS SECTEUR                    vérifie l’intégrité du pool P0, P5 ou P6 (option --full)
novafs check-pool BLOCS SECTEUR [--cles-fd3]       inspecte P7 ou authentifie par pipe anonyme FD 3
novafs check BLOCS SECTEUR POOL_ID OBJET EMPREINTE [--cles-fd3]   contrôle P7 et son reçu avec clés
novafs space BLOCS SECTEUR FS_ID LV_ID             comptabilité du FS au repos, sur un LV hors ligne
novafs space-pool BLOCS SECTEUR                    comptabilité physique du catalogue hors ligne
novafs salvage BLOCS SECTEUR FS_ID LV_ID SLOT NOM extrait un fichier sur stdout en lecture seule
SECTEUR = 512 ou 4096. Identités = 32 chiffres hexadécimaux chacune.
Commandes de script (une par ligne, chemins NovaFS sans espaces) :
  mkdir /nom | create /nom | rename /source /dest | unlink /nom | collect
  write /nom OFFSET HEX | read /nom OFFSET LONGUEUR | inspect | verify
  policy BLOCS_MAINTENANCE QUOTA_OCTETS|none | space | cache [OCTETS]
Les résultats du script, les identités générées et les erreurs vont sur stderr."
);

fn num(s: &str) -> Result<u64> {
    s.parse()
        .map_err(|_| Error::Invalid("entier non signé attendu"))
}
fn hex(s: &str) -> Result<Vec<u8>> {
    if !s.len().is_multiple_of(2) || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(Error::Invalid("hexadécimal"));
    }
    s.as_bytes()
        .chunks_exact(2)
        .map(|c| {
            let nibble = |b: u8| {
                if b.is_ascii_digit() {
                    b - b'0'
                } else {
                    b.to_ascii_lowercase() - b'a' + 10
                }
            };
            Ok(nibble(c[0]) * 16 + nibble(c[1]))
        })
        .collect()
}
fn label(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn identifier(s: &str) -> Result<[u8; 16]> {
    if s.len() != 32 {
        return Err(Error::Invalid("identité de 16 octets"));
    }
    hex(s)?.try_into().map_err(|_| Error::Invalid("identité"))
}
fn new_id() -> Result<[u8; 16]> {
    let mut id = [0; 16];
    getrandom::fill(&mut id).map_err(|e| io::Error::other(e.to_string()))?;
    Ok(id)
}
fn describe(dev: &impl Device) -> Result<()> {
    let mut damaged = false;
    for slot in [0, 1] {
        match Anchor::decode(&dev.read(slot)?, slot) {
            Ok(a) => println!(
                "slot={slot} génération={} blocs={} FS={} LV={} manifeste={} sha256={}",
                a.manifest.birth,
                a.blocks,
                label(&a.ids.fs),
                label(&a.ids.lv),
                a.manifest.block,
                label(&a.manifest.hash)
            ),
            Err(e) => {
                eprintln!("slot={slot} invalide : {e}");
                damaged = true;
            }
        }
    }
    if damaged {
        Err(Error::Corrupt("inspection des ancres"))
    } else {
        Ok(())
    }
}
// Examiner le descripteur déjà ouvert sans suivre un chemin de périphérique.
// Les images transitent par un fichier ordinaire ou un tube, jamais par un terminal.
fn image_stream(fd: &impl AsFd) -> Result<()> {
    let handle = std::fs::File::from(fd.as_fd().try_clone_to_owned()?);
    let ty = handle.metadata()?.file_type();
    if ty.is_file() || ty.is_fifo() || ty.is_socket() {
        Ok(())
    } else {
        Err(Error::Invalid(
            "transport d’image : fichier ordinaire ou tube requis",
        ))
    }
}
fn load(dev: &mut impl Device, reader: &mut impl Read) -> Result<()> {
    let mut buf: Block = [0; 4096];
    for b in 0..dev.blocks() {
        reader.read_exact(&mut buf)?;
        dev.write(b, &buf)?;
    }
    if reader.read(&mut buf[..1])? != 0 {
        return Err(Error::Invalid("octets après l’image"));
    }
    dev.flush()
}
fn emit(dev: &impl Device, mut output: impl Write) -> Result<()> {
    for b in 0..dev.blocks() {
        output.write_all(&dev.read(b)?)?;
    }
    output.flush()?;
    Ok(())
}
fn commands(fs: &mut Engine<&mut FileImage>, mut input: impl BufRead) -> Result<()> {
    // Le payload maximal est 64 Kio, deux caractères ASCII par octet.
    const LINE: usize = 132_200;
    let mut buf = Vec::new();
    loop {
        buf.clear();
        if input
            .by_ref()
            .take((LINE + 1) as u64)
            .read_until(b'\n', &mut buf)?
            == 0
        {
            break;
        }
        if buf.len() > LINE {
            return Err(Error::Limit("ligne de commande"));
        }
        let line = std::str::from_utf8(&buf).map_err(|_| Error::Invalid("UTF-8"))?;
        let mut words = line.split_ascii_whitespace();
        let Some(cmd) = words.next() else { continue };
        let args: Vec<_> = words.by_ref().take(4).collect();
        if words.next().is_some() {
            return Err(Error::Invalid("arguments en trop"));
        }
        match (cmd, args.as_slice()) {
            ("mkdir", [p]) => fs.mkdir(p)?,
            ("create", [p]) => fs.create(p)?,
            ("unlink", [p]) => fs.unlink(p)?,
            ("rename", [a, b]) => fs.rename(a, b)?,
            ("collect", []) => while fs.collect()? {},
            ("write", [p, off, bytes]) => fs.write(p, num(off)?, &hex(bytes)?)?,
            ("read", [p, off, n]) => eprintln!(
                "{}",
                label(&fs.read(
                    p,
                    num(off)?,
                    usize::try_from(num(n)?).map_err(|_| Error::Limit("lecture"))?
                )?)
            ),
            ("inspect", []) => eprintln!("génération={} {:?}", fs.generation(), fs.space_stats()),
            ("space", []) => eprintln!("{}", check::volume_space(&fs.accounting()?)),
            ("cache", [bytes]) => {
                let limit =
                    usize::try_from(num(bytes)?).map_err(|_| Error::Limit("budget cache"))?;
                fs.set_cache_budget(limit)?;
            }
            ("cache", []) => {
                let s = fs.cache_stats();
                eprintln!(
                    "{}",
                    serde_json::json!({
                        "budget_octets": s.budget_bytes, "capacite_pages": s.capacity_pages,
                        "pages": s.pages, "contenu_octets": s.payload_bytes, "index_octets": s.metadata_bytes,
                        "debite_octets": s.charged_bytes, "fixe_octets": s.fixed_bytes,
                        "succes": s.hits, "absences": s.misses, "evictions": s.evictions,
                        "invalidations": s.invalidations, "pas_clock": s.clock_steps
                    })
                );
            }
            ("policy", [reserve, quota]) => {
                fs.set_volume_policy(novafs::VolumePolicy {
                    maintenance_blocks: num(reserve)?,
                    logical_quota_bytes: match *quota {
                        "none" => None,
                        limit => Some(num(limit)?),
                    },
                })?;
            }
            ("verify", []) => fs.verify()?,
            _ => return Err(Error::Invalid("commande du script")),
        }
    }
    Ok(())
}
fn run(args: &[String]) -> Result<()> {
    if args.is_empty() || args == ["--help"] {
        println!("{HELP}\n{}", shared::HELP);
        return Ok(());
    }
    if args.len() == 2 && args[1] == "--help" {
        let prefix = format!("novafs {} ", args[0]);
        let usage = HELP
            .lines()
            .chain(shared::HELP.lines())
            .find(|s| s.starts_with(&prefix))
            .ok_or(Error::Invalid("commande inconnue"))?;
        println!("{usage}");
        if shared::known(&args[0]) {
            println!("{}", shared::P7_HELP);
        }
        if args[0] == "script" {
            println!(
                "policy BLOCS_MAINTENANCE QUOTA_OCTETS|none    fixe la politique de cette session\nspace    écrit les compteurs d’espace sur stderr\ncache OCTETS    règle le budget pages et index (0 le désactive)\ncache    écrit les compteurs du cache sur stderr"
            );
        }
        return Ok(());
    }
    if shared::known(&args[0]) {
        return shared::run(args);
    }
    if args.len() < 3 {
        return Err(Error::Invalid("consulter --help"));
    }
    let blocks = num(&args[1])?;
    if blocks > 32768 {
        return Err(Error::Limit("outil limité à 128 Mio par image"));
    }
    let sector = u32::try_from(num(&args[2])?).map_err(|_| Error::Invalid("secteur"))?;
    let mut dev = FileImage::create(blocks, sector)?;
    match (args[0].as_str(), args.len()) {
        ("create" | "script", 3) => {
            image_stream(&io::stdout())?;
            if args[0] == "script" && !io::stdin().is_terminal() {
                image_stream(&io::stdin())?;
            }
            let ids = Ids {
                fs: new_id()?,
                lv: new_id()?,
            };
            eprintln!("FS={} LV={}", label(&ids.fs), label(&ids.lv));
            let mut engine = Engine::format(&mut dev, ids)?;
            if args[0] == "script" {
                commands(&mut engine, io::stdin().lock())?;
            }
            engine.verify()?;
            engine.unmount()?;
            emit(&dev, io::BufWriter::new(io::stdout().lock()))
        }
        ("inspect", 3) => {
            image_stream(&io::stdin())?;
            load(&mut dev, &mut io::stdin().lock())?;
            describe(&dev)
        }
        ("verify", 5) | ("salvage", 7) => {
            image_stream(&io::stdin())?;
            if args[0] == "salvage" {
                image_stream(&io::stdout())?;
            }
            let ids = Ids {
                fs: identifier(&args[3])?,
                lv: identifier(&args[4])?,
            };
            load(&mut dev, &mut io::stdin().lock())?;
            if args[0] == "verify" {
                let fs = Engine::mount(&mut dev, ids, Mode::ReadOnly)?;
                fs.verify()?;
                println!(
                    "Vérification intégrale réussie, génération {}",
                    fs.generation()
                );
            } else {
                let fs = Engine::salvage(&mut dev, ids, num(&args[5])?)?;
                eprintln!(
                    "Sauvetage en lecture seule, génération {}, fraîcheur indéterminée",
                    fs.generation()
                );
                let mut output = io::BufWriter::new(io::stdout().lock());
                if fs.stat(&args[6])?.size > 128 * 1024 * 1024 {
                    return Err(Error::Limit("extraction de plus de 128 Mio"));
                }
                let mut pos = 0;
                loop {
                    let bytes = fs.read(&args[6], pos, 65536)?;
                    if bytes.is_empty() {
                        break;
                    }
                    output.write_all(&bytes)?;
                    pos += bytes.len() as u64;
                }
                output.flush()?;
            }
            Ok(())
        }
        _ => Err(Error::Invalid("syntaxe ; consulter --help")),
    }
}
// Un descripteur standard peut avoir été redirigé avant le démarrage.
// /dev/null est un puits de texte autorisé ; aucun périphérique brut n'est ouvert.
fn text_stream(fd: &(impl AsFd + IsTerminal)) -> Result<()> {
    if fd.is_terminal() || image_stream(fd).is_ok() {
        return Ok(());
    }
    let meta = std::fs::File::from(fd.as_fd().try_clone_to_owned()?).metadata()?;
    if meta.file_type().is_char_device() && meta.rdev() == std::fs::metadata("/dev/null")?.rdev() {
        Ok(())
    } else {
        Err(Error::Invalid("sortie texte non autorisée"))
    }
}
fn main() -> std::process::ExitCode {
    use std::process::ExitCode;
    let args: Vec<String> = std::env::args().skip(1).collect();
    if text_stream(&io::stderr()).is_err() || text_stream(&io::stdout()).is_err() {
        return ExitCode::from(2);
    }
    let command = args.first().map(String::as_str);
    if command.is_some_and(shared::known)
        && !matches!(
            command,
            Some("check-export" | "list-views" | "compression-space")
        )
        && args.get(1).map(String::as_str) != Some("--help")
        && !check::separate_binary_diagnostics().unwrap_or(false)
    {
        return ExitCode::from(2);
    }
    if (matches!(
        command,
        Some("check" | "check-pool" | "space" | "space-pool" | "verify" | "inspect" | "salvage")
    ) || command.is_some_and(shared::known))
        && !check::separate_outputs().unwrap_or(false)
    {
        return ExitCode::from(2);
    }
    if matches!(
        command,
        Some("check" | "check-pool" | "space" | "space-pool" | "check-export")
    ) {
        if args.get(1).map(String::as_str) == Some("--help") && args.len() == 2 {
            println!("{}", check::help(&args[0]));
            return ExitCode::SUCCESS;
        }
        let status = check::run(&args);
        #[cfg(all(feature = "check-metrics", target_os = "macos"))]
        report_check_memory();
        return ExitCode::from(status);
    }
    if let Err(e) = run(&args) {
        eprintln!("NovaFS : {e}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

#[cfg(all(feature = "check-metrics", target_os = "macos"))]
fn report_check_memory() {
    use std::sync::atomic::Ordering;
    let stack = heap_meter::current_stack_size();
    let peak = heap_meter::PEAK.load(Ordering::Relaxed);
    eprintln!(
        "mesure pic_tas_octets={peak} pile_reservee_octets={stack} total_octets={}",
        peak + stack
    );
}
