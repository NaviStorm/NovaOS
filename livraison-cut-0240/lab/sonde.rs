#![no_std]
#![no_main]
extern crate alloc;
#[path = "../../components/cut/src/args.rs"]
mod args;
#[path = "../../components/cut/src/engine.rs"]
mod engine;
use alloc::{format, vec::Vec};
use nova_cli::runtime as rt;

fn log(msg: &str) {
    let _ = rt::call(rt::DEBUG_WRITE, msg.as_ptr() as u64, msg.len() as u64, 0);
}
fn number(data: &mut &[u8]) -> usize {
    let (a, b) = data.split_at(4);
    *data = b;
    u32::from_le_bytes(a.try_into().unwrap()) as usize
}
fn blob<'a>(data: &mut &'a [u8]) -> &'a [u8] {
    let n = number(data);
    let (a, b) = data.split_at(n);
    *data = b;
    a
}
fn vectors() {
    let mut data = include_bytes!(env!("CUT_VECTORS")).as_slice();
    let mut cnt = 0;
    while !data.is_empty() {
        let id = number(&mut data);
        let utf8 = number(&mut data) != 0;
        let argc = number(&mut data);
        let argv: Vec<_> = (0..argc).map(|_| blob(&mut data)).collect();
        let repeats = number(&mut data);
        let tail = blob(&mut data);
        let rc = number(&mut data);
        let expected = blob(&mut data);
        let ok = match args::parse(&argv, utf8, false) {
            Ok(args::Action::Run(c, files)) if files.as_slice() == [b"-"] && rc == 0 => {
                let mut stream = engine::Stream::new(&c);
                let mut output = Vec::new();
                let pad = [b'x'; 1024];
                let mut remaining = repeats;
                while remaining > 0 {
                    let n = remaining.min(pad.len());
                    stream.feed(&pad[..n], &mut output);
                    remaining -= n
                }
                for chunk in tail.chunks(1024) {
                    stream.feed(chunk, &mut output)
                }
                stream.finish(&mut output);
                output == expected
            }
            Err(error) => rc != 0 && error.as_bytes() == expected,
            _ => false,
        };
        if !ok {
            log(&format!("CUT ECART {id}"));
            rt::exit(1)
        }
        cnt += 1;
    }
    log(&format!("CUT MATRICE {cnt} OK"));
}
fn bench(id: usize, argv: &[&[u8]], data: &[u8], expected: &[u8]) {
    let Ok(args::Action::Run(cfg, _)) = args::parse(argv, true, false) else {
        rt::exit(2)
    };
    let source = data.repeat(512);
    let oracle = expected.repeat(512);
    let mut result = Vec::new();
    let mut stream = engine::Stream::new(&cfg);
    stream.feed(&source, &mut result);
    assert_eq!(result, oracle);
    let bytes = if option_env!("CUT_LARGE") == Some("1") {
        if id == 0 { 272 << 20 } else { 16 << 20 }
    } else {
        1 << 20
    };
    let repeats: usize = bytes / source.len() + 1;
    log(&format!("CUT DEBUT {id} 0 {}", repeats * source.len()));
    for rep in 0..4 {
        for _ in 0..repeats {
            result.clear();
            stream.feed(core::hint::black_box(&source), &mut result);
            core::hint::black_box(&result);
        }
        log(&format!("CUT FIN {id} {rep}"));
    }
    result.clear();
    stream.finish(&mut result);
    assert!(result.is_empty())
}
#[unsafe(no_mangle)]
extern "C" fn run() -> ! {
    vectors();
    let row = b"2\t10\t2\t10\t2\t10\t77\t2\t10\n";
    bench(0, &[b"-f1,3,8,9"], row, b"2\t2\t2\t10\n");
    bench(
        1,
        &[b"--complement", b"-f2,7"],
        row,
        b"2\t2\t10\t2\t10\t2\t10\n",
    );
    bench(
        2,
        &[b"-c1,3,5"],
        "é2界10😀\n".as_bytes(),
        "é界0\n".as_bytes(),
    );
    rt::exit(0)
}
