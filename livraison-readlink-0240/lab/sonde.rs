#![no_std]
#![no_main]
extern crate alloc;
use alloc::{format, vec::Vec};
use nova_cli::runtime as rt;
#[allow(dead_code)]
#[path = "../../components/readlink/src/options.rs"]
mod options;
#[path = "../../components/readlink/src/resolver.rs"]
mod resolver;
use options::Mode;
use resolver::{Namespace, resolve};

// Espace de noms synthétique à racine privée. Aucun IPC ni accès disque.
struct Tree;
#[derive(Debug, PartialEq, Eq)]
enum Error {
    Ordinary,
    Missing,
    Cycle,
    Denied,
    NotDirectory,
}
impl Namespace for Tree {
    type Error = Error;
    fn cwd(&self) -> Result<Vec<u8>, Error> {
        Ok(b"/".to_vec())
    }
    fn link(&self, path: &[u8]) -> Result<Vec<u8>, Error> {
        let p = path.strip_prefix(b"/").unwrap_or(path);
        match p {
            b"file" | b"dir" | b"deny" | b"" => Err(Error::Ordinary),
            b"raw" => Ok(b"./dir//../file".to_vec()),
            b"loop" => Ok(b"loop".to_vec()),
            b"dangling" => Ok(b"absent".to_vec()),
            b"deny/x" => Err(Error::Denied),
            [b'l', index] if index.is_ascii_lowercase() => Ok(if *index == b'a' {
                b"file".to_vec()
            } else {
                alloc::vec![b'l', index - 1]
            }),
            _ => Err(Error::Missing),
        }
    }
    fn search_directory(&self, path: &[u8]) -> Result<(), Error> {
        match path {
            b"/dir" | b"/deny" | b"/" => Ok(()),
            _ => Err(Error::NotDirectory),
        }
    }
    fn ordinary(e: &Error) -> bool {
        *e == Error::Ordinary
    }
    fn missing(e: &Error) -> bool {
        *e == Error::Missing
    }
    fn empty() -> Error {
        Error::Missing
    }
    fn cycle() -> Error {
        Error::Cycle
    }
}
fn log(s: &str) {
    assert_eq!(
        rt::call(rt::DEBUG_WRITE, s.as_ptr() as u64, s.len() as u64, 0).status,
        0
    );
}
#[unsafe(no_mangle)]
extern "C" fn run() -> ! {
    assert!(rt::channels().is_err());
    use Mode::*;
    for (path, mode, expected) in [
        (&b"raw"[..], Raw, Ok(&b"./dir//../file"[..])),
        (b"dangling", Raw, Ok(b"absent")),
        (b"dangling", Existing, Err(Error::Missing)),
        (b"dangling", Last, Ok(b"/absent")),
        (b"raw", Existing, Ok(b"/file")),
        (b"loop", Existing, Err(Error::Cycle)),
        (b"loop", Missing, Ok(b"/loop")),
        (b"/../../file", Existing, Ok(b"/file")),
        (b"deny/x", Existing, Err(Error::Denied)),
        (b"absent/inter", Last, Err(Error::Missing)),
        (b"absent/inter", Missing, Ok(b"/absent/inter")),
        (b"file/..", Existing, Err(Error::NotDirectory)),
        (b"", Missing, Err(Error::Missing)),
    ] {
        assert_eq!(
            resolve(&Tree, path, mode).as_deref().map_err(|e| e),
            expected.as_ref().copied()
        );
    }
    log("READLINK MOTEUR 13 OK");
    for (id, (mode, path)) in [
        (Raw, &b"la"[..]),
        (Existing, b"ld"),
        (Existing, b"lp"),
        (Missing, b"absent/inter"),
    ]
    .into_iter()
    .enumerate()
    {
        // Quatre séries dont trois passages, sans dépasser le quota DEBUG_WRITE.
        for pass in 0..3 {
            for i in 0..4000 {
                let independent = [b'l', b'a' + (i % 16) as u8];
                let name = if id == 0 { &independent[..] } else { path };
                core::hint::black_box(resolve(&Tree, core::hint::black_box(name), mode).unwrap());
            }
            log(&format!("READLINK PERF {id} {pass}"));
        }
    }
    rt::exit(0)
}
