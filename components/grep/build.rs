fn main() {
    let lib = pkg_config::Config::new()
        .atleast_version("10.43")
        .statik(true)
        .cargo_metadata(false)
        .probe("libpcre2-8")
        .expect("PCRE2 8-bit >= 10.43 (headers and static library) required");
    cc::Build::new()
        .includes(lib.include_paths)
        .file("src/perl.c")
        .compile("grep_perl");
    for path in lib.link_paths {
        println!("cargo:rustc-link-search=native={}", path.display());
    }
    println!("cargo:rustc-link-lib=static=pcre2-8");
    println!("cargo:rerun-if-changed=src/perl.c");
}
