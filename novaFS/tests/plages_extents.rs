use novafs::device::SimDevice;
use novafs::{Block, Device, Engine, Error, Ids, Mode, Result};

const IDS: Ids = Ids {
    fs: [0x47; 16],
    lv: [0x18; 16],
};
const PAGE: usize = 4096;

#[test]
fn plages_partielles_trous_et_frontieres() -> Result<()> {
    for sector in [512, 4096] {
        let mut fs = Engine::format(SimDevice::new(512, sector)?, IDS)?;
        fs.create("/plages")?;
        let mut expected = Vec::new();
        for (offset, len, seed) in [
            (2 * PAGE + 3, 12 * PAGE, 3),
            (17 * PAGE, 2 * PAGE + 5, 31),
            (3 * PAGE + 7, 15 * PAGE + 12, 11),
            (0, 2 * PAGE + 4, 97),
        ] {
            let data: Vec<u8> = (0..len).map(|i| ((i + seed) % 251) as u8).collect();
            fs.write("/plages", offset as u64, &data)?;
            expected.resize(expected.len().max(offset + len), 0);
            expected[offset..offset + len].copy_from_slice(&data);
            for start in [
                0,
                17,
                PAGE + 1,
                2 * PAGE + 2,
                6 * PAGE - 1,
                14 * PAGE + 3,
                19 * PAGE,
            ] {
                let begin = start.min(expected.len());
                assert_eq!(
                    fs.read("/plages", start as u64, 16 * PAGE)?,
                    expected[begin..(begin + 16 * PAGE).min(expected.len())]
                );
            }
        }
        fs.verify()?;
    }
    Ok(())
}

#[test]
fn refus_sur_derniere_page_dun_extent_meme_en_reecriture_totale() -> Result<()> {
    for sector in [512, 4096] {
        let dev = SimDevice::new(384, sector)?;
        let ctl = dev.control();
        let mut fs = Engine::format(dev, IDS)?;
        fs.create("/f")?;
        let payload: Vec<u8> = (0..16 * PAGE).map(|i| (1 + i / PAGE) as u8).collect();
        fs.write("/f", 0, &payload)?;
        assert_eq!(fs.extent_count("/f")?, 1);
        let good = ctl.snapshot();
        let scan = good.device();
        let last: Block = [16; PAGE];
        let block = (2..scan.blocks())
            .find(|&b| scan.read(b).unwrap() == last)
            .unwrap();
        for overwrite in [false, true] {
            let dev = good.device();
            let ctrl = dev.control();
            let mut mounted = Engine::mount(dev, IDS, Mode::ReadWrite)?;
            ctrl.replace_stable(block, [0; PAGE])?;
            ctrl.trace(None, None);
            if overwrite {
                assert!(matches!(
                    mounted.write("/f", 0, &payload),
                    Err(Error::Corrupt(_))
                ));
            } else {
                assert!(matches!(
                    mounted.read("/f", 0, payload.len()),
                    Err(Error::Corrupt(_))
                ));
            }
            assert!(ctrl.events().is_empty());
            assert!(matches!(mounted.stat("/f"), Err(Error::Unavailable)));
        }
    }
    Ok(())
}
