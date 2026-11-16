use std::{env, fs, path::Path};
fn main() {
    let mut cc = cc::Build::new();
    cc.compiler("clang")
        .include("qse/include")
        .file("engine.c")
        .define("QSE_HAVE_CONFIG_H", None)
        .define("NDEBUG", None)
        .flag("-std=gnu11")
        .flag("-ffreestanding")
        .flag("-fno-strict-aliasing")
        .flag("-ffunction-sections")
        .flag("-fdata-sections");
    if env::var("CARGO_CFG_TARGET_OS").unwrap() == "none" {
        cc.flag("-mgeneral-regs-only").flag("-fno-stack-protector");
        if env::var("CARGO_CFG_TARGET_ARCH").unwrap() == "x86_64" {
            cc.flag("-mno-red-zone");
        }
    }
    for dir in ["qse/lib/sed", "qse/lib/cmn"] {
        let mut files: Vec<_> = fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.extension() == Some("c".as_ref()))
            .collect();
        files.sort();
        cc.files(files);
    }
    cc.warnings(false).compile("sed_engine");
    for file in ["qse", "engine.c"] {
        println!("cargo:rerun-if-changed={}", Path::new(file).display());
    }
}
