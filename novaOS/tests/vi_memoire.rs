//! Banc Darwin. Les trois modules proviennent directement de l'ELF vi.
#![allow(dead_code)]
extern crate alloc;
#[path = "../../components/vi/buffer.rs"]
mod buffer;
#[path = "../../components/vi/gap.rs"]
mod gap;
#[path = "../../components/vi/history.rs"]
mod history;

use buffer::Buffer;
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::hint::black_box;
use std::time::Instant;

thread_local! { static ALLOCATIONS: Cell<Option<usize>> = const { Cell::new(None) }; }
struct Injection;
// Sûreté : System conserve la propriété des blocs. L'injection retourne le pointeur
// nul prévu par GlobalAlloc avant toute allocation, uniquement dans le fil du test.
unsafe impl GlobalAlloc for Injection {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let refuse = ALLOCATIONS.with(|v| match v.get() {
            Some(0) => true,
            Some(n) => {
                v.set(Some(n - 1));
                false
            }
            None => false,
        });
        if refuse {
            std::ptr::null_mut()
        } else {
            unsafe { System.alloc(layout) }
        }
    }
    unsafe fn dealloc(&self, p: *mut u8, layout: Layout) {
        unsafe { System.dealloc(p, layout) }
    }
}
#[global_allocator]
static HEAP: Injection = Injection;

fn panne<T>(after: usize, action: impl FnOnce() -> T) -> T {
    ALLOCATIONS.with(|v| v.set(Some(after)));
    let result = action();
    ALLOCATIONS.with(|v| v.set(None));
    result
}

#[test]
fn allocation_et_branche_de_retablissement() {
    for n in 0..6 {
        let mut b = Buffer::new("été\r\n".repeat(1000)).unwrap();
        b.change();
        b.insert('É').unwrap();
        b.change();
        b.insert('!').unwrap();
        b.undo_redo(false).unwrap();
        b.saved();
        let before = b.compact().to_owned();
        let at = b.at;
        b.change();
        if panne(n, || b.delete_line()).is_err() {
            assert_eq!(b.at, at);
            assert_eq!(b.compact(), before);
            assert!(!b.dirty());
            b.undo_redo(true).unwrap();
            assert!(b.compact().starts_with("É!été"));
        } else {
            assert!(b.undo_redo(true).is_err());
        }
    }
    for n in 0..6 {
        let mut b = Buffer::new("été".repeat(3000)).unwrap();
        let original = b.compact().to_owned();
        b.change();
        if panne(n, || b.insert('🦊')).is_err() {
            assert_eq!(b.at, 0);
            assert_eq!(b.compact(), original);
            assert!(!b.dirty());
            assert!(b.undo_redo(false).is_err());
        } else {
            b.undo_redo(false).unwrap();
            assert_eq!(b.compact(), original);
        }
    }
}

#[test]
fn sauvegarde_annulation_sans_memoire() {
    let mut b = Buffer::new("base".into()).unwrap();
    b.change();
    b.insert('A').unwrap();
    b.saved();
    assert!(panne(0, || b.undo_redo(false)).is_err());
    assert_eq!(b.compact(), "Abase");
    assert!(!b.dirty());
    b.undo_redo(false).unwrap();
    assert!(b.dirty());
    b.undo_redo(true).unwrap();
    assert!(!b.dirty());
}

#[test]
fn curseur_apres_retablissement() {
    let mut b = Buffer::new("abc".into()).unwrap();
    b.change();
    b.insert('é').unwrap();
    b.left();
    b.finish();
    b.undo_redo(false).unwrap();
    b.undo_redo(true).unwrap();
    assert_eq!(b.at, 0);
    assert_eq!(b.compact(), "éabc");
}

#[test]
fn evictions_par_groupes_entiers() {
    let mut b = Buffer::new(String::new()).unwrap();
    for _ in 0..3000 {
        b.change();
        for c in "éZ".chars() {
            b.insert(c).unwrap();
        }
    }
    let mut count = 0;
    while b.undo_redo(false).is_ok() {
        count += 1;
    }
    assert!(count > 1 && count < 3000);
    assert_eq!(b.compact(), "éZ".repeat(3000 - count));
    for _ in 0..count {
        b.undo_redo(true).unwrap();
    }
    assert_eq!(b.compact(), "éZ".repeat(3000));
    assert!(b.history_bytes() <= 256 * 1024);
    let mut b = Buffer::new("é".repeat(70000)).unwrap();
    b.change();
    b.delete_line().unwrap();
    assert!(b.truncated());
    assert_eq!(b.compact(), "");
    assert!(b.undo_redo(false).is_err());
    b.change();
    b.insert('猫').unwrap();
    b.undo_redo(false).unwrap();
    assert_eq!(b.compact(), "");
}

#[test]
fn journal_differentiel_utf8() {
    let mut b = Buffer::new("été\r\nβ\n".into()).unwrap();
    let mut states = vec![b.compact().to_owned()];
    let mut seed = 71u32;
    for _ in 0..600 {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        let mut model = states.last().unwrap().clone();
        let positions: Vec<_> = model
            .char_indices()
            .filter(|(_, c)| *c != '\n')
            .map(|(i, _)| i)
            .chain(std::iter::once(model.len()))
            .collect();
        b.at = positions[seed as usize % positions.len()];
        b.change();
        if seed & 4 != 0 && model[b.at..].chars().next().is_some_and(|c| c != '\r') {
            model.remove(b.at);
            b.delete().unwrap();
        } else {
            let txt = if seed & 8 != 0 { "猫Ü" } else { "🙂\r\n" };
            model.insert_str(b.at, txt);
            for c in txt.chars().filter(|&c| c != '\r') {
                b.insert(c).unwrap();
            }
        }
        assert_eq!(b.compact(), model);
        assert_eq!(
            b.row(),
            model[..b.at].bytes().filter(|&c| c == b'\n').count()
        );
        states.push(model);
    }
    for state in states[..states.len() - 1].iter().rev() {
        b.undo_redo(false).unwrap();
        assert_eq!(b.compact(), state);
    }
    for state in &states[1..] {
        b.undo_redo(true).unwrap();
        assert_eq!(b.compact(), state);
    }
}

#[test]
fn mesure_tampon() {
    let empty = Buffer::new(String::new()).unwrap();
    println!(
        "VIDE structure={} tas={}",
        std::mem::size_of::<Buffer>(),
        empty.allocated()
    );
    for n in [1024, 8192, 65536, 1048576, 4194304, 16777216] {
        let mut b = Buffer::new("A".repeat(n)).unwrap();
        b.at = n / 2;
        let opened = b.allocated();
        let start = Instant::now();
        b.change();
        b.insert('é').unwrap();
        let first = start.elapsed().as_nanos();
        let start = Instant::now();
        for _ in 0..1024 {
            b.insert(black_box('猫')).unwrap();
        }
        let insert = start.elapsed().as_nanos() / 1024;
        b.change();
        let start = Instant::now();
        for _ in 0..1024 {
            b.erase().unwrap();
        }
        let erase = start.elapsed().as_nanos() / 1024;
        let start = Instant::now();
        for _ in 0..1024 {
            b.left();
            b.right(true);
        }
        let nav = start.elapsed().as_nanos() / 2048;
        println!(
            "MESURE octets={n} ouvert={opened} modifie={} historique={} premiere_ns={first} insertion_ns={insert} effacement_ns={erase} mouvement_ns={nav}",
            b.allocated(),
            b.history_bytes()
        );
        b.undo_redo(false).unwrap();
        b.undo_redo(false).unwrap();
        assert_eq!(b.compact(), "A".repeat(n));
        b.undo_redo(true).unwrap();
        b.undo_redo(true).unwrap();
    }
}

#[test]
fn annulation_apres_lecture_de_pages() {
    let old = "u".repeat(4095) + "\n";
    let mut b = Buffer::new(old.clone()).unwrap();
    b.change();
    b.delete_line().unwrap();
    b.extend_source(&"a".repeat(4096)).unwrap();
    let before = b.compact().to_owned();
    assert!(panne(0, || b.undo_redo(false)).is_err());
    assert_eq!(b.compact(), before);
    b.undo_redo(false).unwrap();
    assert_eq!(b.compact(), old + &before);
    b.undo_redo(true).unwrap();
    assert_eq!(b.compact(), before);
}
