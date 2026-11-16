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
    let marker = b"NAP ELF mis a jour execute";
    rt::call(rt::DEBUG_WRITE, marker.as_ptr() as u64, marker.len() as u64, 0);
    rt::exit(serve().err().unwrap_or(0))
}
