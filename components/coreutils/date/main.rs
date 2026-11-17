#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
extern crate alloc;
#[cfg(target_os = "macos")]
mod bytes;
#[cfg(target_os = "macos")]
mod calendar;
mod debug;
#[cfg(target_os = "macos")]
#[path = "../../libraries/documentation.rs"]
mod documentation;
mod engine;
mod format;
mod options;
#[cfg(target_os = "macos")]
#[path = "host.rs"]
mod platform;
#[cfg(target_os = "none")]
#[path = "guest.rs"]
mod platform;
#[cfg(target_os = "none")]
use nova_cli::documentation;
#[cfg(target_os = "macos")]
fn main() {
    platform::main()
}
