//! Décision d’admission privée. Ce processus ne reçoit ni canal ni vue de fichiers.
#![no_std]
#![no_main]
#[allow(dead_code)]
#[path = "../abi/native.rs"]
mod abi;
#[allow(dead_code)]
#[path = "../abi/instances.rs"]
mod instance_abi;
use instance_abi as wire;
mod gate;
#[path = "../abi/installation.rs"]
mod installation;

#[cfg(target_arch = "x86_64")]
core::arch::global_asm!(
    ".section .text.demo,\"ax\"",
    ".global _start",
    "_start:",
    "call run",
    "ud2"
);
#[cfg(target_arch = "aarch64")]
core::arch::global_asm!(
    ".section .text.demo,\"ax\"",
    ".global _start",
    "_start:",
    "bl run",
    "brk #0"
);

fn exit(code: u64) -> ! {
    gate::call(abi::PROCESS_EXIT, code, 0, 0);
    loop {
        core::hint::spin_loop()
    }
}

#[unsafe(no_mangle)]
extern "C" fn run() -> ! {
    // Page de configuration privée de cet ELF, remplie par le lanceur de confiance.
    let plan = unsafe { &*(0x401000 as *const [u64; 3]) };
    let [installation, ports, pair] = *plan;
    let Some(workers) = installation::grant(installation, ports, pair) else {
        exit(0)
    };
    if workers {
        let instance = gate::call(wire::WATCH, 0, 0, 0);
        if instance.status != abi::OK {
            exit(0)
        }
        let done = gate::call(wire::WATCH, 1, instance.value, 0);
        exit(if done.status == abi::OK {
            wire::ALLOW
        } else {
            0
        })
    }
    exit(wire::ALLOW)
}
#[panic_handler]
fn panic(_: &core::panic::PanicInfo<'_>) -> ! {
    exit(0)
}
