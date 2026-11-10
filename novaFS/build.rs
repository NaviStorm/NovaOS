fn main() {
    // Le banc et la CLI mesurent la même réserve de pile principale Darwin.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        println!("cargo::rustc-link-arg-bin=novafs=-Wl,-stack_size,0x200000");
        println!("cargo::rustc-link-arg-bin=novafs-bench=-Wl,-stack_size,0x200000");
    }
    println!("cargo::rerun-if-changed=build.rs");
}
