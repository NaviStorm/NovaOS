// Oracles exacts en nanosecondes, sans écoulement du temps civil.
extern crate alloc;
#[path = "../../components/coreutils/sleep/engine.rs"]
mod engine;
#[path = "../../components/coreutils/sleep/number.rs"]
mod number;
#[path = "../../components/coreutils/sleep/options.rs"]
mod options;
#[path = "../../components/coreutils/sleep/quote.rs"]
mod quote;

fn nanos(args: &[&[u8]], point: &str) -> Option<u128> {
    match engine::parse(args, point, false, false, b"sleep").unwrap() {
        engine::Request::Wait(seconds) => engine::nanos(seconds),
        _ => panic!("durée attendue"),
    }
}

#[test]
fn bornes_exactes_et_somme() {
    for (args, minimum) in [
        (vec![b"0".as_slice()], 0u128),
        (vec![b".0000000001"], 1),
        (vec![b"0x.002p1"], 976_563),
        (vec![b"0x0.01d"], 7_080_079),
        (vec![b"0.1m", b".002h", b"0.001d"], 99_600_000_000),
        (vec![b"1234e-3", b"567.89e-6"], 1_234_567_890),
        (vec![b"1e-9999"], 1),
        (vec![b"0x1p-1075"], 1),
    ] {
        let result = nanos(&args, ".").unwrap();
        assert!(
            (minimum..=minimum + 1).contains(&result),
            "{args:?} {result}"
        );
    }
    assert_eq!(nanos(&[b".001"], ","), nanos(&[b",001"], ","));
    assert_eq!(nanos(&[b"--", b"-0", b"0"], "."), Some(0));
}

#[test]
fn grand_intervalle_sans_attendre() {
    let ns = nanos(&[b"365d", b"1h"], ".").unwrap();
    let requested = 31_539_600_000_000_000u128;
    assert!((requested..=requested + 32).contains(&ns));
    for arg in [
        b"inf".as_slice(),
        b"+Infinityd",
        b"1e300",
        b"0x1p1024",
        b"9223372036854775808",
    ] {
        assert_eq!(nanos(&[arg], "."), None, "{arg:?}");
    }
    for arg in [
        b"-1e-999".as_slice(),
        b"-0x1p-99999",
        b"nan",
        b"1e+",
        b"0xp1",
    ] {
        assert!(engine::parse(&[b"--", arg], ".", false, false, b"sleep").is_err());
    }
}
