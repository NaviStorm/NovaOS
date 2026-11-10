//! Injections ressignées du catalogue ; emploient l’encodeur et le lecteur du pool.
use super::*;
#[test]
fn p01_rejects_forged_catalogues_even_with_valid_digests() -> Result<()> {
    use crate::device::SimDevice;
    let d = SimDevice::new(5 * GRAIN, 512)?;
    let h = d.control();
    let mut p = Pool::format(d)?;
    p.create([1; 16], None, 1)?;
    p.create([2; 16], Some([3; 16]), 1)?;
    let cap = h.snapshot();
    for mutate in 0..7 {
        let mut d = cap.device();
        let mut c = p.head.clone();
        match mutate {
            0 => c.entries[1].id = c.entries[0].id,
            1 => c.entries[1].segments[0].physical = c.entries[0].segments[0].physical,
            2 => c.entries[0].segments[0].physical = 5,
            3 => c.entries[0].segments[0].physical = 0,
            4 => c.entries[1].segments[0].logical = 1,
            5 => c.entries[0].segments[0].count = u64::MAX,
            6 => {}
            _ => unreachable!(),
        }
        let mut b = c.bytes()?;
        if mutate == 6 {
            b[81] = 2;
        }
        let hash = digest(b"PoolP0.catalog", &b);
        for (i, blk) in b.chunks_exact(4096).enumerate() {
            d.write(band_start(c.band) + i as u64, blk.try_into().unwrap())?;
        }
        d.flush()?;
        let slot = c.generation % 2;
        d.write(slot, &anchor(slot, &c, hash))?;
        d.flush()?;
        let bytes = d.control().snapshot().bytes()?;
        let diagnosis = crate::pool::check_pool(&d);
        assert!(matches!(
            diagnosis.slots[slot as usize],
            Err(Error::Corrupt(_))
        ));
        assert!(diagnosis.slots[(1 - slot) as usize].is_ok());
        assert!(diagnosis.pair.is_none() && diagnosis.volumes.is_empty());
        assert_eq!(d.control().snapshot().bytes()?, bytes);
        assert!(Pool::import(d).is_err(), "catalogue forgé {mutate}");
    }
    // Un export oublié simule un détenteur qui ne rend pas son bail. Même si son
    // emprunt Rust se termine, le droit d'écrire le catalogue reste suspendu.
    {
        let v = p.raw_for(ADMIN, [1; 16])?;
        std::mem::forget(v);
        assert!(matches!(p.advance(), Err(Error::Busy)));
    }
    Ok(())
}

#[test]
fn p2_catalogues_valides_localement_mais_succession_impossible() -> Result<()> {
    for sector in [512, 4096] {
        let dev = crate::device::SimDevice::new(GRAIN * 5, sector)?;
        let control = dev.control();
        let mut pool = Pool::format(dev)?;
        // Des hashes cohérents sur les deux ancres ne transforment pas un
        // catalogue muni d'un prédécesseur en genèse du format P0.
        let mut genesis = pool.head.clone();
        genesis.previous = [1; 32];
        let mut forged = control.snapshot().device();
        let digest = write_band(&mut forged, &genesis)?;
        for slot in 0..2 {
            forged.write(slot, &anchor(slot, &genesis, digest))?;
        }
        forged.flush()?;
        let c = forged.control();
        let bytes = c.snapshot().bytes()?;
        c.trace(None, None);
        let invalid = crate::pool::check_pool(&forged);
        assert!(
            invalid
                .slots
                .iter()
                .all(|s| matches!(s, Err(Error::Corrupt("genèse du catalogue de pool"))))
        );
        assert!(invalid.pair.is_none() && invalid.volumes.is_empty());
        assert!(c.events().is_empty());
        assert_eq!(c.snapshot().bytes()?, bytes);
        pool.create([1; 16], None, 1)?;
        pool.create([2; 16], Some([3; 16]), 1)?;
        pool.advance()?;
        let base = control.snapshot();
        // Chaque mutation est encodée et signée de nouveau. Les deux bandes
        // passent le contrôle local ; aucune nouvelle attribution n'a été autorisée.
        for case in 0..5 {
            let mut candidate = pool.head.clone();
            match case {
                0 => candidate.entries[0].segments[0].physical = 3,
                1 => candidate.entries[0].owner = [9; 16],
                2 => candidate.entries[0].id = [4; 16],
                3 => {
                    candidate.entries[1].id = [4; 16];
                    candidate.entries[1].kind = Type::Ready;
                }
                4 => candidate.previous = [0; 32],
                _ => unreachable!(),
            }
            let mut d = base.device();
            let c = d.control();
            let hash = write_band(&mut d, &candidate)?;
            let slot = candidate.generation % 2;
            d.write(slot, &anchor(slot, &candidate, hash))?;
            d.flush()?;
            let bytes = c.snapshot().bytes()?;
            c.trace(None, None);
            let report = crate::pool::check_pool(&d);
            assert!(report.slots.iter().all(Result::is_ok));
            assert!(matches!(report.pair, Some(Err(Error::Corrupt(_)))));
            assert!(report.volumes.is_empty());
            assert!(c.events().is_empty());
            assert_eq!(c.snapshot().bytes()?, bytes);
        }
    }
    Ok(())
}
