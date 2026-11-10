//! Une migration prête à publier deux volumes dépend d'une seule décision de pool.
use novafs::device::{Event, Persistence, SimDevice};
use novafs::pool::{Pool, Type};
use novafs::shared::SharedPool;
use novafs::{Block, Device, Engine, Error, Ids, Result};

struct Source<D>(D);
impl<D: Device> Device for Source<D> {
    fn blocks(&self) -> u64 {
        self.0.blocks()
    }
    fn read(&self, b: u64) -> Result<Block> {
        self.0.read(b)
    }
    fn write(&mut self, _: u64, _: &Block) -> Result<()> {
        panic!("écriture sur la source")
    }
    fn flush(&mut self) -> Result<()> {
        panic!("synchronisation de la source")
    }
}
const OWNER: [u8; 16] = [22; 16];
fn ids(n: u8) -> Ids {
    Ids {
        lv: [n; 16],
        fs: [n + 8; 16],
    }
}

fn legacy(sector: u32) -> Result<Pool<SimDevice>> {
    let mut p = Pool::format(SimDevice::new(1024, sector)?)?;
    for n in 1..=2u8 {
        let keys = ids(n);
        p.create_for(OWNER, keys.lv, Some(keys.fs), 1)?;
        let mut fs = Engine::format(p.view(keys.lv, Type::Preparing, keys.fs)?, keys)?;
        fs.create("/source")?;
        fs.write("/source", 4095, &[n, 0xfe, n])?;
        fs.unmount()?;
        p.mark_ready(keys.lv, keys.fs)?;
    }
    Ok(p)
}

#[test]
fn migration_p0_de_deux_volumes_est_indivisible_et_relancable() -> Result<()> {
    for secteur in [512, 4096] {
        let source = Source(legacy(secteur)?.into_device());
        let d = SimDevice::new(1024, secteur)?;
        let ctl = d.control();
        let mut migrated = SharedPool::format(d, [7; 16])?;
        let empty = ctl.snapshot();
        ctl.trace(None, None);
        let branches = migrated.migrate_legacy(&source)?;
        assert_eq!(branches, vec![1, 2]);
        let operations = ctl.events();
        let f1 = operations
            .iter()
            .position(|e| *e == Event::FlushCompleted(1))
            .unwrap();
        let selector = operations
            .iter()
            .position(|e| *e == Event::WriteSubmitted(2))
            .unwrap();
        let f2 = operations
            .iter()
            .position(|e| *e == Event::FlushEffect(2))
            .unwrap();
        assert!(f1 < selector && selector < f2);
        for cut in 0..operations.len() {
            let d = empty.device();
            let ctl = d.control();
            let mut dst = SharedPool::open(d)?;
            // La première mutation stabilise le montage. La rotation ci-dessous
            // est accomplie avant la trace et conserve la destination vide.
            dst.advance()?;
            ctl.trace(Some(cut), None);
            assert!(dst.migrate_legacy(&source).is_err());
            ctl.power_cut(Persistence::Newest(usize::MAX))?;
            let mut recovered = SharedPool::open(ctl.snapshot().device())?;
            if recovered.list().is_empty() {
                assert!(cut <= selector);
                assert_eq!(recovered.migrate_legacy(&source)?, branches);
            } else {
                assert!(cut >= selector);
                assert_eq!(recovered.list().len(), 2);
            }
            for (n, id) in [1u8, 2].into_iter().zip(&branches) {
                assert_eq!(recovered.info(*id)?.owner, OWNER);
                assert_eq!(
                    recovered.read_file(*id, "/source", 4094, 4)?,
                    [0, n, 0xfe, n]
                );
            }
        }
        println!(
            "MIGRATION secteur={secteur} frontières={}",
            operations.len()
        );
        let mut has_raw = legacy(secteur)?;
        has_raw.create_for(OWNER, [34; 16], None, 1)?;
        let mut dest = SharedPool::open(empty.device())?;
        assert!(matches!(
            dest.migrate_legacy(&Source(has_raw.into_device())),
            Err(Error::Invalid(_))
        ));
        assert!(SharedPool::open(dest.into_device())?.list().is_empty());
    }
    Ok(())
}

#[test]
fn reattribution_du_temoin_sur_le_pool_minimal() -> Result<()> {
    for secteur in [512, 4096] {
        let mut p = SharedPool::format(SimDevice::new(512, secteur)?, [4; 16])?;
        let origin = p.create(OWNER, ids(1), 256)?;
        p.create_file(origin, "/temoin")?;
        let marker = [0xa7; 4096];
        p.write_file(origin, "/temoin", 0, &marker)?;
        let s = p.snapshot(origin)?;
        let wanted = p.receipt(s)?;
        p.remove(origin)?;
        p.advance()?;
        let mut p = SharedPool::open(p.into_device())?;
        assert!(matches!(p.create(OWNER, ids(1), 256), Err(Error::Exists)));
        let new = p.create([66; 16], ids(2), 256)?;
        p.create_file(new, "/temoin")?;
        for i in 0..40 {
            p.write_file(new, "/temoin", 0, &[i; 4096])?;
        }
        let p = SharedPool::open(p.into_device())?;
        p.verify_receipt(&wanted)?;
        assert_eq!(p.read_file(s, "/temoin", 0, 4096)?, marker);
        assert_eq!(p.read_file(new, "/temoin", 0, 4096)?, [39; 4096]);
    }
    Ok(())
}

#[test]
fn la_collecte_dun_fichier_du_clone_preserve_le_snapshot() -> Result<()> {
    for sector in [512, 4096] {
        let mut p = SharedPool::format(SimDevice::new(2048, sector)?, [21; 16])?;
        let o = p.create(OWNER, ids(3), 512)?;
        p.mkdir(o, "/d")?;
        p.create_file(o, "/d/long")?;
        let data = [0x71; 65536];
        for i in 0..3 {
            p.write_file(o, "/d/long", i * 65536, &data)?;
        }
        let snap = p.snapshot(o)?;
        let receipt = p.receipt(snap)?;
        let clone = p.clone_snapshot(snap, [35; 16])?;
        p.remove(o)?;
        p.rename(clone, "/d/long", "/d/ancien")?;
        p.unlink(clone, "/d/ancien")?;
        let mut batches = 0;
        while p.collect(clone)? {
            batches += 1;
            assert!(batches < 128);
        }
        assert!(batches > 0);
        p.create_file(clone, "/d/neuf")?;
        p.write_file(clone, "/d/neuf", 31 * 4096 + 1, b"c")?;
        p.reclaim_cache()?;
        assert_eq!(p.read_file(clone, "/d/neuf", 31 * 4096, 2)?, [0, b'c']);
        p.advance()?;
        let p = SharedPool::open(p.into_device())?;
        p.verify_receipt(&receipt)?;
        assert!(matches!(
            p.read_file(clone, "/d/long", 0, 1),
            Err(Error::NotFound)
        ));
        assert_eq!(
            p.read_file(clone, "/d/neuf", 31 * 4096 - 1, 3)?,
            [0, 0, b'c']
        );
        for i in 0..3 {
            assert_eq!(p.read_file(snap, "/d/long", i * 65536, 65536)?, data);
        }
    }
    Ok(())
}
