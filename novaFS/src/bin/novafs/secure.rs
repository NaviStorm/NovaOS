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

fn end(input: &mut impl Read) -> Result<()> {
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
fn enrollment(input: &mut impl Read) -> Result<(Zeroizing<Vec<u8>>, Secret)> {
    let mut len = [0; 2];
    input.read_exact(&mut len)?;
    let n = usize::from(u16::from_le_bytes(len));
    if !(1..=1024).contains(&n) {
        return Err(Error::Invalid("longueur de phrase P7"));
    }
    let mut phrase = Zeroizing::new(vec![0; n]);
    input.read_exact(&mut phrase)?;
    let recovery = key(input)?;
    end(input)?;
    Ok((phrase, recovery))
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
        SecurePool::format(image, frame.pool, &frame.admin)?
    } else {
        load(&mut image, &mut io::stdin().lock())?;
        let mut pool = SecurePool::open(image, frame.pool, &frame.admin)?;
        for (id, cred) in &frame.domains {
            pool.unlock(*id, cred.borrowed())?;
        }
        pool
    };
    let id = match (cmd, args) {
        ("pool-format", [_, "p7"]) => None,
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
            let bytes = Zeroizing::new(pool.read_file(num(id)?, path, num(at)?, n as usize)?);
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
