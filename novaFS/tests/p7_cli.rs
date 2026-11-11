//! Vérification de l'exécutable livré. Seuls des secrets de test synthétiques
//! transitent dans un pipe anonyme hérité. Le fichier temporaire est une image.
use novafs::crypto::Secret;
use novafs::device::FileImage;
use novafs::secure_pool::SecurePool;
use novafs::{Compression, Ids, Result};
use serde_json::Value;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

const BLOCKS: u64 = 1200;
const POOL: [u8; 16] = [21; 16];
const ADMIN: [u8; 32] = [34; 32];
const RECOVERY: [u8; 32] = [55; 32];
const PHRASE: &[u8] = b"Phrase-pour-fixture-NovaFS";

fn cli(image: &Path, args: &[&str], wire: Option<&[u8]>) -> Output {
    // La commande shell du test ne manipule que des descripteurs. Le lanceur
    // hérite des octets via stdin et substitue le fichier image en lecture.
    let mut p = Command::new("/bin/sh")
        .args(["-c", r#"exec 3<&0; exec < "$1"; shift; exec "$@""#, "labo"])
        .arg(image)
        .arg(env!("CARGO_BIN_EXE_novafs"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = p.stdin.take().unwrap();
    if let Some(data) = wire {
        input.write_all(data).unwrap();
    }
    drop(input);
    p.wait_with_output().unwrap()
}

fn report(out: Output, expected: i32) -> Value {
    assert_eq!(
        out.status.code(),
        Some(expected),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    assert!(!out.stdout.windows(PHRASE.len()).any(|s| s == PHRASE));
    assert!(!out.stderr.windows(PHRASE.len()).any(|s| s == PHRASE));
    serde_json::from_slice(&out.stdout).unwrap()
}

fn wire(domain: [u8; 16], recovery: bool) -> Vec<u8> {
    let mut b = b"NP7KEYS1".to_vec();
    b.extend(POOL);
    b.extend(ADMIN);
    b.push(1);
    b.extend(domain);
    let secret: &[u8] = if recovery { &RECOVERY } else { PHRASE };
    b.push(if recovery { 2 } else { 1 });
    b.extend((secret.len() as u16).to_le_bytes());
    b.extend(secret);
    b
}

#[test]
fn controles_cli_ro_et_canal_secret_borne() -> Result<()> {
    for sector in [512, 4096] {
        let file = tempfile::NamedTempFile::new()?;
        let dev = FileImage::create(BLOCKS, sector)?;
        let mut p = SecurePool::format(dev, POOL, &Secret::from_bytes(ADMIN))?;
        let ids = Ids {
            fs: [7; 16],
            lv: [8; 16],
        };
        let volume = p.create(
            [9; 16],
            ids,
            256,
            Compression::Lz4,
            Some((PHRASE, &Secret::from_bytes(RECOVERY))),
        )?;
        p.create_file(volume, "/note")?;
        p.write_file(volume, "/note", 0, b"contenu-test")?;
        let domain = p.domain(volume)?;
        let receipt = p.receipt(volume)?;
        let generation = p.generation();
        std::io::copy(&mut p.into_device().into_file(), &mut file.reopen()?)?;
        let sector_arg = sector.to_string();
        let input = fs::read(file.path())?;
        let pool = ["check-pool", "1200", sector_arg.as_str()];
        let public = report(cli(file.path(), &pool, None), 3);
        assert_eq!(public["authenticite_verifiee"], false);
        assert_eq!(public["copies_protegees_redondantes"], true);
        assert!(public["references_racine_chiffrees"].as_u64().unwrap() > 0);
        let mut auth = pool.to_vec();
        auth.push("--cles-fd3");
        // Une image illisible doit échouer avant même l'ouverture du canal de clés.
        let unreadable = fs::OpenOptions::new().write(true).open(file.path())?;
        let unreadable = Command::new(env!("CARGO_BIN_EXE_novafs"))
            .args(&auth)
            .stdin(unreadable)
            .output()?;
        let incomplete = report(unreadable, 3);
        assert_eq!(incomplete["statut"], "incomplet");
        assert!(!incomplete["raison"].as_str().unwrap().contains("pipe"));
        let regular = Command::new("/bin/sh")
            .args([
                "-c",
                r#"exec 3< "$1"; exec < "$1"; shift; exec "$@""#,
                "labo",
            ])
            .arg(file.path())
            .arg(env!("CARGO_BIN_EXE_novafs"))
            .args(&auth)
            .output()?;
        report(regular, 2);
        for rec in [true, false] {
            assert_eq!(
                report(cli(file.path(), &auth, Some(&wire(domain, rec))), 0)["authenticite_verifiee"],
                true
            );
        }
        let mut truncated = wire(domain, true);
        truncated.pop();
        report(cli(file.path(), &auth, Some(&truncated)), 2);
        let mut surplus = wire(domain, true);
        surplus.push(0);
        report(cli(file.path(), &auth, Some(&surplus)), 2);
        let mut wrong = wire(domain, true);
        *wrong.last_mut().unwrap() ^= 1;
        report(cli(file.path(), &auth, Some(&wrong)), 1);
        let mut missing = wire(domain, true);
        missing.truncate(57);
        missing[56] = 0;
        report(cli(file.path(), &auth, Some(&missing)), 3);
        let id = volume.to_string();
        let pool_id = "15".repeat(16);
        let root: String = receipt.root.iter().map(|b| format!("{b:02x}")).collect();
        let mut target = [
            "check",
            "1200",
            sector_arg.as_str(),
            &pool_id,
            &id,
            &root,
            "--cles-fd3",
        ];
        assert_eq!(
            report(cli(file.path(), &target, Some(&wire(domain, true))), 0)["recu_verifie"],
            true
        );
        let fake = "00".repeat(32);
        target[5] = &fake;
        report(cli(file.path(), &target, Some(&wire(domain, true))), 1);
        assert_eq!(fs::read(file.path())?, input);

        for g in [generation, generation - 1] {
            for replica in 0..2 {
                let slot = 2 + 2 * (g % 4) as usize + replica;
                let mut erased = input.clone();
                let block = &mut erased[slot * 4096..(slot + 1) * 4096];
                assert_eq!(&block[..8], b"NP7COM01");
                assert_eq!(u64::from_le_bytes(block[40..48].try_into().unwrap()), g);
                block.fill(0);
                fs::write(file.path(), &erased)?;
                let public = report(cli(file.path(), &pool, None), 3);
                assert_eq!(public["copies_protegees_redondantes"], false);
                let failure = report(cli(file.path(), &auth, Some(&wire(domain, true))), 1);
                assert!(
                    failure["raison"]
                        .as_str()
                        .unwrap()
                        .contains("redondance dégradée")
                );
                let dev = FileImage::from_file(fs::File::open(file.path())?, BLOCKS, sector)?;
                let recovered = SecurePool::open(dev, POOL, &Secret::from_bytes(ADMIN))?;
                assert_eq!(recovered.generation(), generation);
                assert_eq!(fs::read(file.path())?, erased);
            }
        }

        // La forme publique ne permet pas de reconnaître un MAC altéré d'un bit.
        let mut damaged = input;
        let slot = (2usize..10)
            .find(|&i| &damaged[i * 4096..i * 4096 + 8] == b"NP7COM01")
            .unwrap();
        damaged[(slot + 1) * 4096 - 1] ^= 4;
        fs::write(file.path(), &damaged)?;
        report(cli(file.path(), &pool, None), 3);
        report(cli(file.path(), &auth, Some(&wire(domain, true))), 1);
        assert_eq!(fs::read(file.path())?, damaged);
    }
    Ok(())
}
