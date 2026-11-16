//! Exercices de la bibliothèque avec un lecteur court et un allocateur à panne unique.
use awk_rs::{Interpreter, Lexer, Parser};
use std::alloc::{GlobalAlloc, Layout, System};
use std::io::{self, BufReader, Read, Repeat};
use std::sync::atomic::{AtomicBool, Ordering};

struct Allocator;
static FAIL_LARGE: AtomicBool = AtomicBool::new(false);

#[global_allocator]
static ALLOCATOR: Allocator = Allocator;

unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if layout.size() >= 1 << 20 && FAIL_LARGE.swap(false, Ordering::Relaxed) {
            return std::ptr::null_mut();
        }
        // The harness forwards unchanged layouts to the system allocator.
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}

struct Fault {
    first: bool,
}
impl Read for Fault {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.first {
            self.first = false;
            buf[..3].copy_from_slice(b"abc");
            return Ok(3);
        }
        Err(io::Error::other("lecture retiree"))
    }
}

#[test]
fn partial_record_is_not_emitted_after_io_error() {
    let p = Parser::new(Lexer::new("{ print }").tokenize().unwrap())
        .parse()
        .unwrap();
    let mut out = Vec::new();
    let error = Interpreter::new(&p)
        .run(vec![BufReader::new(Fault { first: true })], &mut out)
        .unwrap_err();
    assert!(error.to_string().contains("lecture retiree"));
    assert!(out.is_empty());
}

#[test]
fn allocation_failure_is_reported_before_record_output() {
    let p = Parser::new(Lexer::new("{ print }").tokenize().unwrap())
        .parse()
        .unwrap();
    let infinite: Repeat = io::repeat(b'x');
    let reader = BufReader::new(infinite.take(3 << 20));
    let mut awk = Interpreter::new(&p);
    let mut out = Vec::new();
    FAIL_LARGE.store(true, Ordering::Relaxed);
    let error = awk.run(vec![reader], &mut out).unwrap_err();
    assert!(!FAIL_LARGE.load(Ordering::Relaxed));
    assert!(error.to_string().contains("out of memory reading record"));
    assert!(out.is_empty());
}
