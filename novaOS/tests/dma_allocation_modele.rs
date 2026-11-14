//! Tester Pages::new() du transport dans un allocateur hôte à capacité imposée.
//! Compiler à côté de io.rs. Aucune page n’est rendue accessible au matériel.
#![allow(dead_code)]

#[derive(Debug)]
enum Error {
    Geometry,
    Memory,
    Timeout,
}
type Result<T> = core::result::Result<T, Error>;

#[path = "io.rs"]
mod io;

mod hal {
    pub mod timer {
        pub fn counter() -> u64 {
            0
        }
    }
}

mod memory {
    use std::{collections::BTreeSet, sync::Mutex};

    pub struct Etat {
        pub prises: usize,
        pub limite: usize,
        pub cadres: BTreeSet<u64>,
    }
    pub static ETAT: Mutex<Etat> = Mutex::new(Etat {
        prises: 0,
        limite: 0,
        cadres: BTreeSet::new(),
    });

    pub fn pin_dma() -> Result<(u64, usize), ()> {
        let mut s = ETAT.lock().unwrap();
        if s.prises == s.limite {
            return Err(());
        }
        s.prises += 1;
        let pa = s.prises as u64 * 4096;
        assert!(s.cadres.insert(pa));
        Ok((pa, pa as usize))
    }

    pub unsafe fn release_dma(pa: u64) {
        assert!(ETAT.lock().unwrap().cadres.remove(&pa));
    }

    pub fn map_io(_: u64, _: u64) -> Result<(), ()> {
        unreachable!("le constructeur de Pages ne programme aucun registre")
    }
}

#[test]
fn allocation_partielle_et_restitution() {
    let total = if cfg!(target_arch = "aarch64") { 16 } else { 7 };
    for limite in 0..=total {
        {
            let mut s = memory::ETAT.lock().unwrap();
            assert!(s.cadres.is_empty());
            s.limite = limite;
            s.prises = 0;
        }
        let pages = io::Pages::new();
        if limite < total {
            assert!(pages.is_err());
        } else {
            let pages = pages.unwrap();
            assert_eq!(memory::ETAT.lock().unwrap().cadres.len(), total);
            // Sûreté : le modèle n’a publié aucune adresse.
            unsafe { pages.release() };
        }
        assert!(memory::ETAT.lock().unwrap().cadres.is_empty());
    }
}
