use std::{fmt::Write, string::String, vec::Vec};

/// Remplit le document avec les identités et les listes du schéma.
/// Un seul critère PCI correspond à la cible. Les 31 autres visent des plateformes.
pub fn maximal(raw: &str) -> String {
    let publisher = "org.novaos.lab.";
    let mut out = raw.replace(
        "org.novaos.lab.port",
        &(String::from(publisher) + &"p".repeat(96 - publisher.len())),
    );
    let caps: Vec<_> = (0..31)
        .map(|i| std::format!("\"c{i:02}{}\"", "c".repeat(93)))
        .collect();
    out = out.replace("\"stats\"", &caps.join(","));
    let interfaces: String = (0..31)
        .map(|i| std::format!(",\"driver.{i:02}.api/1.0\""))
        .collect();
    out = out.replace(
        "\"ethernet/1.0\"",
        &std::format!("\"ethernet/1.0\"{interfaces}"),
    );
    for i in 0..31 {
        writeln!(
            out,
            "\n[[match]]\nbus='platform'\nprovider='p{i:02}{}'\nfirmware_id='f{i:02}{}'",
            "a".repeat(93),
            "b".repeat(93)
        )
        .unwrap();
    }
    for i in 0..15 {
        writeln!(out,"\n[[dependency]]\nid='option.{i:02}{}'\nkind='service'\nmin=[1,0,0]\nbefore=[2,0,0]\nsha256='{}'\nrequired=false","d".repeat(87), "aa".repeat(32)).unwrap();
    }
    assert_eq!(out.len(), 16381);
    out.push_str("   ");
    out
}
