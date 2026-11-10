fn main() {
    // Réserver 2 Mio pour le thread principal de la CLI Darwin, y compris le
    // vérificateur. Le plafond des parcours vient du profil disque P0.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        println!("cargo::rustc-link-arg-bin=novafs=-Wl,-stack_size,0x200000");
    }
    println!("cargo::rerun-if-changed=build.rs");
}
