#![no_std]
#![no_main]
#[allow(dead_code)]
#[path = "../abi/native.rs"]
mod abi;
mod gate;
use abi::*;
use core::arch::global_asm;
use core::sync::atomic::{AtomicU64, Ordering};
use gate::call;

// Sûreté : entrée autonome CPL3/EL0t, SP initial de la pile privée aligné.
#[cfg(target_arch = "x86_64")]
global_asm!(
    ".section .text.demo,\"ax\"",
    ".global _start",
    "_start:",
    "call worker",
    "ud2"
);
#[cfg(target_arch = "aarch64")]
global_asm!(
    ".section .text.demo,\"ax\"",
    ".global _start",
    "_start:",
    "bl worker",
    "brk #0"
);

const DATA: usize = 0x401000;
fn read(i: usize) -> u64 {
    // Sûreté : mots de commande alignés dans la première page DATA du lanceur.
    unsafe { AtomicU64::from_ptr((DATA as *mut u64).add(i)).load(Ordering::Acquire) }
}
fn write(i: usize, v: u64) {
    // Sûreté : la page DATA privée reste RW pendant toute la vie du travailleur.
    unsafe { AtomicU64::from_ptr((DATA as *mut u64).add(i)).store(v, Ordering::Release) }
}

const SEND_BUF: u64 = DATA as u64 + 256;
const RECV_BUF: u64 = DATA as u64 + 512;

// Chaque série s'exécute entièrement en CPL3/EL0. Les canaux de la demande
// et de la réponse sont distincts pour qu'un acteur ne consomme pas son envoi.
fn exchange(count: u64, mode: u64, responder: bool) -> Reply {
    let tx = read(6);
    let rx = read(7);
    let receive = || {
        let r = call(IPC_RECEIVE, rx, RECV_BUF, 184);
        if r.status != OK {
            return r;
        }
        let kind = match (mode, responder) {
            (3, _) => 2,
            (4, true) | (5, _) => 3,
            _ => 1,
        };
        let len = if mode == 1 {
            32
        } else if mode == 2 {
            128
        } else {
            0
        };
        let valid = r.value == kind
            && read(64 + 3) == (kind | (len << 32))
            && match kind {
                1 => read(64 + 4) == 19,
                2 => read(64 + 6) == 1,
                _ => read(64 + 5) != 0 && read(64 + 6) == 5,
            };
        if valid {
            r
        } else {
            Reply {
                status: BAD_ARGUMENT,
                value: 0,
            }
        }
    };
    for _ in 0..count {
        if responder {
            let r = receive();
            if r.status != OK {
                return r;
            }
            if mode >= 4 {
                let imported = read(64 + 5);
                if mode == 4 {
                    let r = call(IPC_CLOSE, imported, 0, 0);
                    if r.status != OK {
                        return r;
                    }
                } else {
                    write(32 + 2, imported);
                }
            }
        }
        let r = match (mode, responder) {
            (3, _) => call(IPC_NOTIFY, tx, 1, 0),
            (4, false) | (5, _) => call(IPC_GIVE, tx, SEND_BUF, 48),
            _ => call(IPC_SEND, tx, SEND_BUF, 160),
        };
        if r.status != OK {
            return r;
        }
        if !responder {
            let r = receive();
            if r.status != OK {
                return r;
            }
            if mode == 5 {
                write(32 + 2, read(64 + 5));
            }
        }
    }
    Reply {
        status: OK,
        value: count,
    }
}
fn background(kind: u64) -> Reply {
    let chan = read(6);
    let mut rounds = 0;
    while read(10) == 0 {
        if kind == 1 {
            let r = call(IPC_SEND, chan, SEND_BUF, 160);
            if r.status != OK {
                return r;
            }
            let r = call(IPC_RECEIVE, chan, RECV_BUF, 184);
            if r.status != OK {
                return r;
            }
        } else {
            for i in 0..128u64 {
                core::hint::black_box(i.wrapping_mul(rounds));
            }
        }
        rounds += 1;
        write(11, rounds);
        call(THREAD_YIELD, 0, 0, 0);
    }
    Reply {
        status: OK,
        value: rounds,
    }
}

// Sûreté : symbole appelé depuis le point d’entrée utilisateur ci-dessus.
#[unsafe(no_mangle)]
extern "C" fn worker() -> ! {
    loop {
        let op = read(0);
        if op == 0 {
            call(THREAD_YIELD, 0, 0, 0);
            continue;
        }
        if op == u64::MAX {
            // Sûreté : faute volontaire confinée à ce processus de test.
            unsafe { (0xffffffff80000000u64 as *const u64).read_volatile() };
        }
        let r = match op {
            // Le client de mesure termine sur la dernière réponse. Le serveur
            // attend que le contrôleur ait observé les sorties de tous les clients.
            #[cfg(ipc_smp_lab)]
            x if x == u64::MAX - 3 => {
                let r = exchange(read(1), 2, false);
                call(PROCESS_EXIT, if r.status == OK && r.value == read(1) { 0 } else { 255 }, 0, 0);
                unreachable!()
            }
            x if x == u64::MAX - 1 => exchange(read(1), read(2), read(3) != 0),
            x if x == u64::MAX - 2 => background(read(1)),
            _ => call(op, read(1), read(2), read(3)),
        };
        write(4, r.status);
        write(5, r.value);
        write(0, 0);
    }
}
#[panic_handler]
fn panic(_: &core::panic::PanicInfo<'_>) -> ! {
    call(PROCESS_EXIT, 255, 0, 0);
    loop {
        core::hint::spin_loop();
    }
}
