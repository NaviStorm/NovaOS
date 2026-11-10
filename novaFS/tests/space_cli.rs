use novafs::device::FileImage;
use novafs::pool::{GRAIN, Pool, Type};
use novafs::{Engine, Ids, Result};
use serde_json::Value;
use std::fs::{self, File};
use std::io::Write;
use std::process::{Command, Stdio};

fn report(image: &tempfile::NamedTempFile, args: &[&str]) -> Result<Value> {
    let before = fs::read(image.path())?;
    let result = Command::new(env!("CARGO_BIN_EXE_novafs"))
        .args(args)
        .stdin(File::open(image.path())?)
        .output()?;
    assert!(result.status.success(), "{result:?}");
    assert_eq!(fs::read(image.path())?, before);
    let data: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(data["schema"], "novafs-space-1");
    assert_eq!(data["statut"], "coherent");
    Ok(data)
}

#[test]
fn espace_hors_ligne_et_limite_de_session() -> Result<()> {
    let mut child = Command::new(env!("CARGO_BIN_EXE_novafs"))
        .args(["script", "320", "512"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"policy 80 8193\ncreate /creux\nwrite /creux 8192 01\nspace\n")?;
    let result = child.wait_with_output()?;
    assert!(result.status.success(), "{:?}", result.stderr);
    let messages = std::str::from_utf8(&result.stderr).unwrap();
    let (fs_id, lv_id) = messages
        .lines()
        .next()
        .unwrap()
        .strip_prefix("FS=")
        .unwrap()
        .split_once(" LV=")
        .unwrap();
    let live: Value = serde_json::from_str(messages.lines().nth(1).unwrap()).unwrap();
    assert_eq!(live["cible_maintenance_blocs"], 80);
    assert_eq!(live["quota_logique_octets"], 8193);
    assert_eq!(live["logique_octets"], "8193");
    let mut image = tempfile::NamedTempFile::new()?;
    image.write_all(&result.stdout)?;
    let offline = report(&image, &["space", "320", "512", fs_id, lv_id])?;
    assert_eq!(offline["logique_octets"], "8193");
    assert_eq!(offline["cible_maintenance_blocs"], 128);
    assert!(offline["quota_logique_octets"].is_null());
    assert_eq!(offline["reserves_transaction_blocs"], 0);
    assert_eq!(
        offline["alloues_blocs"].as_u64().unwrap()
            + offline["reserves_maintenance_blocs"].as_u64().unwrap()
            + offline["disponibles_blocs"].as_u64().unwrap(),
        320
    );
    Ok(())
}

#[test]
fn bilan_physique_du_pool_sur_fichier_ro() -> Result<()> {
    let file = tempfile::NamedTempFile::new()?;
    let capacity = 4 * GRAIN + 7;
    file.as_file().set_len(capacity * 4096)?;
    let ids = Ids {
        fs: [5; 16],
        lv: [7; 16],
    };
    let mut pool = Pool::format(FileImage::from_file(file.reopen()?, capacity, 4096)?)?;
    pool.create(ids.lv, Some(ids.fs), 1)?;
    {
        let mut fs = Engine::format(pool.view(ids.lv, Type::Preparing, ids.fs)?, ids)?;
        fs.create("/f")?;
        fs.write("/f", 8192, b"x")?;
        drop(fs.unmount()?);
    }
    pool.mark_ready(ids.lv, ids.fs)?;
    pool.create([9; 16], None, 1)?;
    drop(pool);
    let offline = report(&file, &["space-pool", &capacity.to_string(), "4096"])?;
    assert_eq!(offline["capacite_blocs"], capacity);
    assert_eq!(offline["grain_entete_blocs"], GRAIN);
    assert_eq!(offline["queue_blocs"], 7);
    assert_eq!(offline["attribues_blocs"], 2 * GRAIN);
    assert_eq!(offline["disponibles_blocs"], GRAIN);
    assert_eq!(offline["partages_entre_lv_blocs"], 0);
    let rows = offline["volumes"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    let ready = rows.iter().find(|r| r["type"] == "pret").unwrap();
    assert_eq!(ready["garantis_blocs"], GRAIN);
    assert_eq!(ready["fs"]["quota_logique_octets"], GRAIN * 4096);
    assert_eq!(ready["fs"]["logique_octets"], "8193");
    assert!(rows.iter().find(|r| r["type"] == "raw").unwrap()["fs"].is_null());
    Ok(())
}
