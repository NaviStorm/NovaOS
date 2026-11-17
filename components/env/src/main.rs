#![cfg_attr(target_os = "none", no_std)]
#![no_main]
extern crate alloc;
#[cfg(not(target_os = "none"))]
#[path = "../../libraries/documentation.rs"]
#[allow(dead_code)]
mod documentation;
mod engine;
#[cfg(target_os = "none")]
mod guest;
#[cfg(not(target_os = "none"))]
mod host;
mod quoting;
mod signals;
mod split;
