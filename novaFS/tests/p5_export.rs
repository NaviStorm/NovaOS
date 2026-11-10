//! Export complet, preuve de clôture et publication après contrôle du flux.
use novafs::device::{SimDevice, SimFault};
use novafs::shared::{ObjectKind, SharedPool};
use novafs::shared_export::{check_export, export_view, restore_view};
use novafs::{Error, Ids, Result};
use sha2::{Digest, Sha256};
use std::io::{self, Write};

const GEOMETRY: u64 = 4096;
const OWNER: [u8; 16] = [3; 16];
const FS: Ids = Ids {
    fs: [5; 16],
    lv: [7; 16],
};
const HEADER: usize = 160;
const ROW: usize = 8 + 4096;
const DOMAIN: &[u8] = b"NovaFS/export/complet/v1\0";

fn archive(sector: u32, clone: bool) -> Result<(Vec<u8>, Vec<u8>)> {
    let mut p = SharedPool::format(SimDevice::new(GEOMETRY, sector)?, [17; 16])?;
    let id = p.create(OWNER, FS, 256)?;
    p.create_file(id, "/données")?;
    let mut oracle: Vec<_> = (0..9321).map(|i| ((17 * i + 3) % 253) as u8).collect();
    p.write_file(id, "/données", 0, &oracle)?;
    let snap = p.snapshot(id)?;
    let who = if clone {
        let clone = p.clone_snapshot(snap, [4; 16])?;
        let patch = [0x2b; 29];
        p.write_file(clone, "/données", 4094, &patch)?;
        oracle[4094..4094 + patch.len()].copy_from_slice(&patch);
        p.remove(snap)?;
        clone
    } else {
        snap
    };
    p.remove(id)?;
    p.advance()?;
    let mut bytes = Vec::new();
    let info = export_view(&p, who, sector, &mut bytes)?;
    assert_eq!(p.receipt(who)?.root, info.source_root);
    assert_eq!(info.pages as usize * ROW + HEADER + 40, bytes.len());
    assert!(info.pages < info.blocks);
    drop(p);
    Ok((bytes, oracle))
}

#[test]
fn restaurer_seulement_avec_archive_et_nouveau_pool() -> Result<()> {
    for source in [512, 4096] {
        for clone in [false, true] {
            let (bytes, oracle) = archive(source, clone)?;
            let info = check_export(bytes.as_slice())?;
            assert_eq!(info.sector, source);
            assert_eq!(
                info.kind,
                if clone {
                    ObjectKind::Clone
                } else {
                    ObjectKind::Snapshot
                }
            );
            for destination in [512, 4096] {
                let mut dest =
                    SharedPool::format(SimDevice::new(GEOMETRY, destination)?, [29; 16])?;
                let id = restore_view(&mut dest, bytes.as_slice(), [8; 16])?;
                let mut mounted = SharedPool::open(dest.into_device())?;
                assert_eq!(mounted.info(id)?.kind, info.kind);
                assert_eq!(mounted.info(id)?.ids, FS);
                assert_eq!(mounted.info(id)?.owner, [8; 16]);
                assert_ne!(mounted.receipt(id)?.pool, info.pool);
                assert_eq!(mounted.read_file(id, "/données", 0, 9321)?, oracle);
                if clone {
                    mounted.write_file(id, "/données", 0, b"r")?;
                } else {
                    assert!(matches!(
                        mounted.write_file(id, "/données", 0, b"r"),
                        Err(Error::ReadOnly)
                    ));
                }
            }
        }
    }
    Ok(())
}

fn reseal(bytes: &mut [u8]) {
    let prefix = bytes.len() - 32;
    let mut sha = Sha256::new();
    sha.update(DOMAIN);
    sha.update(&bytes[..prefix]);
    bytes[prefix..].copy_from_slice(&sha.finalize());
}
#[test]
fn refus_avant_reservation_destination() -> Result<()> {
    for sector in [512, 4096] {
        let (original, _) = archive(sector, true)?;
        let dev = SimDevice::new(GEOMETRY, sector)?;
        let control = dev.control();
        let mut dest = SharedPool::format(dev, [29; 16])?;
        let id = dest.create(
            [8; 16],
            Ids {
                fs: [9; 16],
                lv: [11; 16],
            },
            256,
        )?;
        let witness = dest.receipt(id)?;
        control.trace(None, None);
        let refuse = |p: &mut SharedPool<SimDevice>, input: &[u8]| {
            assert!(check_export(input).is_err());
            assert!(restore_view(p, input, [8; 16]).is_err());
            assert!(control.events().is_empty());
            assert_eq!(p.list().len(), 1);
            p.verify_receipt(&witness)
        };
        for n in [
            0,
            7,
            12,
            56,
            HEADER - 1,
            HEADER,
            HEADER + 7,
            HEADER + 8 + 513,
            original.len() - 41,
            original.len() - 40,
            original.len() - 1,
        ] {
            refuse(&mut dest, &original[..n])?;
        }
        let mut extra = original.clone();
        extra.push(0);
        refuse(&mut dest, &extra)?;
        for i in [
            0,
            8,
            12,
            16,
            112,
            HEADER + 8 + 57,
            original.len() - 33,
            original.len() - 1,
        ] {
            let mut bad = original.clone();
            bad[i] ^= 1;
            refuse(&mut dest, &bad)?;
        }
        // Des erreurs sémantiques restent détectées si le checksum externe est recalculé.
        for i in [8, 12, HEADER, HEADER + 8 + 57] {
            let mut bad = original.clone();
            bad[i] ^= 1;
            reseal(&mut bad);
            refuse(&mut dest, &bad)?;
        }
        for blocks in [255u64, 257, 1025] {
            let mut bad = original.clone();
            bad[24..32].copy_from_slice(&blocks.to_le_bytes());
            reseal(&mut bad);
            assert!(matches!(
                check_export(bad.as_slice()),
                Err(Error::Corrupt("identités ou bornes de l’export"))
            ));
            refuse(&mut dest, &bad)?;
        }
        let mut bad = original.clone();
        let count = u64::from_le_bytes(bad[144..152].try_into().unwrap());
        bad[144..152].copy_from_slice(&(count + 1).to_le_bytes());
        let pos = bad.len() - 40;
        let mut unused = vec![19; ROW];
        unused[..8].copy_from_slice(&255u64.to_le_bytes());
        bad.splice(pos..pos, unused);
        reseal(&mut bad);
        refuse(&mut dest, &bad)?;
        // Une panne d'écriture de la destination ne valide aucun objet partiel.
        control.inject(SimFault::Flush {
            ordinal: 1,
            persist_newest: 1,
        })?;
        assert!(restore_view(&mut dest, original.as_slice(), [8; 16]).is_err());
        let restored = SharedPool::open(control.snapshot().device())?;
        restored.verify_receipt(&witness)?;
        assert_eq!(restored.list().len(), 1);
    }
    Ok(())
}
struct BrokenWriter {
    remaining: usize,
    bytes: Vec<u8>,
}
impl Write for BrokenWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let n = self.remaining.min(bytes.len());
        if n == 0 {
            return Err(io::Error::other("E/S injectée"));
        }
        self.remaining -= n;
        self.bytes.extend_from_slice(&bytes[..n]);
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
#[test]
fn interruption_reelle_du_producteur_de_flux() -> Result<()> {
    for sector in [512, 4096] {
        let mut pool = SharedPool::format(SimDevice::new(GEOMETRY, sector)?, [19; 16])?;
        let o = pool.create(OWNER, FS, 256)?;
        let s = pool.snapshot(o)?;
        let mut writer = BrokenWriter {
            remaining: HEADER + 8 + 513,
            bytes: Vec::new(),
        };
        assert!(export_view(&pool, s, sector, &mut writer).is_err());
        assert!(check_export(writer.bytes.as_slice()).is_err());
        assert_eq!(writer.bytes.len(), HEADER + 8 + 513);
        assert!(export_view(&pool, o, sector, io::sink()).is_err());
    }
    Ok(())
}
