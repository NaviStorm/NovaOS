#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
extern crate alloc;
mod args;
mod engine;
#[cfg(target_os = "none")]
mod guest;
#[cfg(target_os = "macos")]
mod host;
mod tables;
#[cfg(target_os = "macos")]
fn main() {
    std::process::exit(host::run());
}
