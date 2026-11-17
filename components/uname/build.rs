use std::{env, fs};

fn main() {
    // Même manifeste que la bannière du noyau de cette livraison.
    let path = "../../novaOS/kernel/Cargo.toml";
    println!("cargo:rerun-if-changed={path}");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("none") {
        let manifest = fs::read_to_string(path).expect("manifeste du noyau");
        let package = manifest
            .split("[package]")
            .nth(1)
            .unwrap()
            .split("\n[")
            .next()
            .unwrap();
        let version = package
            .lines()
            .find_map(|line| line.strip_prefix("version = \"")?.strip_suffix('"'))
            .unwrap();
        println!("cargo:rustc-env=NOVA_KERNEL_RELEASE={version}");
    }
}
