//! Vecteurs sans appel de métadonnées, avec un adaptateur de valeurs fictives.
extern crate alloc;
#[path = "../components/stat/src/format.rs"]
mod format;
#[path = "../components/stat/src/options.rs"]
mod options;
use format::{Value, render};

#[test]
fn formats_octets_et_numeriques() {
    let mut out = Vec::new();
    render(
        br"%Qn\0%#06a|%04s|%.Y|%013.6Y|%Hd|%%",
        true,
        false,
        |k| match k {
            b"Qn" => Value::text("'deux mots'"),
            b"a" => Value::Number(0o644, 8),
            b"s" => Value::dec(3),
            b"Y" => Value::Epoch(67413, 23456789),
            b"Hd" => Value::dec(1),
            _ => panic!(),
        },
        &mut out,
        |e| panic!("{e}"),
    )
    .unwrap();
    assert_eq!(
        out,
        b"'deux mots'\x00000644|0003|67413.023456789|067413.023456|1|%"
    );
}

#[test]
fn premieres_erreurs_et_avertissements() {
    let mut out = Vec::new();
    let mut warnings = Vec::new();
    let err = render(
        br"avant\q%9%",
        true,
        false,
        |_| unreachable!(),
        &mut out,
        |s| warnings.push(s),
    );
    assert_eq!(out, b"avantq");
    assert_eq!(warnings, ["warning: unrecognized escape '\\q'"]);
    assert_eq!(err.unwrap_err(), "'%9%': invalid directive");
}

#[test]
fn volume_et_champ_inconnu() {
    let mut out = Vec::new();
    render(
        b"%Hd %t %T",
        false,
        true,
        |k| match k {
            b"t" => Value::hex(0x1a),
            b"T" => Value::text("volume"),
            _ => Value::Unknown,
        },
        &mut out,
        |s| panic!("{s}"),
    )
    .unwrap();
    assert_eq!(out, b"?d 1a volume\n");
}

#[test]
fn selection_et_precedence() {
    let args: &[&[u8]] = &[
        b"chemin",
        b"-Lft",
        b"--printf=x",
        b"--for",
        b"%Qn",
        b"--",
        b"--help",
    ];
    let o = options::parse(args, false).unwrap();
    assert!(o.follow && o.fs && o.terse && !o.escapes && o.doc.is_none());
    assert_eq!(o.files, [b"chemin".as_slice(), b"--help"]);
    assert_eq!(format::template(&o, false), b"%Qn");
    let o = options::parse(args, true).unwrap();
    assert_eq!(o.files, args);
    assert!(options::parse(&[b"--cached="], false).is_err());
    assert_eq!(options::parse(&[b"--ver"], false).unwrap().doc, Some(false));
}

#[test]
fn secondes_negatives_et_lien() {
    let mut out = Vec::new();
    render(
        b"%Y %.3Y %5.2N",
        false,
        false,
        |k| match k {
            b"Y" => Value::Epoch(-1, 123456789),
            b"N" => Value::Link(b"lien".to_vec(), b"cible".to_vec()),
            _ => unreachable!(),
        },
        &mut out,
        |s| panic!("{s}"),
    )
    .unwrap();
    assert_eq!(out, b"-1 -0.876    li ->    ci\n");
}
