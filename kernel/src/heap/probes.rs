//! Sondes exécutées par le noyau réel via un argument Limine explicite.
use super::{global::GLOBAL, *};
use crate::hal::{log, memory::Probe, paging};
use alloc::{
    alloc::{alloc_zeroed, dealloc, realloc},
    boxed::Box,
    string::String,
    vec::Vec,
};
use core::ptr::NonNull;

/// Bloc client de test. Tous ses octets sont initialisés.
struct Allocation {
    ptr: NonNull<u8>,
    layout: Layout,
}

impl Allocation {
    fn new(size: usize, align: usize) -> Self {
        let layout = Layout::from_size_align(size, align).unwrap();
        // Sûreté : layout non nul et valide. Le bloc rendu appartient à cette instance.
        let ptr = NonNull::new(unsafe { alloc_zeroed(layout) }).expect("allocation de test");
        assert_eq!(ptr.addr().get() % align, 0);
        Self { ptr, layout }
    }

    fn bytes(&mut self) -> &mut [u8] {
        // Sûreté : unique emprunt du bloc vivant, entièrement initialisé.
        unsafe { core::slice::from_raw_parts_mut(self.ptr.as_ptr(), self.layout.size()) }
    }

    fn resize(&mut self, size: usize, align: usize) -> bool {
        let new = Layout::from_size_align(size, align).unwrap();
        // Sûreté : allocation exclusive, layout d’origine conservé jusqu’au succès.
        let ptr = unsafe {
            if self.layout.align() == align {
                realloc(self.ptr.as_ptr(), self.layout, size)
            } else {
                super::realign(self.ptr.as_ptr(), self.layout, new)
            }
        };
        let Some(ptr) = NonNull::new(ptr) else {
            return false;
        };
        if size > self.layout.size() {
            // Sûreté : seuls les nouveaux octets de la réallocation sont initialisés.
            unsafe {
                ptr.as_ptr()
                    .add(self.layout.size())
                    .write_bytes(0, size - self.layout.size())
            };
        }
        self.ptr = ptr;
        self.layout = new;
        true
    }
}

impl Drop for Allocation {
    fn drop(&mut self) {
        // Sûreté : un seul propriétaire, adresse et layout de la dernière réallocation.
        unsafe { dealloc(self.ptr.as_ptr(), self.layout) };
    }
}

fn fragmentation() {
    for round in 0..8u8 {
        let mut slots: [Option<Allocation>; 128] =
            core::array::from_fn(|_| Some(Allocation::new(40, 8)));
        let mut addresses = [0usize; 128];
        for (i, slot) in slots.iter_mut().enumerate() {
            let block = slot.as_mut().unwrap();
            addresses[i] = block.ptr.addr().get();
            block.bytes().fill(i as u8 ^ round);
            if i % 2 == 0 {
                *slot = None;
            }
        }
        for i in (0..slots.len()).step_by(2) {
            let mut block = Allocation::new(40, 8);
            assert_eq!(block.ptr.addr().get(), addresses[i]);
            assert!(block.bytes().iter().all(|&v| v == 0));
            slots[i] = Some(block);
            assert!(
                slots[i + 1]
                    .as_mut()
                    .unwrap()
                    .bytes()
                    .iter()
                    .all(|&v| v == (i + 1) as u8 ^ round)
            );
        }
        for item in &mut slots[..32] {
            *item = None;
        }
        let merged = Allocation::new(2000, 64);
        assert_eq!(merged.ptr.addr().get(), addresses[0]);
    }
    log(format_args!(
        "TAS fragmentation, fusion et reutilisation OK"
    ));
}

fn alignments() {
    for power in 0..=21 {
        let mut block = Allocation::new(13, 1 << power);
        block.bytes().copy_from_slice(b"0123456789abc");
        assert_eq!(block.bytes(), b"0123456789abc");
    }
    let mut a = Allocation::new(8, 1);
    a.bytes().copy_from_slice(b"12345678");
    let original = a.ptr;
    assert!(a.resize(8192, 1));
    assert_eq!(a.ptr, original);
    assert_eq!(&a.bytes()[..8], b"12345678");
    assert!(a.resize(8, 1));
    assert_eq!(a.ptr, original);
    let mut b = Allocation::new(11, 1);
    b.bytes().copy_from_slice(b"abcdefghijk");
    let moved = b.ptr;
    assert!(b.resize(128, 65536));
    assert_ne!(b.ptr, moved);
    assert_eq!(b.ptr.addr().get() % 65536, 0);
    assert_eq!(&b.bytes()[..11], b"abcdefghijk");
    assert_eq!(a.bytes(), b"12345678");
    log(format_args!(
        "TAS alignements 1..2M, realloc et realignement OK"
    ));
}

fn collections() {
    let boxed = Box::new(core::hint::black_box([0x42u8; 321]));
    let mut values = Vec::new();
    for i in 0..4097u32 {
        values.push(i.wrapping_mul(17));
    }
    assert!(values.iter().enumerate().all(|(i, &v)| v == i as u32 * 17));
    values.truncate(37);
    values.shrink_to_fit();
    assert_eq!(values[36], 612);
    let mut text = String::from("entrée");
    for _ in 0..90 {
        text.push_str(" à €");
    }
    assert_eq!(text.chars().filter(|&c| c == '€').count(), 90);
    assert!(core::hint::black_box(&boxed).iter().all(|&v| v == 0x42));
    GLOBAL
        .with_state(|s| {
            s.heap
                .inspect(boxed.as_ptr().addr(), Layout::new::<[u8; 321]>())?;
            s.heap.inspect(
                values.as_ptr().addr(),
                Layout::array::<u32>(values.capacity()).unwrap(),
            )?;
            s.heap.inspect(
                text.as_ptr().addr(),
                Layout::array::<u8>(text.capacity()).unwrap(),
            )?;
            Ok(())
        })
        .unwrap();
    log(format_args!("TAS Box Vec String OK"));
}

fn checks() {
    GLOBAL
        .with_state(|s| {
            let layout = Layout::from_size_align(40, 8).unwrap();
            assert_eq!(
                block_size(Layout::from_size_align(isize::MAX as usize, 1).unwrap()),
                Err(Error::NoMemory)
            );
            let p = s.heap.allocate(&mut s.pages, layout)?;
            assert_eq!(
                s.heap.free(&mut s.pages, p.wrapping_add(1), layout),
                Err(Error::InvalidPointer)
            );
            assert_eq!(
                s.heap
                    .free(&mut s.pages, p, Layout::from_size_align(39, 8).unwrap()),
                Err(Error::WrongLayout)
            );
            s.heap.free(&mut s.pages, p, layout)?;
            // Test du moteur contrôlé : un double appel à GlobalAlloc::dealloc
            // violerait le contrat Rust avant même d’entrer dans l’allocateur.
            assert_eq!(
                s.heap.free(&mut s.pages, p, layout),
                Err(Error::InvalidPointer)
            );
            Ok(())
        })
        .unwrap();
    log(format_args!(
        "TAS pointeur interieur, disposition et double liberation rejetes"
    ));
}

// Motif déterministe de tailles et de durées de vie mélangées. Vérifie les
// octets conservés, y compris lors des réductions de la fenêtre engagée.
fn mixed() {
    let mut slots: [Option<Allocation>; 32] = core::array::from_fn(|_| None);
    let mut seed = 0x6234_9355u32;
    for _ in 0..1500 {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        let i = (seed as usize >> 8) % slots.len();
        let size = 1 + (seed as usize >> 18) % 9000;
        let align = 1 << (seed % 13);
        let pattern = i as u8 + 1;
        if let Some(block) = slots[i].as_mut() {
            let old = block.layout.size();
            assert!(block.bytes().iter().all(|&b| b == pattern));
            if seed & 8 == 0 {
                slots[i] = None;
            } else {
                assert!(block.resize(size, align));
                assert_eq!(block.ptr.addr().get() % align, 0);
                assert!(block.bytes()[..size.min(old)].iter().all(|&b| b == pattern));
                block.bytes().fill(pattern);
            }
        } else {
            let mut block = Allocation::new(size, align);
            block.bytes().fill(pattern);
            slots[i] = Some(block);
        }
    }
    for (i, block) in slots.iter_mut().enumerate() {
        if let Some(block) = block {
            assert!(block.bytes().iter().all(|&b| b == i as u8 + 1));
        }
    }
    log(format_args!("TAS 1500 operations mixtes OK"));
}

pub(crate) fn shortage() {
    const SCRATCH: usize = BASE + 2 * CAPACITY;
    const REQUEST: usize = 64 << 10;
    let baseline = audit(true);
    let mut anchor = Allocation::new(27, 8);
    anchor.bytes().fill(0x71);
    let counts = || {
        GLOBAL
            .with_state(|s| Ok((s.heap.committed, s.pages.free_pages())))
            .unwrap()
    };
    let initial = audit(true);
    // Immobiliser de vrais cadres dans une fenêtre distincte. La MMU reste la
    // seule source de pages du tas et conserve quelques cadres pour amorcer
    // l’extension avant son échec. Aucune allocation Rust dans cette section.
    let mut held = 0;
    loop {
        let done = GLOBAL
            .with_state(|s| {
                for _ in 0..32 {
                    if s.pages.free_pages() <= 8 {
                        return Ok(true);
                    }
                    s.pages
                        .allocate_page(
                            VirtualAddress(SCRATCH + held * PAGE_BYTES),
                            Access::ReadWrite,
                        )
                        .map_err(Error::Paging)?;
                    held += 1;
                }
                Ok(false)
            })
            .unwrap();
        if done {
            break;
        }
    }
    let constrained = counts();
    let original = anchor.ptr;
    let mut v = Vec::<u8>::new();
    assert!(v.try_reserve_exact(REQUEST).is_err());
    assert_eq!(v.capacity(), 0);
    assert_eq!(counts(), constrained);
    assert!(!anchor.resize(REQUEST * 2, 8));
    assert_eq!(anchor.ptr, original);
    assert!(anchor.bytes().iter().all(|&v| v == 0x71));
    assert_eq!(counts(), constrained);
    assert!(v.try_reserve_exact(usize::MAX).is_err());

    while held > 0 {
        GLOBAL
            .with_state(|s| {
                let lower = held.saturating_sub(32);
                for i in (lower..held).rev() {
                    // Sûreté : chaque page appartient exclusivement à la sonde, sans référence.
                    unsafe {
                        s.pages
                            .release_page(VirtualAddress(SCRATCH + i * PAGE_BYTES))
                    }
                    .map_err(Error::Paging)?;
                }
                held = lower;
                Ok(())
            })
            .unwrap();
    }
    assert_eq!(audit(true), initial);
    v.try_reserve_exact(REQUEST)
        .expect("même demande après pénurie");
    v.resize(REQUEST, 0xa5);
    assert!(anchor.resize(REQUEST * 2, 8));
    assert!(anchor.bytes()[..27].iter().all(|&b| b == 0x71));
    assert_eq!(v[v.len() - 1], 0xa5);
    drop(v);
    drop(anchor);
    assert_eq!(audit(true), baseline);
    log(format_args!(
        "TAS penurie physique, rollback, realloc conserve et reprise OK"
    ));
}

pub(crate) fn test(scenario: &[u8]) {
    match scenario {
        b"heap-test" => {
            fragmentation();
            alignments();
            mixed();
            checks();
            collections();
        }
        b"heap-oom" => shortage(),
        b"heap-infallible" => {
            log(format_args!("SONDE allocation infaillible"));
            // La variante infaillible doit passer au gestionnaire de panique du noyau.
            let _v = core::hint::black_box(Vec::<u8>::with_capacity(core::hint::black_box(
                CAPACITY + 1,
            )));
            panic!("pénurie ignorée");
        }
        b"heap-corrupt" => {
            let mut a = Allocation::new(9, 8);
            a.bytes().fill(0);
            GLOBAL
                .with_state(|s| {
                    s.heap.inspect(a.ptr.addr().get(), a.layout)?;
                    let tag = a.ptr.addr().get() + a.layout.size();
                    // Sûreté : injection par l’allocateur dans son propre trailer initialisé,
                    // sous verrou ; aucun débordement via la référence du client.
                    unsafe { (tag as *mut u64).write_unaligned(0) };
                    Ok(())
                })
                .unwrap();
            drop(a);
            panic!("sentinelle ignorée");
        }
        b"heap-reentrant" => {
            GLOBAL
                .with_state(|_| {
                    audit(false);
                    Ok(())
                })
                .unwrap();
            panic!("réentrée ignorée");
        }
        b"heap-metadata" => {
            let _a = Allocation::new(9, 8);
            GLOBAL
                .with_state(|s| {
                    let mut tree = s.heap.tree();
                    // Une racine entièrement libre contredit le décompte des blocs vivants.
                    tree.empty_first_page();
                    Ok(())
                })
                .unwrap();
            audit(false);
            panic!("arbre corrompu accepté");
        }
        b"heap-guard" => {
            let _anchor = Allocation::new(17, 8);
            let address = GLOBAL
                .with_state(|s| Ok(s.heap.base + s.heap.committed))
                .unwrap();
            log(format_args!("SONDE adresse={address:#x} acces=lecture"));
            // Sûreté : sonde terminale explicite sur la garde haute non mappée.
            unsafe { paging::probe(address as u64, Probe::Read) };
            panic!("garde accessible");
        }
        _ => panic!("scénario de tas inconnu"),
    }
}
