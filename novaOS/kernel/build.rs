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
        "storage/relay",
        "storage/disk",
        "storage/requester",
        "storage/control",
        "storage/fileclient",
    ] {
        let output = out.join(format!("{}.bin", name.replace('/', "-")));
        let file_client = name == "storage/fileclient";
        let linker = format!(
            "link-arg=-T{}",
            user.join(if file_client {
                "novafs/service.ld"
            } else {
                "demo.ld"
            })
            .display()
        );
        let mut rust = Command::new(env::var_os("RUSTC").unwrap());
        let compact = matches!(
            name,
            "storage/relay" | "storage/disk" | "storage/requester" | "storage/control"
        );
        if compact {
            rust.args([
                "-C",
                "lto=fat",
                "-C",
                "codegen-units=1",
                "-C",
                "strip=symbols",
            ]);
        }
        if file_client {
            rust.args(["-C", "link-arg=-zmax-page-size=4096"]);
        } else {
            rust.args(["-C", "link-arg=--oformat=binary"]);
        }
        let ok = rust
            .arg(user.join(format!("{name}.rs")))
            .args([
                "--edition=2024",
                "--target",
                &target,
                "-C",
                "panic=abort",
                "-C",
                if compact {
                    "opt-level=z"
                } else {
                    "opt-level=s"
                },
                "-C",
                "relocation-model=static",
                "-C",
                &linker,
                "-o",
            ])
            .arg(output)
            .status()
            .expect("compilation du programme utilisateur");
        assert!(ok.success(), "édition de liens de {name}");
    }
    println!("cargo::rerun-if-changed=../../novaFS/src");
    println!("cargo::rerun-if-changed=../../novaFS/Cargo.toml");
    let service = user.join("novafs");
    let linker = service.join("service.ld").canonicalize().unwrap();
    let work = out.join("service-target");
    // Workspace et target-dir séparés pour que les dépendances invitées ne soient
    // jamais liées à l'ELF superviseur, même en compilation incrémentale.
    let flags = [
        "-Crelocation-model=static",
        "-Cno-redzone=yes",
        "--cfg",
        "sha2_backend=\"soft\"",
        "--cfg",
        "chacha20_backend=\"soft\"",
        "--cfg",
        "poly1305_backend=\"soft\"",
        "-Clink-arg=-zmax-page-size=4096",
        &format!("-Clink-arg=-T{}", linker.display()),
    ]
    .join("\x1f");
    let mut cargo_service = Command::new(env::var_os("CARGO").unwrap());
    cargo_service.args(["build", "--release", "--locked", "--target", &target]);
    if env::var_os("CARGO_FEATURE_INSTALLATION_GPT").is_some() {
        cargo_service.args(["--features", "installation"]);
    }
    let result = cargo_service
        .current_dir(service)
        .env("CARGO_TARGET_DIR", &work)
        .env("CARGO_ENCODED_RUSTFLAGS", &flags)
        .status()
        .expect("compilation du service NovaFS");
    assert!(result.success(), "liaison du service NovaFS");
    std::fs::copy(
        work.join(&target).join("release/novaos-storage"),
        out.join("storage-media.bin"),
    )
    .unwrap();

    println!("cargo::rerun-if-env-changed=NOVA_HISTORY_ENTRIES");
    println!("cargo::rerun-if-env-changed=NOVA_MORE_LINES");
    println!("cargo::rerun-if-env-changed=NOVA_MORE_COLUMNS");
    let components = manifest.join("../../components");
    for path in [
        "Cargo.toml",
        "Cargo.lock",
        "libraries",
        "coreutils",
        "shell",
    ] {
        println!(
            "cargo::rerun-if-changed={}",
            components.join(path).display()
        );
    }
    let result = Command::new(env::var_os("CARGO").unwrap())
        .args(["build", "--release", "--locked", "--target", &target])
        .current_dir(&components)
        .env("CARGO_TARGET_DIR", &work)
        .env("CARGO_ENCODED_RUSTFLAGS", &flags)
        .status()
        .expect("compilation des ELF CLI");
    assert!(result.success(), "liaison des composants CLI");
    let mut catalog = String::from("&[");
    for name in [
        "pwd", "ls", "cat", "head", "mkdir", "rm", "probe", "cp", "mv", "touch", "more", "wc",
        "tail", "rmdir", "nsh",
    ] {
        let path = out.join(format!("{name}.elf"));
        std::fs::copy(work.join(&target).join("release").join(name), &path).unwrap();
        if name != "nsh" {
            catalog.push_str(&format!("(include_bytes!({path:?}),{}),", name == "probe"));
        } else {
            // Nom historique de l’image privée du lanceur, même ELF nsh.
            std::fs::copy(&path, out.join("shell.elf")).unwrap();
        }
    }
    catalog.push(']');
    std::fs::write(out.join("cli_catalog.rs"), catalog).unwrap();
}
