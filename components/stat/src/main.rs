#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
extern crate alloc;
#[cfg(target_os = "macos")]
#[path = "../../libraries/documentation.rs"]
#[allow(dead_code)]
mod documentation;
mod format;
#[cfg(target_os = "macos")]
mod host;
mod options;
mod quoting;
#[cfg(target_os = "macos")]
fn main() {
    std::process::exit(host::run())
}
#[cfg(target_os = "none")]
mod guest;
