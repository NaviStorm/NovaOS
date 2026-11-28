use sha2::{Digest, Sha256};
use std::{env, fs, path::PathBuf};
// Compiler le vérificateur 5M historique, y compris la vérification de l'ancre
// LoadOptions et sa consommation unique. Seule la suite de examiner diffère.
fn main() {
    let source = "../diagnostic-5m/entree.rs";
    println!("cargo:rerun-if-changed={source}");
    let s = fs::read_to_string(source).unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(s.as_bytes())),
        "c8c8522ff4c094da8b66980b843023384ef49f9e5640cd5f11c614fd26c5c074",
        "source 5M différente"
    );
    let start = s.find("fn donnees(").unwrap();
    let end = s
        .find("    let cfg = unsafe { ptr::read_volatile(&CONFIG) };")
        .unwrap();
    let mut code = s[start..end].to_owned();
    assert!(code.ends_with("    r.confiance = rec[16].into();\n"));
    code.push_str("diagnostiquer(st,&carte,img,ram,off,pe,r)\n}\n");
    fs::write(
        PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("reception5m.rs"),
        code,
    )
    .unwrap();
}
