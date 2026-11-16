#![no_std]
#![no_main]
extern crate alloc;
use alloc::{format, vec::Vec};
use nova_cli::{
    documentation::{Language, lookup},
    runtime as rt,
};
#[path = "../../components/dirname/src/options.rs"]
mod options;
#[path = "../../components/dirname/src/path.rs"]
mod path;
#[path = "../../components/dirname/src/quote.rs"]
mod quote;

fn log(text: &str) {
    assert_eq!(
        rt::call(rt::DEBUG_WRITE, text.as_ptr() as u64, text.len() as u64, 0).status,
        0
    );
}

fn number(data: &mut &[u8]) -> usize {
    let (raw, rest) = data.split_at(4);
    *data = rest;
    u32::from_le_bytes(raw.try_into().unwrap()) as usize
}

fn blob<'a>(data: &mut &'a [u8]) -> &'a [u8] {
    let len = number(data);
    let (head, rest) = data.split_at(len);
    *data = rest;
    head
}

fn vectors() {
    let mut bytes = include_bytes!(env!("DIRNAME_VECTEURS")).as_slice();
    let count = number(&mut bytes);
    for index in 0..count {
        let argc = number(&mut bytes);
        let args: Vec<_> = (0..argc).map(|_| blob(&mut bytes)).collect();
        let lang = Language::from_tag(core::str::from_utf8(blob(&mut bytes)).ok());
        let posix = number(&mut bytes) != 0;
        let tty = number(&mut bytes) != 0;
        let utf8 = number(&mut bytes) != 0;
        let style = core::str::from_utf8(blob(&mut bytes)).unwrap();
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let mut rc = 0;
        match options::parse(&args, posix) {
            Ok(options::Action::Paths { names, zero }) => {
                for name in names {
                    let value = path::parent(name);
                    if tty && !zero {
                        out.extend(quote::quoted(value, style, utf8, |c| !c.is_control()));
                    } else {
                        out.extend_from_slice(value);
                    }
                    out.push(if zero { 0 } else { b'\n' });
                }
            }
            Ok(options::Action::Help) => {
                out = lookup("dirname")
                    .unwrap()
                    .render(&[], false, lang)
                    .unwrap()
                    .into_bytes()
            }
            Ok(options::Action::Version) => {
                out = lookup("dirname").unwrap().version(lang).into_bytes()
            }
            Err(reason) => {
                rc = 1;
                err.extend_from_slice(b"dirname: ");
                err.extend(reason);
                err.extend_from_slice(b"\nTry 'dirname --help' for more information.\n");
            }
        }
        let expected = number(&mut bytes);
        let stdout = blob(&mut bytes);
        let stderr = blob(&mut bytes);
        if (rc, out.as_slice(), err.as_slice()) != (expected, stdout, stderr) {
            log(&format!("DIRNAME ECART {index}"));
            rt::exit(1);
        }
    }
    assert!(bytes.is_empty());
    log(&format!("DIRNAME MATRICE {count} OK"));
}

fn bench(count: usize, len: usize, repeats: usize) {
    let names: Vec<Vec<u8>> = (0..count)
        .map(|i| {
            let mut v = alloc::vec![b'a'+(i%26) as u8;len];
            v[len - 2..].copy_from_slice(b"/f");
            v
        })
        .collect();
    let args: Vec<_> = names.iter().map(Vec::as_slice).collect();
    assert!(count <= 32 && 1 + count * (len + 2) <= 2560);
    let mut out = Vec::with_capacity(count * len);
    let mut process = || {
        let Ok(options::Action::Paths { names, .. }) =
            options::parse(core::hint::black_box(&args), false)
        else {
            rt::exit(2)
        };
        out.clear();
        for name in names {
            out.extend_from_slice(path::parent(name));
            out.push(b'\n');
        }
        core::hint::black_box(&out);
    };
    // L’ABI accorde seize messages de diagnostic à ce processus.
    log(&format!("DIRNAME DEBUT {count} {len} {repeats} 0"));
    for pass in 0..4 {
        for _ in 0..repeats {
            process();
        }
        log(&format!("DIRNAME FIN {count} {pass}"));
    }
    let expected: Vec<_> = args
        .iter()
        .flat_map(|p| p[..len - 2].iter().copied().chain([b'\n']))
        .collect();
    assert_eq!(out, expected);
}

#[unsafe(no_mangle)]
extern "C" fn run() -> ! {
    assert!(rt::channels().is_err() && rt::config(0) == 0);
    vectors();
    bench(1, 8, 16000);
    bench(32, 64, 4000);
    bench(4, 512, 4000);
    rt::exit(0)
}
