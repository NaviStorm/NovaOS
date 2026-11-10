//! Outils d’images P5 sur flux. Les transformations publient une nouvelle image
//! sur stdout après validation. Le descripteur source reste ouvert en lecture seule.
use super::{emit, hex, identifier, image_stream, label, load, num};
use novafs::device::FileImage;
use novafs::shared::{ObjectKind, SharedPool};
use novafs::shared_export::{ExportInfo, export_view, restore_view};
use novafs::{Device, Error, Ids, Result};
use serde_json::{Value, json};
use std::io::{self, Write};

pub(super) const HELP: &str = "novafs pool-format BLOCS SECTEUR POOL_ID             crée un pool P5 vide sur stdout
novafs volume-create BLOCS SECTEUR DOMAINE FS_ID LV_ID BLOCS_LV    ajoute une origine au pool lu sur stdin
novafs file-create BLOCS SECTEUR OBJET CHEMIN       crée un fichier dans une branche inscriptible
novafs file-write BLOCS SECTEUR OBJET CHEMIN OFFSET HEX   modifie le fichier dans une copie du pool
novafs file-read BLOCS SECTEUR OBJET CHEMIN OFFSET OCTETS    extrait les octets du fichier sur stdout
novafs snapshot BLOCS SECTEUR OBJET                 crée un snapshot du volume
novafs clone BLOCS SECTEUR SNAPSHOT DOMAINE          crée un clone inscriptible
novafs drop-view BLOCS SECTEUR OBJET                retire une vue et publie son nouveau catalogue
novafs advance BLOCS SECTEUR                        effectue une rotation du catalogue de pool
novafs list-views BLOCS SECTEUR                     énumère les vues et leurs reçus en JSON
novafs export BLOCS SECTEUR OBJET                   émet un export complet de snapshot ou clone
novafs restore BLOCS_DEST SECTEUR_DEST POOL_DEST DOMAINE   restaure l’export dans un nouveau pool
novafs migrate-pool BLOCS_DEST SECTEUR_DEST POOL_DEST BLOCS_P0 SECTEUR_P0   migre un pool P0 complet hors ligne
novafs migrate-lv BLOCS_DEST SECTEUR_DEST POOL_DEST BLOCS_P0 SECTEUR_P0 DOMAINE FS_ID LV_ID   importe un LV P0 autonome
novafs check-export                                vérifie un export complet sur stdin, JSON sur stdout";

pub(super) fn known(cmd: &str) -> bool {
    HELP.lines()
        .any(|l| l.split_ascii_whitespace().nth(1) == Some(cmd))
}
fn volume(id: u64, pool: &SharedPool<impl Device>) -> Result<Value> {
    let obj = pool.info(id)?;
    let receipt = pool.receipt(id)?;
    Ok(
        json!({"id":id, "pool":label(&receipt.pool), "empreinte":label(&receipt.root),
        "type":kind(obj.kind), "domaine":label(&obj.owner), "lignee_fs":label(&obj.ids.fs),
        "lignee_lv":label(&obj.ids.lv), "blocs_logiques":obj.blocks}),
    )
}
fn kind(k: ObjectKind) -> &'static str {
    match k {
        ObjectKind::Origin => "origine",
        ObjectKind::Clone => "clone",
        ObjectKind::Snapshot => "snapshot",
    }
}
pub(super) fn archive_info(info: &ExportInfo) -> Value {
    json!({"schema":"novafs-export-1","statut":"coherent","pool_source":label(&info.pool),
        "objet_source":info.id,"empreinte_source":label(&info.source_root),"type":kind(info.kind),
        "domaine_source":label(&info.owner),"lignee_fs":label(&info.ids.fs),"lignee_lv":label(&info.ids.lv),
        "blocs_logiques":info.blocks,"secteur_source":info.sector,"pages_emises":info.pages})
}
fn output_json(info: Value) -> Result<()> {
    let mut out = io::stdout().lock();
    writeln!(out, "{info}")?;
    out.flush()?;
    Ok(())
}

pub(super) fn run(args: &[String]) -> Result<()> {
    let cmd = args[0].as_str();
    if args.len() < 3 {
        return Err(Error::Invalid("arguments P5, consulter --help"));
    }
    let blocks = num(&args[1])?;
    if blocks > 32768 {
        return Err(Error::Limit("outil limité à 128 Mio par image"));
    }
    let sector = u32::try_from(num(&args[2])?).map_err(|_| Error::Invalid("secteur"))?;
    let mut scratch = FileImage::create(blocks, sector)?;
    let tail: Vec<_> = args[3..].iter().map(|s| s.as_str()).collect();
    let tail = tail.as_slice();
    if cmd != "pool-format" {
        image_stream(&io::stdin())?;
    }
    if cmd != "list-views" {
        image_stream(&io::stdout())?;
    }
    let mut pool = match (cmd, tail) {
        ("pool-format", [pool]) => SharedPool::format(scratch, identifier(pool)?)?,
        ("restore", [pool, owner]) => {
            let mut pool = SharedPool::format(scratch, identifier(pool)?)?;
            let id = restore_view(&mut pool, io::stdin().lock(), identifier(owner)?)?;
            eprintln!("{}", volume(id, &pool)?);
            pool
        }
        ("migrate-pool" | "migrate-lv", [pool, old_blocks, old_sector, other @ ..]) => {
            let size = num(old_blocks)?;
            if size > 32768 {
                return Err(Error::Limit("outil limité à 128 Mio par image"));
            }
            let sector =
                u32::try_from(num(old_sector)?).map_err(|_| Error::Invalid("secteur source"))?;
            let mut old = FileImage::create(size, sector)?;
            load(&mut old, &mut io::stdin().lock())?;
            let mut target = SharedPool::format(scratch, identifier(pool)?)?;
            let ids = match (cmd, other) {
                ("migrate-pool", []) => target.migrate_legacy(&old)?,
                ("migrate-lv", [owner, fs, lv]) => vec![target.import_image(
                    identifier(owner)?,
                    Ids {
                        fs: identifier(fs)?,
                        lv: identifier(lv)?,
                    },
                    &old,
                    ObjectKind::Origin,
                )?],
                _ => return Err(Error::Invalid("arguments de migration P0")),
            };
            for id in ids {
                eprintln!("{}", volume(id, &target)?);
            }
            target
        }
        ("restore" | "pool-format" | "migrate-pool" | "migrate-lv", _) => {
            return Err(Error::Invalid("syntaxe P5"));
        }
        _ => {
            load(&mut scratch, &mut io::stdin().lock())?;
            SharedPool::open(scratch)?
        }
    };
    let created = match (cmd, tail) {
        ("pool-format" | "restore" | "migrate-pool" | "migrate-lv", _) => None,
        ("volume-create", [owner, fs, lv, size]) => Some(pool.create(
            identifier(owner)?,
            Ids {
                fs: identifier(fs)?,
                lv: identifier(lv)?,
            },
            num(size)?,
        )?),
        ("snapshot", [id]) => Some(pool.snapshot(num(id)?)?),
        ("clone", [id, owner]) => Some(pool.clone_snapshot(num(id)?, identifier(owner)?)?),
        ("drop-view", [id]) => {
            pool.remove(num(id)?)?;
            None
        }
        ("advance", []) => {
            pool.advance()?;
            None
        }
        ("file-create", [id, path]) => {
            pool.create_file(num(id)?, path)?;
            Some(num(id)?)
        }
        ("file-write", [id, path, offset, data]) => {
            if data.len() > 131072 {
                return Err(Error::Limit("écriture limitée à 64 Kio"));
            }
            pool.write_file(num(id)?, path, num(offset)?, &hex(data)?)?;
            Some(num(id)?)
        }
        ("file-read", [id, path, offset, count]) => {
            let n: usize = num(count)?
                .try_into()
                .map_err(|_| Error::Limit("lecture"))?;
            if n > 65536 {
                return Err(Error::Limit("lecture limitée à 64 Kio"));
            }
            let bytes = pool.read_file(num(id)?, path, num(offset)?, n)?;
            let mut out = io::stdout().lock();
            out.write_all(&bytes)?;
            out.flush()?;
            return Ok(());
        }
        ("list-views", []) => {
            let views: Result<Vec<_>> = pool.list().iter().map(|o| volume(o.id, &pool)).collect();
            return output_json(json!({"pool":label(&pool.pool_id()),"vues":views?}));
        }
        ("export", [id]) => {
            let info = export_view(
                &pool,
                num(id)?,
                sector,
                io::BufWriter::new(io::stdout().lock()),
            )?;
            eprintln!("{}", archive_info(&info));
            return Ok(());
        }
        _ => return Err(Error::Invalid("syntaxe P5, consulter --help")),
    };
    if let Some(id) = created {
        eprintln!("{}", volume(id, &pool)?);
    }
    emit(&pool.into_device(), io::BufWriter::new(io::stdout().lock()))
}
