//! Banc défensif compilé exclusivement dans une archive jetable par dma_propriete.py.
use super::*;
use core::sync::atomic::{AtomicUsize, Ordering::SeqCst};

static POINT: AtomicUsize = AtomicUsize::new(0);
static RESTANT: AtomicUsize = AtomicUsize::new(0);
static DECLENCHES: AtomicUsize = AtomicUsize::new(0);

pub(super) fn point(id: usize) -> Result<()> {
    if POINT.load(SeqCst) == id && RESTANT.fetch_sub(1, SeqCst) == 1 {
        POINT.store(0, SeqCst);
        DECLENCHES.fetch_add(1, SeqCst);
        return Err(Error::Timeout);
    }
    Ok(())
}

fn armer(id: usize, occurrence: usize) {
    assert_eq!(POINT.swap(id, SeqCst), 0);
    assert!(occurrence > 0);
    RESTANT.store(occurrence, SeqCst);
}

fn cadres() -> u64 {
    crate::heap::with_pages(|m| Ok(m.dma_laboratoire())).unwrap()
}
fn socle() -> u64 {
    if cfg!(target_arch = "x86_64") { 9 } else { 0 }
}
fn session() -> u64 {
    if cfg!(target_arch = "x86_64") { 7 } else { 16 }
}
fn ouverture(id: usize, occurrence: usize, attendu: u64) {
    armer(id, occurrence);
    assert!(Transport::open().is_err());
    assert_eq!(POINT.load(SeqCst), 0);
    ownership::reprendre().unwrap();
    assert_eq!(cadres(), attendu);
}

fn fermeture(id: usize, b: Bail, attendu: u64) {
    armer(id, 1);
    assert_eq!(b.close(), Err(Error::Quarantine));
    assert_eq!(POINT.load(SeqCst), 0);
    assert_eq!(cadres(), attendu);
    assert!(matches!(Transport::open(), Err(Error::Quarantine)));
    ownership::reprendre().unwrap();
    ownership::reprendre().unwrap();
    assert_eq!(cadres(), socle());
}

pub(super) fn essai() -> Result<()> {
    // Les 9 tables stationnées VT-d sont possédées par la machine entre les sessions.
    assert_eq!(cadres(), 0);
    let n = session() as usize;
    let allocations = if cfg!(target_arch = "x86_64") {
        n + 9
    } else {
        n
    };
    for i in 1..=allocations {
        ouverture(1, i, 0);
    }
    ouverture(2, 1, 0);
    #[cfg(target_arch = "x86_64")]
    ouverture(35, 1, socle()); // première activation matérielle, avant stationnement
    for id in [3, 4, 5, 6, 7, 8, 9] {
        if id == 4 {
            for i in 1..=7 {
                ouverture(id, i, socle());
            }
        } else {
            ouverture(id, 1, socle());
        }
    }
    #[cfg(target_arch = "aarch64")]
    for id in [30, 31, 32, 33, 34] {
        ouverture(id, 1, socle());
    }
    #[cfg(target_arch = "aarch64")]
    ouverture(39, 1, socle()); // MAP refusé avant que ce slot ne soit publié
    #[cfg(target_arch = "x86_64")]
    ouverture(33, 1, socle());

    for _ in 0..8 {
        for id in [10, 11, 12, 13, 14, 15] {
            let mut b = Transport::open()?;
            let lba = 8192 / b.size;
            b.begin_read(lba as u64)?;
            if id != 10 {
                b.finish_read()?;
            }
            fermeture(id, b, socle() + session());
        }
        for sens in [false, true] {
            let mut b = Transport::open()?;
            b.laboratoire_arene(sens, 4)?;
            fermeture(17, b, socle() + session() + 4);
        }
        // Deux pages entièrement traduites et la troisième encore au constructeur.
        for point_map in [1, 16] {
            let mut b = Transport::open()?;
            armer(point_map, 3);
            assert!(b.laboratoire_arene(true, 7).is_err());
            assert_eq!(POINT.load(SeqCst), 0);
            assert_eq!(
                cadres(),
                socle() + session() + if point_map == 1 { 2 } else { 3 }
            );
            drop(b); // sortie précoce sans .close() explicite
            ownership::reprendre()?;
            assert_eq!(cadres(), socle());
        }
    }
    // La réponse de la commande UNMAP soumise reste à collecter au moment du retrait.
    #[cfg(target_arch = "aarch64")]
    {
        let b = Transport::open()?;
        fermeture(36, b, socle() + session());
        let b = Transport::open()?;
        armer(37, 1);
        b.close()?; // refus simulé d’UNMAP, détachement matériel obligatoire
        assert_eq!(POINT.load(SeqCst), 0);
        assert_eq!(cadres(), socle());
        let mut b = Transport::open()?;
        armer(39, 2);
        assert!(b.laboratoire_arene(false, 3).is_err());
        assert_eq!(cadres(), socle() + session() + 2);
        b.close()?;
        assert_eq!(cadres(), socle());
        let b = Transport::open()?;
        fermeture(38, b, socle() + session());
        let mut b = Transport::open()?;
        let avant = STRAYS.load(SeqCst);
        let irq = IRQS.load(SeqCst);
        hal::timer::spi_ownership();
        wait(|| STRAYS.load(SeqCst) == avant + 1)?;
        assert!(hal::timer::spi_ownership_actif());
        let lba = 8192 / b.size;
        b.begin_read(lba as u64)?;
        b.finish_read()?;
        assert_eq!(IRQS.load(SeqCst), irq + 1);
        b.close()?;
    }
    for _ in 0..64 {
        let cle = lease::begin_session(1 | 8)?;
        assert!(lease::attach(71, cle));
        assert!(lease::admit(71, cle));
        lease::died(71); // même point de révocation que la destruction dans l’ordonnanceur
        assert!(!lease::admit(71, cle) && !lease::attach(72, cle));
        lease::retire()?;
        lease::retire()?;
        assert_eq!(cadres(), socle());
    }
    hal::log(format_args!(
        "DMA OWNERSHIP injections={} cycles=64 final={} session={} allocations={} pertes=0",
        DECLENCHES.load(SeqCst),
        cadres(),
        session(),
        allocations
    ));
    Ok(())
}
