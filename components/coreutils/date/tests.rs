use super::{engine, format::Fields};
use jiff::{Timestamp, tz::TimeZone};

#[test]
fn fractions_avant_epoque() {
    for (input, expected) in [
        ("@-0.0000000001", "-1.999999999"),
        ("@-1.25", "-2.750000000"),
        ("@0", "0.000000000"),
    ] {
        let parsed = engine::parse(input, None, &TimeZone::UTC).unwrap();
        let fields = Fields::new(&parsed, &TimeZone::UTC).unwrap();
        assert_eq!(fields.render("%s.%N", true).unwrap(), expected);
    }
}

#[test]
fn changement_heure_et_zone_dans_operande() {
    let now = Timestamp::UNIX_EPOCH.to_zoned(TimeZone::UTC);
    let parse = |input| engine::parse(input, Some(&now), &TimeZone::UTC);
    assert!(parse("TZ=\"Europe/Paris\" 2024-03-31 02:30:00").is_none());
    assert_eq!(
        parse("TZ=\"Europe/Paris\" 2024-03-31 03:30:00")
            .unwrap()
            .unix_epoch_second(),
        1711848600
    );
}

#[test]
fn annees_bissextiles_et_borne_de_format() {
    let now = Timestamp::UNIX_EPOCH.to_zoned(TimeZone::UTC);
    assert!(engine::parse("2023-02-29", Some(&now), &TimeZone::UTC).is_none());
    let parsed = engine::parse("2024-02-29", Some(&now), &TimeZone::UTC).unwrap();
    let fields = Fields::new(&parsed, &TimeZone::UTC).unwrap();
    assert_eq!(fields.render("%F", true).unwrap(), "2024-02-29");
    assert!(fields.render("%1048577Y", true).is_err());
}
