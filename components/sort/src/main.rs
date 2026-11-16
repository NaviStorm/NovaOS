#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
extern crate alloc;
const VERSION: &str = concat!("sort (NovaOS) ", env!("CARGO_PKG_VERSION"), "\n");
const EN: &str = include_str!("../doc/help.en");
const FR: &str = include_str!("../doc/help.fr");
#[cfg(not(target_os = "none"))]
mod host;
#[cfg(not(target_os = "none"))]
#[path = "../../libraries/documentation_locale.rs"]
mod locale;
#[cfg(target_os = "none")]
mod native;
#[cfg(not(target_os = "none"))]
fn main() {
    let name = std::env::args_os().next().unwrap_or_default();
    match host::command(&name.to_string_lossy()) {
        Ok(status) => std::process::exit(status),
        Err(e) => {
            eprintln!("{}: {e}", name.to_string_lossy());
            std::process::exit(if e.to_string().contains("\nValid arguments are:") {
                1
            } else {
                2
            })
        }
    }
}
#[cfg(all(target_os = "none", feature = "qualification"))]
mod fixtures;
#[cfg(target_os = "none")]
mod private;
