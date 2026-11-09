use std::{env, path::Path};

fn main() {
    let manifest = env::var_os("CARGO_MANIFEST_DIR").unwrap();
    let script = Path::new(&manifest).join("linker.ld");
    println!("cargo::rerun-if-changed=linker.ld");
    println!("cargo::rustc-link-arg-bin=novaos=-T{}", script.display());
}
