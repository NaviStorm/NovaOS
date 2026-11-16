#![no_std]
#![no_main]
#[allow(dead_code)]
#[path = "../../../../storage/runtime.rs"]
mod runtime;
use runtime as rt;
include!("common/policy_app.rs");
#[allow(dead_code)]
#[path = "../../../../../abi/files.rs"]
mod files;
#[unsafe(no_mangle)]
extern "C" fn run() -> ! {
    rt::exit(serve().err().unwrap_or(0))
}
