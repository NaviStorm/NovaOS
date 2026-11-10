//! Tests de la vue montée et de l'invalidation. Le support stable peut être
//! altéré séparément dans ce banc, sans passer par un second écrivain NovaFS.
use novafs::{
    Block, Device, Engine, Error, Ids, Mode, Ref, Result, View,
    device::{Persistence, SimDevice, SimFault},
    tree::{Family, ReadTree},
};
use std::sync::{
    Arc, Barrier,
    atomic::{AtomicU64, Ordering},
};

const IDS: Ids = Ids {
    fs: [0xa5; 16],
    lv: [0x3d; 16],
};

#[test]
fn limites_cache_reutilisation_cow_et_echec_f2() -> Result<()> {
    for sector in [512, 4096] {
        let dev = SimDevice::new(384, sector)?;
        let ctrl = dev.control();
        let mut fs = Engine::format(dev, IDS)?;
        fs.create("/f")?;
        fs.write("/f", 0, &[1; 4096])?;
        fs.set_cache_budget(8192)?;
        assert_eq!(fs.cache_stats().capacity_pages, 1);
        for i in 1..=4u8 {
            assert_eq!(fs.read("/f", 0, 4096)?, [i; 4096]);
            assert_eq!(fs.cache_stats().pages, 1);
            assert!(fs.cache_stats().charged_bytes <= 8192);
            fs.write("/f", 0, &[i + 1; 4096])?;
        }
        assert!(fs.cache_stats().evictions > 0);
        // Le budget réduit à zéro reste désactivé après les lectures.
        fs.set_cache_budget(0)?;
        assert_eq!(fs.cache_stats().charged_bytes, 0);
        assert_eq!(fs.read("/f", 0, 4096)?, [5; 4096]);
        assert_eq!(fs.cache_stats().charged_bytes, 0);

        fs.set_cache_budget(512 * 1024)?;
        for i in 5..25u8 {
            assert_eq!(fs.read("/f", 0, 4096)?, [i; 4096]);
            assert_eq!(fs.read("/f", 0, 4096)?, [i; 4096]);
            fs.write("/f", 0, &[i + 1; 4096])?;
        }
        assert!(fs.cache_stats().invalidations > 0);
        assert_eq!(fs.read("/f", 0, 4096)?, [25; 4096]);
        ctrl.inject(SimFault::Flush {
            ordinal: 2,
            persist_newest: 0,
        })?;
        assert!(matches!(
            fs.write("/f", 0, &[26; 4096]),
            Err(Error::Publication { phase: "F2", .. })
        ));
        assert_eq!(fs.cache_stats().charged_bytes, 0);
        assert!(matches!(fs.read("/f", 0, 4096), Err(Error::UnknownOutcome)));
        assert!(matches!(View::raw(&fs, 0), Err(Error::UnknownOutcome)));
        assert!(matches!(
            ReadTree::read(&fs, Ref::ZERO, Family::Space),
            Err(Error::UnknownOutcome)
        ));
        let dev = fs.into_device();
        ctrl.power_cut(Persistence::None)?;
        let reader = Engine::mount(dev, IDS, Mode::ReadOnly)?;
        assert_eq!(reader.cache_stats().pages, 0);
        assert_eq!(reader.read("/f", 0, 4096)?, [25; 4096]);
        reader.verify()?;
    }
    Ok(())
}

// Faire terminer une lecture du support après la détection d'une corruption
// par un autre thread. Les deux barrières sont exclusivement du banc hôte.
struct Suspended {
    disk: SimDevice,
    hold: Arc<AtomicU64>,
    rendezvous: Arc<Barrier>,
}
impl Device for Suspended {
    fn blocks(&self) -> u64 {
        self.disk.blocks()
    }
    fn read(&self, b: u64) -> Result<Block> {
        let saved = self.disk.read(b)?;
        if self
            .hold
            .compare_exchange(b, u64::MAX, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            self.rendezvous.wait();
            self.rendezvous.wait();
        }
        Ok(saved)
    }
    fn write(&mut self, b: u64, data: &Block) -> Result<()> {
        self.disk.write(b, data)
    }
    fn flush(&mut self) -> Result<()> {
        self.disk.flush()
    }
}

#[test]
fn corruption_connue_pendant_une_lecture_en_vol() -> Result<()> {
    for sector in [512, 4096] {
        let disk = SimDevice::new(384, sector)?;
        let ctrl = disk.control();
        let hold = Arc::new(AtomicU64::new(u64::MAX));
        let rendezvous = Arc::new(Barrier::new(2));
        let mut fs = Engine::format(
            Suspended {
                disk,
                hold: hold.clone(),
                rendezvous: rendezvous.clone(),
            },
            IDS,
        )?;
        fs.create("/f")?;
        fs.write("/f", 0, &[0x51; 4096])?;
        let stable = ctrl.snapshot().device();
        let block = (2..stable.blocks())
            .find(|&b| stable.read(b).unwrap() == [0x51; 4096])
            .unwrap();
        fs.reclaim_cache();
        hold.store(block, Ordering::Release);
        std::thread::scope(|scope| {
            let pending = scope.spawn(|| fs.read("/f", 0, 4096));
            rendezvous.wait();
            ctrl.replace_stable(block, [0xff; 4096]).unwrap();
            let verdict = fs.verify();
            rendezvous.wait();
            assert!(matches!(verdict, Err(Error::Corrupt(_))));
            assert!(matches!(pending.join().unwrap(), Err(Error::Unavailable)));
        });
        assert_eq!(fs.cache_stats().charged_bytes, 0);
        assert!(matches!(fs.read("/f", 0, 4096), Err(Error::Unavailable)));
    }
    Ok(())
}

#[test]
fn les_erreurs_des_traits_publics_gelent_aussi_les_lectures_chaudes() -> Result<()> {
    for sector in [512, 4096] {
        let disk = SimDevice::new(384, sector)?;
        let ctl = disk.control();
        let mut fs = Engine::format(disk, IDS)?;
        fs.create("/chaud")?;
        fs.write("/chaud", 0, b"garde")?;
        fs.create("/froid")?;
        let payload = [0xa9; 4096];
        fs.write("/froid", 0, &payload)?;
        let base = ctl.snapshot();
        let address = (2..base.device().blocks())
            .find(|&b| View::raw(&fs, b).unwrap() == payload)
            .unwrap();
        let birth = fs.generation();
        let cold = Ref {
            block: address,
            birth,
            hash: novafs::codec::data_hash(IDS, address, birth, &payload),
        };
        for action in ["noeud", "donnee", "brut", "fichier"] {
            let disk = base.device();
            let control = disk.control();
            let reader = Engine::mount(disk, IDS, Mode::ReadOnly)?;
            assert_eq!(reader.read("/chaud", 0, 5)?, b"garde");
            assert!(reader.cache_stats().charged_bytes > 0);
            match action {
                "noeud" => {
                    let root = reader.manifest().roots[Family::Space.index()];
                    let mut bytes = View::raw(&reader, root.block)?;
                    bytes[18] ^= 1;
                    control.replace_stable(root.block, bytes)?;
                    assert!(matches!(
                        ReadTree::read(&reader, root, Family::Space),
                        Err(Error::Corrupt(_))
                    ));
                }
                "brut" => {
                    control.inject(SimFault::Read { block: address })?;
                    assert!(matches!(View::raw(&reader, address), Err(Error::Io(_))));
                }
                "donnee" | "fichier" => {
                    control.replace_stable(address, [0; 4096])?;
                    if action == "donnee" {
                        assert!(matches!(
                            View::checked_data(&reader, cold),
                            Err(Error::Corrupt(_))
                        ));
                    } else {
                        // L'opération englobante doit garder la première cause.
                        assert!(matches!(
                            reader.read("/froid", 0, 8),
                            Err(Error::Corrupt(_))
                        ));
                    }
                }
                _ => unreachable!(),
            }
            let hits = reader.cache_stats().hits;
            assert_eq!(reader.cache_stats().charged_bytes, 0);
            assert!(matches!(
                reader.read("/chaud", 0, 5),
                Err(Error::Unavailable)
            ));
            assert!(matches!(View::raw(&reader, 0), Err(Error::Unavailable)));
            assert_eq!(reader.cache_stats().hits, hits);
        }
    }
    Ok(())
}
