use super::*;
const RAW: &str = include_str!("../../vecteurs/pilote.toml");
use alloc::{format, vec, vec::Vec};

#[test]
fn grammaire_et_limites_contre_serde_4b() {
    let mut cases = vec![RAW.into()];
    for (from, variants) in [
        (
            "security_epoch=3",
            &[
                "security_epoch=+3",
                "security_epoch=0x3",
                "security_epoch=0b0011",
                "security_epoch=0_3",
                "security_epoch=2.5",
                "security_epoch=1979-05-27",
                "security_epoch=[3]",
                "security_epoch=-0",
            ][..],
        ),
        (
            "bus=\"pci\"",
            &["bus='futur'", "bus='pci'\nextra=true", "bus='usb'"],
        ),
        (
            "class=\"ethernet\"",
            &[
                "class=\"ether\\u006eet\"",
                "class='''ethernet'''",
                "class=\"ethernet\"\nclass='x'",
                "class={a={b={c=3}}}",
                "class='ethernet' # commentaire",
            ],
        ),
        (
            "version=[1,0,0]",
            &[
                "version=[1,\n 0, 0,]",
                "version=[1_0,0,0]",
                "version=[0x1,0o0,0b0]",
                "version=[1 0,0]",
                "version=[-0,0,0]",
            ],
        ),
        (
            "device=0x1041",
            &[
                "device=65536",
                "device=-0",
                "device='4161'",
                "device=0x10_41",
                "device=0x1041\n[match.future]\na=0",
                "device.value=0x1041",
            ],
        ),
        (
            "[quotas]",
            &["['quotas']", "[quotas]\n[quotas]", "[quotas.extra]"],
        ),
        (
            "[\"stats\"]",
            &[
                "[true]",
                "[[]]",
                "[\"stats\", 'stats']",
                "[ 'cap1', 'cap2', ]",
            ],
        ),
    ] {
        for to in variants {
            cases.push(RAW.replace(from, to));
        }
    }
    let (head, quotas) = RAW.split_once("[quotas]\n").unwrap();
    let dotted = quotas
        .lines()
        .map(|l| format!("quotas.{l}\n"))
        .collect::<alloc::string::String>();
    cases.push(format!("{dotted}{head}"));
    cases.push(format!(
        "quotas={{{}}}\n{head}",
        quotas.lines().collect::<Vec<_>>().join(",")
    ));
    cases.push(RAW.replace(
        "org.novaos.lab.port",
        &format!("org.novaos.lab.{}", "x".repeat(82)),
    ));
    cases.push(RAW.replace("\"stats\"", &vec!["\"cap\""; 33].join(",")));
    for extra in [
        "18446744073709551616",
        "18446744073709551615",
        "-9223372036854775809",
        "1970-99-99",
        "1970-03-01",
        "1e1000",
        "true",
        "{x=[1,2]}",
    ] {
        cases.push(RAW.replace("bus=\"pci\"", &format!("bus='future'\nextra={extra}")));
    }
    let mut failures = Vec::new();
    for (i, raw) in cases.iter().enumerate() {
        let old = toml::from_str::<Manifest>(raw)
            .ok()
            .and_then(|m| m.validate().ok())
            .is_some();
        let new = decode::<Manifest>(raw.as_bytes(), MAX_DECODE_HEAP)
            .and_then(|m| m.validate())
            .is_ok();
        if new != old {
            failures.push((
                i,
                new,
                old,
                raw.lines().find(|l| l.starts_with("extra=")).unwrap_or(""),
            ));
        }
    }
    assert!(failures.is_empty(), "{failures:?}");
    // Le schéma 4B exige exactement trois u32. L’ancien adaptateur Serde
    // ignorait une quatrième composante après avoir rempli le tableau Rust.
    let long = RAW.replace("version=[1,0,0]", "version=[1,0,0,0]");
    assert!(toml::from_str::<Manifest>(&long).is_ok());
    assert!(decode::<Manifest>(long.as_bytes(), MAX_DECODE_HEAP).is_err());
    let too_deep = RAW.replace(
        "\"stats\"",
        &format!("{}0{}", "[".repeat(60), "]".repeat(60)),
    );
    assert!(decode::<Manifest>(too_deep.as_bytes(), MAX_DECODE_HEAP).is_err());
}
