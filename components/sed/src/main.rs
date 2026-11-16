#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
extern crate alloc;
mod engine;
#[cfg(any(test, feature = "qualification"))]
mod fixtures;
#[cfg(not(target_os = "none"))]
mod host;
#[path = "../../libraries/documentation_locale.rs"]
mod locale;
#[cfg(target_os = "none")]
mod native;
mod options;

const VERSION: &str = "NovaOS sed 0.1.0 (QSE 0.9.1 / TRE)\n";
const HELP_EN: &str = include_str!("../help.en");
const HELP_FR: &str = include_str!("../help.fr");
#[cfg(not(target_os = "none"))]
fn main() {
    std::process::exit(host::main());
}
