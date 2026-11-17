#![no_std]
#![no_main]
extern crate alloc;
use alloc::{format, vec::Vec};
use nova_cli::{protocol as fs, runtime as rt};
#[path = "../../components/mktemp/src/exchange.rs"]
mod exchange;
#[allow(dead_code)]
#[path = "../../novaOS/abi/instances.rs"]
mod inst;
#[path = "../../components/mktemp/src/private.rs"]
mod private;
use private::{Private, tmp};

fn trace(s: &str) {
    assert_eq!(
        rt::call(rt::DEBUG_WRITE, s.as_ptr() as u64, s.len() as u64, 0).status,
        0
    );
}
fn invoke(v: &Private, identity: &[u8; 32], args: &[&str], rc: u64) -> Vec<u8> {
    invoke_env(v, identity, args, rc, "fr-CA", "tmp")
}
fn invoke_env(
    v: &Private,
    identity: &[u8; 32],
    args: &[&str],
    rc: u64,
    lang: &str,
    dir: &str,
) -> Vec<u8> {
    let file = v
        .named(
            fs::OPEN_AT,
            b"requete",
            fs::CREATE | fs::READABLE | fs::WRITABLE,
        )
        .unwrap();
    let h = fs::word(&file, 16);
    let mut bytes = b"MKTREQ01".to_vec();
    bytes.extend_from_slice(identity);
    for s in [lang, dir] {
        bytes.extend_from_slice(s.as_bytes());
        bytes.push(0);
    }
    for arg in args {
        bytes.extend_from_slice(arg.as_bytes());
        bytes.push(0);
    }
    exchange::write(v, h, &bytes).unwrap();
    let c = rt::value(rt::call(tmp::WORKER, identity.as_ptr() as u64, h, 32)).unwrap();
    assert_eq!(nova_cli::process::wait(c).unwrap(), (false, rc));
    let out = exchange::read(v, h).unwrap();
    assert!(out.starts_with(b"MKTRES01"));
    let n = u32::from_le_bytes(out[8..12].try_into().unwrap()) as usize;
    let output = out[12..12 + n].to_vec();
    v.send(fs::frame(fs::CLOSE, 1, h)).unwrap();
    v.named(fs::UNLINK_AT, b"requete", 0).unwrap();
    output
}

fn check(v: &Private, out: &[u8], pattern: &str, kind: u8) {
    let name = out.strip_suffix(b"\n").unwrap();
    let (prefix, rest) = pattern.split_once('@').unwrap();
    let (width, suffix) = rest.split_once('@').unwrap();
    let width: usize = width.parse().unwrap();
    assert!(name.starts_with(prefix.as_bytes()) && name.ends_with(suffix.as_bytes()));
    assert_eq!(name.len(), prefix.len() + width + suffix.len());
    assert!(
        name[prefix.len()..prefix.len() + width]
            .iter()
            .all(u8::is_ascii_alphanumeric)
    );
    let path = name.strip_prefix(b"tmp/").unwrap_or(name);
    if kind == 0 {
        assert_eq!(v.named(fs::LOOKUP_AT, path, 0), Err(fs::NOT_FOUND));
        return;
    }
    let st = v.named(fs::LOOKUP_AT, path, 0).unwrap();
    assert_eq!(st[fs::PAYLOAD + 24], kind);
    if kind == 1 {
        let f = v.named(fs::OPEN_AT, path, fs::READABLE).unwrap();
        assert_eq!(fs::word(&f, fs::PAYLOAD + 16), 0);
        v.send(fs::frame(fs::CLOSE, 1, fs::word(&f, 16))).unwrap();
    }
    v.named(if kind == 1 { fs::UNLINK_AT } else { fs::RMDIR }, path, 0)
        .unwrap();
}

#[unsafe(no_mangle)]
extern "C" fn run() -> ! {
    let v = Private(rt::value(rt::call(tmp::CAP, 0, 0, 0)).unwrap());
    let mut id = [0u8; 32];
    rt::value(rt::call(inst::INFO, 2, id.as_mut_ptr() as u64, 32)).unwrap();
    if rt::config(0) == 1 {
        v.named(fs::MKDIR_AT, b"sub", 0).unwrap();
        for dir in [b"env".as_slice(), b"p", b"p/sub"] {
            v.named(fs::MKDIR_AT, dir, 0).unwrap();
        }
        type Case = (
            &'static [&'static str],
            &'static str,
            &'static str,
            &'static str,
            &'static str,
            u64,
            u8,
        );
        let cases: &[Case] = include!(env!("MKTEMP_VECTEURS"));
        for (i, &(argv, lang, tmpdir, expected, mode, rc, kind)) in cases.iter().enumerate() {
            let result = invoke_env(&v, &id, argv, rc, lang, tmpdir);
            match mode {
                "nom" => check(&v, &result, expected, kind),
                "egal" => assert_eq!(result, expected.as_bytes(), "cas {i}"),
                "prefixe" => assert!(result.starts_with(expected.as_bytes()), "cas {i}"),
                _ => panic!(),
            }
        }
        trace(&format!("MKTEMP MATRICE {} OK", cases.len()));
        let mut long_path = alloc::string::String::from("sub/");
        // Chaque composant de ce chemin reste dans les limites de P9.
        for _ in 0..9 {
            long_path.push_str("niveauabc/");
            v.named(fs::MKDIR_AT, long_path.trim_end_matches('/').as_bytes(), 0)
                .unwrap();
        }
        let raw = alloc::format!("{long_path}XXX");
        check(
            &v,
            &invoke(&v, &id, &[&raw], 0),
            &format!("{long_path}@3@"),
            1,
        );
        assert!(!invoke(&v, &id, &["../aXXX"], 1).is_empty());
    }
    for pass in 1..=3 {
        let mut names = Vec::new();
        trace(&format!("MKTEMP DEBUT {pass}"));
        for i in 0..8 {
            let args: &[&str] = if i < 4 {
                &["-p", "tmp", "arc.XXX"]
            } else {
                &["-d", "-p", "tmp", "arc.XXX"]
            };
            let out = invoke(&v, &id, args, 0);
            assert!(!names.contains(&out));
            names.push(out);
        }
        trace(&format!("MKTEMP FIN {pass}"));
        for (i, out) in names.iter().enumerate() {
            check(&v, out, "tmp/arc.@3@", if i < 4 { 1 } else { 2 });
        }
    }
    trace("MKTEMP RELECTURE OK");
    rt::exit(0)
}
