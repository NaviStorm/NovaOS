//! Bibliothèque de la session de stockage, liée séparément dans chaque image.
#[allow(dead_code)]
#[path = "../../abi/native.rs"]
pub mod abi;
#[path = "../gate.rs"]
mod gate;
pub use abi::*;
pub use gate::call;

pub const INFO: u8 = 1;
pub const FIN: u8 = 2;
pub const BAD_FRAME: u8 = 1;
pub const UNSUPPORTED: u8 = 2;
pub const NO_VOLUME: u8 = 3;
pub const WIRE_VERSION: u8 = 1;
pub const TIMEOUT: u64 = 250;
pub const IDENTITY: &[u8; 4] = b"NVFS";

pub type Result<T> = core::result::Result<T, u64>;

// Sûreté : chaque image plate démarre à _start en Ring 3 ou EL0 avec une
// pile privée alignée. L'appel établit le cadre ABI Rust de l'entrée divergeante.
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

pub fn value(r: Reply) -> Result<u64> {
    if r.status == OK {
        Ok(r.value)
    } else {
        Err(r.status)
    }
}

pub fn channels() -> Result<(u64, u64)> {
    let requests = value(call(IPC_BOOTSTRAP, 0, 0, 0))?;
    Ok((requests, config(0)))
}

pub fn config(word: usize) -> u64 {
    assert!(word < 6);
    // Sûreté : le lanceur initialise les mots alignés de la page DATA privée
    // avant publication. Ce sont des valeurs locales, jamais des pointeurs.
    unsafe { (0x401000 as *const u64).add(word).read_volatile() }
}

pub fn receive(key: u64) -> Result<ReceiveV1> {
    receive_for(key, TIMEOUT)
}

pub fn receive_for(key: u64, timeout: u64) -> Result<ReceiveV1> {
    let mut msg = ReceiveV1 {
        version: VERSION,
        size: 184,
        timeout,
        options: 0,
        reserved: 0,
        kind: 0,
        len: 0,
        tag: 0,
        handle: 0,
        aux: 0,
        payload: [0; 128],
    };
    value(call(IPC_RECEIVE, key, (&raw mut msg) as u64, 184))?;
    Ok(msg)
}

pub fn send(key: u64, tag: u64, bytes: &[u8]) -> Result<()> {
    if bytes.len() > 128 {
        return Err(BAD_ARGUMENT);
    }
    let mut msg = SendV1 {
        version: VERSION,
        size: 160,
        timeout: TIMEOUT,
        tag,
        len: bytes.len() as u32,
        flags: 1,
        payload: [0; 128],
    };
    msg.payload[..bytes.len()].copy_from_slice(bytes);
    let n = value(call(IPC_SEND, key, (&raw const msg) as u64, 160))?;
    if n == bytes.len() as u64 {
        Ok(())
    } else {
        Err(BAD_ARGUMENT)
    }
}

pub fn exit(code: u64) -> ! {
    call(PROCESS_EXIT, code, 0, 0);
    loop {
        core::hint::spin_loop();
    }
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo<'_>) -> ! {
    exit(255)
}
