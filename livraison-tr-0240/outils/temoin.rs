#![no_std]
#![no_main]
extern crate alloc;
#[path = "../../components/coreutils/tr/native_stream.rs"]
mod native_stream;
use alloc::{format, vec::Vec};
use core::hint::black_box;
use nova_cli::{
    runtime as rt,
    tr::{Action, prepare},
};

fn debug(s: &str) {
    rt::value(rt::call(
        rt::DEBUG_WRITE,
        s.as_ptr() as u64,
        s.len() as u64,
        0,
    ))
    .unwrap();
}
fn take<'a>(bytes: &mut &'a [u8], len: usize) -> &'a [u8] {
    let (head, rest) = bytes.split_at(len);
    *bytes = rest;
    head
}
fn word(bytes: &mut &[u8]) -> usize {
    u32::from_le_bytes(take(bytes, 4).try_into().unwrap()) as usize
}
fn item<'a>(bytes: &mut &[u8], pool: &[&'a [u8]]) -> &'a [u8] {
    pool[word(bytes)]
}
fn matrix() -> usize {
    let mut b = &include_bytes!(env!("NOVA_TR_VECTORS"))[..];
    let n = word(&mut b);
    let count = word(&mut b);
    let pool: Vec<_> = (0..n)
        .map(|_| {
            let len = word(&mut b);
            take(&mut b, len)
        })
        .collect();
    for case in 1..=count {
        let utf8 = take(&mut b, 1)[0] != 0;
        let kind = take(&mut b, 1)[0];
        let size = word(&mut b);
        let argc = take(&mut b, 1)[0];
        let input = item(&mut b, &pool);
        let output = item(&mut b, &pool);
        let err = item(&mut b, &pool);
        let args: Vec<_> = (0..argc).map(|_| item(&mut b, &pool)).collect();
        let mut stderr = Vec::new();
        let mut stdout = Vec::new();
        let action = prepare("tr", &args, utf8, |m| {
            stderr.extend(format!("tr: {m}\n").as_bytes())
        });
        let got = match action {
            Ok(Action::Help) => 1,
            Ok(Action::Version) => 2,
            Err(msg) => {
                stderr.extend(b"tr: ");
                stderr.extend(msg);
                stderr.push(b'\n');
                3
            }
            Ok(Action::Filter(mut filter)) => {
                for chunk in input.chunks(size) {
                    let mut buf = chunk.to_vec();
                    let (end, eof) =
                        native_stream::read(&mut filter, &mut buf, |_| Ok(chunk.len())).unwrap();
                    assert!(!eof);
                    stdout.extend_from_slice(&buf[..end]);
                }
                0
            }
        };
        if got != kind || stderr != err || stdout != output {
            debug(&format!("TR FAIL id={case} kind={got}/{kind}"));
            rt::exit(1);
        }
    }
    assert!(b.is_empty());
    count
}

fn heap_end() -> u64 {
    rt::value(rt::call(rt::HEAP_RESIZE, 0, 0, 0)).unwrap()
}

fn bench(
    mode: usize,
    case: usize,
    total: usize,
    prefix: usize,
    pattern: &[u8],
    expected: usize,
    cases: usize,
) {
    let args: &[&[u8]] = match mode {
        0 => &[b"a-z", b"A-Z"],
        1 => &[b"-d", b"X"],
        2 => &[b"-s", b"aX"],
        _ => &[b"-ds", b"X", b"a"],
    };
    let Ok(Action::Filter(mut filter)) = prepare("tr", args, false, |_| {}) else {
        rt::exit(2)
    };
    // Les quatre motifs divisent 4096 et le préfixe de 1 Mio est aligné.
    let source: [u8; 4096] = core::array::from_fn(|i| pattern[i % pattern.len()]);
    let mut work = [0u8; 4096];
    let start_heap = heap_end();
    debug(&format!("TR BEGIN mode={mode} case={case} cases={cases}"));
    let mut produced = 0;
    for offset in (0..total).step_by(work.len()) {
        let len = (total - offset).min(work.len());
        let block = &mut work[..len];
        if offset < prefix {
            block.fill(b'a');
        } else {
            block.copy_from_slice(&source[..len]);
        }
        let n = filter.apply(black_box(block));
        black_box(&block[..n]);
        produced += n;
    }
    let end_heap = heap_end();
    assert_eq!(produced, expected);
    assert_eq!(start_heap, end_heap);
    debug(&format!(
        "TR END mode={mode} case={case} input={total} output={produced} heap={end_heap}"
    ));
}

#[unsafe(no_mangle)]
extern "C" fn run() -> ! {
    let cases = matrix();
    let Ok(Action::Filter(mut f)) = prepare("tr", &[b"-ds", b"X", b"a"], false, |_| {}) else {
        rt::exit(2)
    };
    let mut buf = [b'X'; 128];
    assert_eq!(
        native_stream::read(&mut f, &mut buf, |_| Ok(128)),
        Ok((0, false))
    );
    assert_eq!(
        native_stream::read(&mut f, &mut buf, |_| Ok(0)),
        Ok((0, true))
    );
    assert_eq!(
        native_stream::read(&mut f, &mut buf, |_| Err(rt::REVOKED)),
        Err(rt::REVOKED)
    );
    for code in [rt::CLOSED, rt::REVOKED, rt::DENIED] {
        let mut closes = 0;
        assert_eq!(
            native_stream::finish(Err(code), || {
                closes += 1;
                Ok(0)
            }),
            Err(code)
        );
        assert_eq!(closes, 1);
    }
    assert_eq!(
        native_stream::finish(Ok(()), || Err(rt::DENIED)),
        Err(rt::DENIED)
    );
    if env!("NOVA_TR_GROUP") == "2" {
        debug(&format!("TR MATRIX cases={cases}"));
        rt::exit(0);
    }

    let binary: [u8; 256] = core::array::from_fn(|i| i as u8);
    let sizes = [
        (0, &b"aB0Xz9!\n"[..], 1_000_000),
        (0, &b"aaaXXXX\n"[..], 10_000_000),
        (1 << 20, &b"aaXaXXX\n"[..], 65_000_000),
        (0, &binary[..], 131072),
    ];
    let group = env!("NOVA_TR_GROUP").parse::<usize>().unwrap();
    let expected: [[usize; 4]; 4] = include!(env!("NOVA_TR_COUNTS"));
    for mode in group * 2..group * 2 + 2 {
        for (case, &(prefix, pattern, lines)) in sizes.iter().enumerate() {
            bench(
                mode,
                case,
                prefix + pattern.len() * lines,
                prefix,
                pattern,
                expected[case][mode],
                cases,
            );
        }
    }
    rt::exit(0)
}
