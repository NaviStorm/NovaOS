//! Transformations explicites d'images P7. Les credentials entrent par pipes
//! hérités. Le fichier de travail reçoit uniquement la représentation disque.
use crate::check::key_pipe;
use crate::{emit, identifier, image_stream, label, load, num};
use novafs::crypto::Secret;
use novafs::device::FileImage;
use novafs::secure_pool::SecurePool;
use novafs::{Error, Ids, Result};
use std::io::{self, Read, Write};
use zeroize::Zeroizing;

pub(super) fn end(input: &mut impl Read) -> Result<()> {
    let mut extra = Zeroizing::new([0; 1]);
    if input.read(&mut extra[..])? != 0 {
        return Err(Error::Invalid("octets en excès sur FD4"));
    }
    Ok(())
}
pub(super) fn key(input: &mut impl Read) -> Result<Secret> {
    let mut bytes = Zeroizing::new([0; 32]);
    input.read_exact(&mut bytes[..])?;
    Ok(Secret::from_bytes(*bytes))
}
fn read_enrollment(input: &mut impl Read) -> Result<(Zeroizing<Vec<u8>>, Secret)> {
    let mut len = [0; 2];
    input.read_exact(&mut len)?;
    let n = usize::from(u16::from_le_bytes(len));
    if !(1..=1024).contains(&n) {
        return Err(Error::Invalid("longueur de phrase P7"));
    }
    let mut phrase = Zeroizing::new(vec![0; n]);
    input.read_exact(&mut phrase)?;
    let recovery = key(input)?;
    Ok((phrase, recovery))
}
fn enrollment(input: &mut impl Read) -> Result<(Zeroizing<Vec<u8>>, Secret)> {
    let entry = read_enrollment(input)?;
    end(input)?;
    Ok(entry)
}
pub(super) fn backup() -> Result<Secret> {
    let mut input = key_pipe::pipe(4)?;
    let result = key(&mut input)?;
    end(&mut input)?;
    Ok(result)
}
pub(super) fn run(cmd: &str, args: &[&str], mut image: FileImage, sector: u32) -> Result<()> {
    let frame = key_pipe::read()?;
    let mut pool = if matches!(cmd, "pool-format" | "restore") {
        let expected = args
            .first()
            .ok_or(Error::Invalid("identité de pool attendue"))?;
        if identifier(expected)? != frame.pool || !frame.domains.is_empty() {
            return Err(Error::Invalid("provisionnement du pool P7"));
        }
        if cmd == "pool-format" && args.get(1) == Some(&"p9") {
            SecurePool::format_p9(image, frame.pool, &frame.admin, novafs::crypto::HostEntropy)?
        } else {
            SecurePool::format(image, frame.pool, &frame.admin)?
        }
    } else {
        load(&mut image, &mut io::stdin().lock())?;
        let mut pool = SecurePool::open(image, frame.pool, &frame.admin)?;
        for (id, cred) in &frame.domains {
            pool.unlock(*id, cred.borrowed())?;
        }
        pool
    };
    let id = match (cmd, args) {
        ("pool-format", [_, "p7" | "p9"]) => None,
        ("migrate-p9", [blocks]) => {
            use novafs::secure_pool::migration::Enrollment;
            let target = num(blocks)?;
            if target > 32768 {
                return Err(Error::Limit("taille de destination"));
            }
            let mut pipe = key_pipe::pipe(4)?;
            let mut tag = [0; 8];
            pipe.read_exact(&mut tag)?;
            if &tag != b"NP9ENRL1" {
                return Err(Error::Invalid("trame d’enrôlement P9"));
            }
            let destination_key = key(&mut pipe)?;
            let mut count = [0];
            pipe.read_exact(&mut count)?;
            if count[0] > 17 {
                return Err(Error::Invalid("nombre de lignées"));
            }
            let mut records = Vec::new();
            for _ in 0..count[0] {
                let mut id = [0; 16];
                pipe.read_exact(&mut id)?;
                let (phrase, recovery) = read_enrollment(&mut pipe)?;
                records.push((id, phrase, recovery));
            }
            end(&mut pipe)?;
            let enroll = records
                .iter()
                .map(|(id, p, r)| Enrollment {
                    old_domain: *id,
                    phrase: p,
                    recovery: r,
                })
                .collect::<Vec<_>>();
            let (image, receipt) = pool.migrate_p9(
                FileImage::create(target, sector)?,
                &destination_key,
                &enroll,
                novafs::crypto::HostEntropy,
            )?;
            pool.verify_migration_source(&receipt)?;
            super::migration_digest(
                &image,
                Some(receipt.destination()),
                receipt.units(),
                novafs::secure_pool::migration::image_digest(&pool.into_device())?,
            )?;
            return emit(&image, io::BufWriter::new(io::stdout().lock()));
        }
        ("file-times", [id, path]) => {
            let t = pool.times(num(id)?, path)?;
            println!(
                "{}",
                serde_json::json!({"btime":t.btime.parts(),"mtime":t.mtime.parts(),"ctime":t.ctime.parts(),"atime":t.atime.parts()})
            );
            return Ok(());
        }
        ("set-atime", [id, policy]) => {
            use novafs::time::Atime;
            let policy = match *policy {
                "noatime" => Atime::No,
                "relatime" => Atime::Relative,
                "strictatime" => Atime::Strict,
                _ => return Err(Error::Invalid("politique atime")),
            };
            pool.set_atime_policy(num(id)?, policy)?;
            None
        }
        ("migration-receipt", []) => {
            let r = pool.migration_receipt()?.ok_or(Error::NotFound)?;
            let lineages=r.lineages().zip(r.seals()).map(|((ids,old,new),(_,data,metadata))|serde_json::json!({"fs":label(&ids.fs),"lv":label(&ids.lv),"origine":label(&old),"destination":label(&new),"scellements_donnees":data,"scellements_metadonnees":metadata})).collect::<Vec<_>>();
            let views=r.views().map(|v| serde_json::json!({"id":v.id,"lignee":v.lineage,"type":super::shared::kind(v.kind),"parent":match v.parent{novafs::secure_pool::ViewParent::Origin=>"origine",_=>"parent historique inconnu"}})).collect::<Vec<_>>();
            println!(
                "{}",
                serde_json::json!({"pool":label(&r.destination()),"pool_source":label(&r.source()),"racine_source":label(&r.source_root()),"racine_destination":label(&r.destination_root()),"empreinte_partage":label(&r.graph_digest()),"generation_source":r.source_generation(),"generation_destination":r.destination_generation(),"unites":r.units(),"lignees":lineages,"vues":views,"recu":label(r.as_bytes())})
            );
            return Ok(());
        }
        ("volume-create", [owner, fs, lv, n, codec, mode]) => {
            let provision = match *mode {
                "chiffre" => Some(enrollment(&mut key_pipe::pipe(4)?)?),
                "clair" => None,
                _ => return Err(Error::Invalid("mode de volume P7")),
            };
            Some(pool.create(
                identifier(owner)?,
                Ids {
                    fs: identifier(fs)?,
                    lv: identifier(lv)?,
                },
                num(n)?,
                super::shared::compression(codec)?,
                provision.as_ref().map(|(p, r)| (p.as_slice(), r)),
            )?)
        }
        ("file-create", [id, path]) => {
            pool.create_file(num(id)?, path)?;
            None
        }
        ("file-write", [id, path, at, "--donnees-fd4"]) => {
            let mut bytes = Zeroizing::new(Vec::with_capacity(65537));
            key_pipe::pipe(4)?.take(65537).read_to_end(&mut bytes)?;
            if bytes.len() > 65536 {
                return Err(Error::Limit("écriture de plus de 64 Kio"));
            }
            pool.write_file(num(id)?, path, num(at)?, &bytes)?;
            None
        }
        ("file-read", [id, path, at, len]) => {
            let n = num(len)?;
            if n > 65536 {
                return Err(Error::Limit("lecture de plus de 64 Kio"));
            }
            // Cette commande extrait des octets d'une image hors ligne et publie uniquement les données.
            let bytes = Zeroizing::new(pool.inspect_file(num(id)?, path, num(at)?, n as usize)?);
            io::stdout().lock().write_all(&bytes)?;
            return Ok(());
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
        ("set-compression", [id, policy]) => {
            pool.set_compression(num(id)?, super::shared::compression(policy)?)?;
            None
        }
        ("list-views", []) => {
            let domains: Vec<_> = pool.encrypted_domains().iter().map(|d| label(d)).collect();
            let mut views = Vec::new();
            for v in pool.list()? {
                views.push(serde_json::json!({"id":v.id,"domaine":label(&pool.domain(v.id)?),"type":super::shared::kind(v.kind),"proprietaire":label(&v.owner),"blocs":v.blocks}));
            }
            println!(
                "{}",
                serde_json::json!({"pool":label(&frame.pool),"generation":pool.generation(),"domaines_chiffres_retenus":domains,"vues":views})
            );
            return Ok(());
        }
        ("export", [id]) => {
            pool.export(
                num(id)?,
                sector,
                &backup()?,
                io::BufWriter::new(io::stdout().lock()),
            )?;
            return Ok(());
        }
        ("restore-into", [owner, codec, "--archive-fd4"]) => {
            if !pool.is_p9() {
                return Err(Error::Invalid("pool P9 requis"));
            }
            let mut pipe = key_pipe::pipe(4)?;
            let transport = key(&mut pipe)?;
            let (phrase, recovery) = read_enrollment(&mut pipe)?;
            Some(pool.restore(
                &mut pipe,
                &transport,
                identifier(owner)?,
                &phrase,
                &recovery,
                super::shared::compression(codec)?,
            )?)
        }
        ("restore", [_, owner, codec]) => {
            let mut pipe = key_pipe::pipe(4)?;
            let transport = key(&mut pipe)?;
            let (phrase, recovery) = enrollment(&mut pipe)?;
            Some(pool.restore(
                io::stdin().lock(),
                &transport,
                identifier(owner)?,
                &phrase,
                &recovery,
                super::shared::compression(codec)?,
            )?)
        }
        _ => return Err(Error::Invalid("syntaxe de la commande P7")),
    };
    if let Some(id) = id {
        let receipt = pool.receipt(id)?;
        eprintln!(
            "{}",
            serde_json::json!({"objet":id,"domaine":label(&pool.domain(id)?),"empreinte":label(&receipt.root),"pool":label(&receipt.pool)})
        );
    }
    image_stream(&io::stdout())?;
    emit(&pool.into_device(), io::BufWriter::new(io::stdout().lock()))
}
