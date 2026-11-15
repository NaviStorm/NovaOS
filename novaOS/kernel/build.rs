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
    let nvme_lab = target == "x86_64-unknown-none" && env::var_os("CARGO_FEATURE_NVME_LAB").is_some();
    for name in [
        "demo",
        "devices",
        "edu",
        "edu_dma",
        "nvme_driver",
        "nvme_rw_driver",
        "nvme_rw_block",
        "nvme_client",
        "nvme_manager",
        "nvme_block",
        "fp_probe",
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
        let nvme_rw = matches!(name, "nvme_rw_driver" | "nvme_rw_block");
        let output = out.join(format!("{}.bin", name.replace('/', "-")));
        if name == "edu_dma" && (env::var_os("CARGO_FEATURE_VTD_LAB").is_none() || target != "x86_64-unknown-none") {
            continue;
        }
        if name == "edu" && env::var_os("CARGO_FEATURE_MATERIEL_LAB").is_none() {
            continue;
        }
        if name.starts_with("nvme_") && !nvme_lab {
            continue;
        }
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
        if name == "fp_probe" && env::var_os("CARGO_FEATURE_USER_FPU").is_none() {
            continue;
        }
        let mut rust = Command::new(env::var_os("RUSTC").unwrap());
        let linker = if nvme_rw {
            format!("link-arg=-T{}", user.join("novafs/service.ld").display())
        } else {
            linker
        };
        if nvme_lab && (nvme_rw || name == "storage/fileclient") {
            rust.arg("--cfg").arg("nvme_rw");
        }
        let compact = name.starts_with("nvme_") || matches!(
            name,
            "devices" | "edu" | "edu_dma" | "storage/relay" | "storage/disk" | "storage/requester" | "storage/control"
        );
        if name == "devices" && env::var_os("CARGO_FEATURE_MATERIEL_LAB").is_some() {
            rust.arg("--cfg").arg("materiel_lab");
        }
        if name == "ipc" && env::var_os("CARGO_FEATURE_IPC_SMP_LAB").is_some() {
            rust.args(["--cfg", "ipc_smp_lab"]);
        }
        if name == "ipc" && env::var_os("CARGO_FEATURE_IPC_EQUITE_LAB").is_some() {
            rust.args(["--cfg", "ipc_equite_lab"]);
        }
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
        if file_client || nvme_rw {
            rust.args(["-C", "link-arg=-zmax-page-size=4096"]);
        } else {
            rust.args(["-C", "link-arg=--oformat=binary"]);
        }
        let source = if name == "nvme_rw_driver" { "nvme_driver" } else { name };
        let ok = rust
            .arg(user.join(format!("{source}.rs")))
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
    if nvme_lab {
        cargo_service.args(["--features", "nvme-lab"]);
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
        "vi",
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
        "tail", "rmdir", "vi", "nsh",
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
    // Admission explicite au catalogue de qualification. Les indices de la CLI
    // restent stables et le profil ordinaire n'importe aucune image externe.
    println!("cargo::rerun-if-env-changed=NOVA_THREAD_PROBE_ELF");
    if env::var_os("CARGO_FEATURE_THREAD_PROBE").is_some() {
        let source = PathBuf::from(env::var_os("NOVA_THREAD_PROBE_ELF")
            .expect("thread-probe exige un ELF natif explicitement sélectionné"))
            .canonicalize().expect("ELF du témoin externe absent");
        println!("cargo::rerun-if-changed={}", source.display());
        let admitted = out.join("thread-witness.elf");
        std::fs::copy(&source, &admitted).expect("admission du témoin externe");
        catalog.push_str(&format!("(include_bytes!({admitted:?}),true),"));
    }
    catalog.push(']');
    std::fs::write(out.join("cli_catalog.rs"), catalog).unwrap();
}
