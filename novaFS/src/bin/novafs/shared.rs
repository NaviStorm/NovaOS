//! Outils d’images de pools P5 et P6 sur flux. Les transformations publient une nouvelle image
//! sur stdout après validation. Le descripteur source reste ouvert en lecture seule.
use super::{emit, hex, identifier, image_stream, label, load, num};
use novafs::device::FileImage;
use novafs::shared::{ObjectKind, SharedPool};
use novafs::shared_export::{ExportInfo, export_view, restore_view, restore_with_compression};
use novafs::{Compression, Device, Engine, Error, Ids, Mode, Result};
use serde_json::{Value, json};
use std::io::{self, Write};

pub(super) const HELP: &str = "novafs pool-format BLOCS SECTEUR POOL_ID [p5|p6]     crée un pool vide (P5 par défaut)
novafs volume-create BLOCS SECTEUR DOMAINE FS_ID LV_ID BLOCS_LV [none|lz4]    ajoute une origine au pool lu sur stdin
novafs set-compression BLOCS SECTEUR OBJET none|lz4    définit la politique des prochaines écritures
novafs compression-space BLOCS SECTEUR OBJET         affiche les octets logiques et l’usage physique en JSON
novafs file-create BLOCS SECTEUR OBJET CHEMIN       crée un fichier dans une branche inscriptible
novafs file-write BLOCS SECTEUR OBJET CHEMIN OFFSET HEX   modifie le fichier dans une copie du pool
novafs file-read BLOCS SECTEUR OBJET CHEMIN OFFSET OCTETS    extrait les octets du fichier sur stdout
novafs snapshot BLOCS SECTEUR OBJET                 crée un snapshot du volume
novafs clone BLOCS SECTEUR SNAPSHOT DOMAINE          crée un clone inscriptible
novafs drop-view BLOCS SECTEUR OBJET                retire une vue et publie son nouveau catalogue
novafs advance BLOCS SECTEUR                        effectue une rotation du catalogue de pool
novafs list-views BLOCS SECTEUR                     énumère les vues et leurs reçus en JSON
novafs export BLOCS SECTEUR OBJET                   émet un export complet de snapshot ou clone
novafs restore BLOCS_DEST SECTEUR_DEST POOL_DEST DOMAINE [none|lz4]   restaure dans un pool neuf (politique explicite pour P6)
novafs migrate-pool BLOCS_DEST SECTEUR_DEST POOL_DEST BLOCS_P0 SECTEUR_P0   migre un pool P0 complet hors ligne
novafs migrate-view BLOCS_DEST SECTEUR_DEST POOL_DEST BLOCS_SOURCE SECTEUR_SOURCE OBJET none|lz4   importe une vue P5/P6 dans un nouveau pool P6
novafs migrate-lv BLOCS_DEST SECTEUR_DEST POOL_DEST BLOCS_P0 SECTEUR_P0 DOMAINE FS_ID LV_ID   importe un LV P0 autonome
novafs check-export                                vérifie un export complet sur stdin, JSON sur stdout";

pub(super) fn known(cmd: &str) -> bool {
    HELP.lines()
        .any(|l| l.split_ascii_whitespace().nth(1) == Some(cmd))
}
fn compression(name: &str) -> Result<Compression> {
    match name {
        "none" => Ok(Compression::None),
        "lz4" => Ok(Compression::Lz4),
        _ => Err(Error::Invalid("politique de compression inconnue")),
    }
}
fn volume(id: u64, pool: &SharedPool<impl Device>) -> Result<Value> {
    let obj = pool.info(id)?;
    let receipt = pool.receipt(id)?;
    Ok(
        json!({"id":id, "pool":label(&receipt.pool), "empreinte":label(&receipt.root),
        "type":kind(obj.kind), "compression": match obj.policy { Compression::None=>"none", Compression::Lz4=>"lz4" }, "domaine":label(&obj.owner), "lignee_fs":label(&obj.ids.fs),
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
    if !matches!(cmd, "list-views" | "compression-space") {
        image_stream(&io::stdout())?;
    }
    let mut pool = match (cmd, tail) {
        ("pool-format", [pool, "p6"]) => SharedPool::format_p6(scratch, identifier(pool)?)?,
        ("pool-format", [pool] | [pool, "p5"]) => SharedPool::format(scratch, identifier(pool)?)?,
        ("restore", [pool, owner]) => {
            let mut pool = SharedPool::format(scratch, identifier(pool)?)?;
            let id = restore_view(&mut pool, io::stdin().lock(), identifier(owner)?)?;
            eprintln!("{}", volume(id, &pool)?);
            pool
        }
        ("restore", [pool, owner, name]) => {
            let policy = compression(name)?;
            let mut pool = SharedPool::format_p6(scratch, identifier(pool)?)?;
            let id = restore_with_compression(
                &mut pool,
                io::stdin().lock(),
                identifier(owner)?,
                policy,
            )?;
            eprintln!("{}", volume(id, &pool)?);
            pool
        }
        (
            "migrate-pool" | "migrate-lv" | "migrate-view",
            [pool, old_blocks, old_sector, other @ ..],
        ) => {
            let size = num(old_blocks)?;
            if size > 32768 {
                return Err(Error::Limit("outil limité à 128 Mio par image"));
            }
            let sector =
                u32::try_from(num(old_sector)?).map_err(|_| Error::Invalid("secteur source"))?;
            let mut old = FileImage::create(size, sector)?;
            load(&mut old, &mut io::stdin().lock())?;
            let mut target = if cmd == "migrate-view" {
                SharedPool::format_p6(scratch, identifier(pool)?)?
            } else {
                SharedPool::format(scratch, identifier(pool)?)?
            };
            let ids = match (cmd, other) {
                ("migrate-view", [id, name]) => {
                    let source = SharedPool::open(old)?;
                    let view = source.info(num(id)?)?;
                    vec![target.import_with_compression(
                        view.owner,
                        view.ids,
                        &source.read_device(view.id)?,
                        view.kind,
                        compression(name)?,
                    )?]
                }
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
        ("restore" | "pool-format" | "migrate-pool" | "migrate-lv" | "migrate-view", _) => {
            return Err(Error::Invalid("syntaxe P5"));
        }
        _ => {
            load(&mut scratch, &mut io::stdin().lock())?;
            SharedPool::open(scratch)?
        }
    };
    let created = match (cmd, tail) {
        ("pool-format" | "restore" | "migrate-pool" | "migrate-lv" | "migrate-view", _) => None,
        ("volume-create", [owner, fs, lv, size, opts @ ..]) => Some(pool.create_with_compression(
            identifier(owner)?,
            Ids {
                fs: identifier(fs)?,
                lv: identifier(lv)?,
            },
            num(size)?,
            match opts {
                [] => Compression::None,
                [p] => compression(p)?,
                _ => return Err(Error::Invalid("arguments volume-create")),
            },
        )?),
        ("set-compression", [id, name]) => {
            let id = num(id)?;
            pool.set_compression(id, compression(name)?)?;
            Some(id)
        }
        ("compression-space", [id]) => {
            let id = num(id)?;
            let info = pool.info(id)?;
            let usage = pool.usage(id)?;
            let packs = pool.compression_usage(id)?;
            let fs = Engine::mount(pool.read_device(id)?, info.ids, Mode::ReadOnly)?;
            let space = fs.accounting()?;
            return output_json(json!({
                "objet": id, "taille_logique_octets": space.logical_bytes,
                "quota_logique_octets": space.logical_quota_bytes, "unites_compressees": packs.compressed_units,
                "pages_compressees_mappees": packs.mapped_compressed_pages, "flux_lz4_octets": packs.encoded_bytes,
                "unites_physiques_octets": packs.physical_bytes, "economie_octets": packs.saved_bytes,
                "vue_exclusifs_blocs": usage.exclusive, "vue_partages_blocs": usage.shared,
                "pool_occupes_blocs": usage.occupied, "pool_credit_blocs": usage.reserved
            }));
        }
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
