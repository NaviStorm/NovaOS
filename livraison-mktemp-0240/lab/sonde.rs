#![no_std]
#![no_main]
extern crate alloc;
use alloc::format;
use nova_cli::{protocol as fs, runtime as rt};
#[allow(dead_code)]
#[path = "../../novaOS/abi/instances.rs"]
mod inst;
#[path = "../../components/mktemp/src/options.rs"]
mod options;
#[path = "../../components/mktemp/src/private.rs"]
mod private;
#[path = "../../components/mktemp/src/quote.rs"]
mod quote;
#[path = "../../components/mktemp/src/template.rs"]
mod template;
use private::{Private, tmp};

fn log(s: &str) {
    assert_eq!(
        rt::call(rt::DEBUG_WRITE, s.as_ptr() as u64, s.len() as u64, 0).status,
        0
    );
}
fn rights() -> Private {
    Private(rt::value(rt::call(tmp::CAP, 0, 0, 0)).unwrap())
}
fn name(pass: u64, index: u8, dir: bool) -> alloc::string::String {
    format!("arc{pass}-{}-{index:03}", if dir { "dir" } else { "file" })
}
fn work(pass: u64) {
    let view = rights();
    for dir in [false, true] {
        let raw = format!("arc{pass}-{}-XXX", if dir { "dir" } else { "file" });
        let flag = if dir {
            b"--directory".as_slice()
        } else {
            b"--quiet"
        };
        let argv = [flag, b"--tmpdir", raw.as_bytes()];
        let Ok(options::Action::Create(opt)) = options::parse(&argv, false) else {
            panic!()
        };
        let t = template::prepare(&opt, b"", b"tmp").unwrap();
        for index in 1..=4u8 {
            let mut draws = 0;
            let reserved = template::reserve(
                &t,
                |out| {
                    let value = if draws == 0 { 0 } else { index };
                    draws += 1;
                    out.copy_from_slice(format!("{value:03}").as_bytes());
                },
                |n| view.claim(n, opt.directory, opt.dry),
            )
            .unwrap()
            .unwrap();
            assert_eq!(draws, 2);
            assert_eq!(
                reserved,
                format!("tmp/{}", name(pass, index, dir)).as_bytes()
            );
        }
        assert_eq!(view.claim(b"tmp/arc-unused", dir, true), Ok(true));
        assert_eq!(
            view.named(fs::LOOKUP_AT, b"arc-unused", 0),
            Err(fs::NOT_FOUND)
        );
    }
}

fn parent() {
    let view = rights();
    let mut identity = [0u8; 32];
    rt::value(rt::call(inst::INFO, 2, identity.as_mut_ptr() as u64, 32)).unwrap();
    for pass in 1..=3 {
        for dir in [false, true] {
            let n = name(pass, 0, dir);
            assert_eq!(view.claim(n.as_bytes(), dir, false), Ok(true));
        }
        log(&format!("MKTEMP DEBUT {pass}"));
        let child = rt::value(rt::call(
            tmp::WORKER,
            identity.as_ptr() as u64,
            100 + pass,
            32,
        ))
        .unwrap();
        let (fault, code) = nova_cli::process::wait(child).unwrap();
        assert!(!fault && code == 0);
        log(&format!("MKTEMP FIN {pass}"));
        for dir in [false, true] {
            for index in 0..=4 {
                let n = name(pass, index, dir);
                let stat = view.named(fs::LOOKUP_AT, n.as_bytes(), 0).unwrap();
                assert_eq!(stat[fs::PAYLOAD + 24], if dir { 2 } else { 1 });
                if !dir {
                    let f = view.named(fs::OPEN_AT, n.as_bytes(), fs::READABLE).unwrap();
                    view.send(fs::frame(fs::CLOSE, 1, fs::word(&f, 16)))
                        .unwrap();
                }
                view.named(if dir { fs::RMDIR } else { fs::UNLINK_AT }, n.as_bytes(), 0)
                    .unwrap();
            }
        }
    }
    log("MKTEMP RELECTURE OK");
}
#[unsafe(no_mangle)]
extern "C" fn run() -> ! {
    let mode = rt::config(0);
    if (101..=103).contains(&mode) {
        work(mode - 100)
    } else {
        parent()
    }
    rt::exit(0)
}
