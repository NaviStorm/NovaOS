use std::{fmt::Write, string::String, vec::Vec};

/// 96 octets d'identité, 32 droits, 32 correspondances, 16 dépendances.
pub fn maximal(raw: &str) -> String {
    let id = "org.novaos.lab.";
    let mut out = raw.replace(
        "org.novaos.lab.port",
        &(String::from(id) + &"p".repeat(96 - id.len())),
    );
    let caps: Vec<_> = (0..31)
        .map(|i| std::format!("\"c{i:02}{}\"", "c".repeat(93)))
        .collect();
    out = out.replace("\"stats\"", &caps.join(","));
    for d in 0..31 {
        writeln!(out, "\n[[match]]\nbus='pci'\nvendor=0x1af4\ndevice={d}").unwrap();
    }
    for i in 0..15 {
        writeln!(out,"\n[[dependency]]\nid='option.{i}'\nkind='service'\nmin=[1,0,0]\nbefore=[2,0,0]\nsha256='{}'\nrequired=false","aa".repeat(32)).unwrap();
    }
    assert!(out.len() <= 16384);
    out.push_str(&" ".repeat(16384 - out.len()));
    out
}
