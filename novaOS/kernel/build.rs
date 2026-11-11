use std::{
    env,
    path::{Path, PathBuf},
    process::Command,
};

fn main() {
    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let script = manifest.join("linker.ld");
    println!("cargo::rerun-if-changed=linker.ld");
    println!("cargo::rustc-link-arg-bin=novaos=-T{}", script.display());

    let user = Path::new("../user");
    println!("cargo::rerun-if-changed=../user");
    println!("cargo::rerun-if-changed=../abi");
    let target = env::var("TARGET").unwrap();
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    for name in [
        "demo",
        "ipc",
        "storage/service",
        "storage/client",
        "storage/failure",
    ] {
        let output = out.join(format!("{}.bin", name.replace('/', "-")));
        let linker = format!("link-arg=-T{}", user.join("demo.ld").display());
        let ok = Command::new(env::var_os("RUSTC").unwrap())
            .arg(user.join(format!("{name}.rs")))
            .args([
                "--edition=2024",
                "--target",
                &target,
                "-C",
                "panic=abort",
                "-C",
                "opt-level=s",
                "-C",
                "relocation-model=static",
                "-C",
                "link-arg=--oformat=binary",
                "-C",
                &linker,
                "-o",
            ])
            .arg(output)
            .status()
            .expect("compilation du programme utilisateur");
        assert!(ok.success(), "édition de liens de {name}");
    }
}
