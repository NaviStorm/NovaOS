//! Transport entre exécutables Darwin. Chaque côté ouvre seulement un fichier de laboratoire.
use novafs::device::{FileImage, SimDevice};
use novafs::shared::SharedPool;
use novafs::{Ids, Result};
use std::fs::{File, OpenOptions};
use std::io::Write;

const NB: u64 = 8192;
const MAGIC: &[u8] = b"point de sauvegarde P5";

#[test]
fn echange_de_pool() -> Result<()> {
    let Some(action) = std::env::var_os("NOVAFS_P5_ECHANGE") else {
        return Ok(());
    };
    let path = std::env::var_os("NOVAFS_P5_IMAGE").unwrap();
    let sector = std::env::var("NOVAFS_P5_SECTEUR").unwrap().parse().unwrap();
    if action == "creer" {
        let disk = SimDevice::new(NB, sector)?;
        let ctl = disk.control();
        let mut pool = SharedPool::format(disk, [21; 16])?;
        let original = pool.create(
            [45; 16],
            Ids {
                fs: [12; 16],
                lv: [82; 16],
            },
            256,
        )?;
        pool.create_file(original, "/état")?;
        pool.write_file(original, "/état", 4088, MAGIC)?;
        let s = pool.snapshot(original)?;
        let c = pool.clone_snapshot(s, [23; 16])?;
        pool.write_file(c, "/état", 4094, &[17, 18, 19, 20])?;
        pool.remove(original)?;
        pool.advance()?;
        OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(path)?
            .write_all(&ctl.snapshot().bytes()?)?;
    } else {
        assert_eq!(action, "lire");
        let src = File::open(path)?;
        let pool = SharedPool::open(FileImage::from_file(src, NB, sector)?)?;
        let items = pool.list();
        assert_eq!(items.len(), 2);
        for item in items {
            let mut expected = vec![0; 4088];
            expected.extend_from_slice(MAGIC);
            match item.kind {
                novafs::shared::ObjectKind::Snapshot => {}
                novafs::shared::ObjectKind::Clone => {
                    expected[4094..4098].copy_from_slice(&[17, 18, 19, 20])
                }
                _ => panic!("origine supprimée réapparue"),
            }
            assert_eq!(pool.read_file(item.id, "/état", 0, 8192)?, expected);
        }
    }
    Ok(())
}
