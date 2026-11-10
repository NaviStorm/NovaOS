//! Programme de vérification autonome de l’ABI. Aucun pointeur noyau importé.
use super::abi::*;
use core::arch::asm;

fn call(id: u64, a: u64, b: u64, c: u64) -> Reply {
    let (status, value);
    // Sûreté : conventions NovaOS v1. Les registres de retour et les
    // destructions architecturales sont déclarés ; la porte peut écrire en mémoire.
    #[cfg(target_arch = "x86_64")]
    unsafe {
        asm!("syscall",inlateout("rax")id=>status,in("rdi")a,in("rsi")b,
        inlateout("rdx")c=>value,lateout("rcx")_,lateout("r11")_,options(nostack))
    };
    // Sûreté : SVC v1 préserve tous les registres hors x0/x1 et restaure SP.
    #[cfg(target_arch = "aarch64")]
    unsafe {
        asm!("svc #0x4e",in("x8")id,inlateout("x0")a=>status,inlateout("x1")b=>value,
        in("x2")c,options(nostack))
    };
    Reply { status, value }
}

const DATA: u64 = 0x401000;
const OUT: u64 = DATA + 256;
fn word(i: usize) -> u64 {
    // Sûreté : mots du bloc de lancement utilisateur ; la sonde de révocation
    // les charge tous avant de se bloquer et n’y accède plus ensuite.
    unsafe { (DATA as *const u64).add(i).read_volatile() }
}
fn store(i: usize, value: u64) {
    // Sûreté : index constants de la page RW pendant la phase de charge.
    unsafe { (DATA as *mut u64).add(i).write_volatile(value) };
}
fn probe() -> Result<u64, u64> {
    let mode = word(0);
    let key = word(5);
    let target = word(6);
    let expected_kind = word(7);
    let expected_code = word(10);
    let p = call(PROCESS_ID, 0, 0, 0);
    let t = call(THREAD_ID, 0, 0, 0);
    if p.status != OK || t.status != OK || p.value == 0 || t.value == 0 {
        return Err(1);
    }
    store(8, p.value);
    store(9, t.value);
    let wait = |key, out, len| call(PROCESS_WAIT, key, out, len);
    if mode == 12 {
        if wait(key, OUT, 32).status != DENIED {
            return Err(2);
        }
        return Ok(12);
    }
    if mode == 11 {
        let text = b"NovaABI";
        let print = |addr, len| call(DEBUG_WRITE, addr, len, 0);
        let first = print(text.as_ptr() as u64, 7);
        if first.status != OK || first.value != 7 {
            return Err(3);
        }
        for (addr, len, expected) in [
            (0, 1, BAD_ADDRESS),
            (u64::MAX - 2, 8, BAD_ADDRESS),
            (0xffffffff80000000, 8, BAD_ADDRESS),
            (DATA + 4090, 16, BAD_ADDRESS),
            (DATA - 2, 4, BAD_ARGUMENT),
            (DATA, 0, BAD_ARGUMENT),
            (DATA, 257, BAD_ARGUMENT),
            (DATA, u64::MAX, BAD_ARGUMENT),
            (DATA + 64, 8, BAD_ARGUMENT),
        ] {
            if print(addr, len).status != expected {
                return Err(4);
            }
        }
        for _ in 1..16 {
            if print(text.as_ptr() as u64, 7).status != OK {
                return Err(5);
            }
        }
        if print(text.as_ptr() as u64, 7).status != LIMIT {
            return Err(6);
        }
        // Alterner appels légitimes et sélecteurs malformés pendant les IRQ réelles.
        let mut n = 0;
        while n < 4096 || word(4) == 0 {
            if call(0x1ffff, n, 0, 0).status != UNKNOWN_CALL
                || call(0x2_0001, 0, n, 0).status != BAD_VERSION
                || call(PROCESS_ID, 0, 0, n).value != p.value
            {
                return Err(7);
            }
            n += 1;
            if n % 256 == 0 {
                store(3, n);
            }
        }
        if wait(key ^ 1, OUT, 32).status != DENIED
            || wait(key, OUT + 1, 32).status != BAD_ARGUMENT
            || wait(key, OUT, 31).status != BAD_ARGUMENT
        {
            return Err(8);
        }
        store(511, 0x12345678);
        for addr in [0x400000, 0xffff_ffff_8000_0000, DATA + 4088, u64::MAX - 7] {
            if wait(key, addr, 32).status != BAD_ADDRESS || word(511) != 0x12345678 {
                return Err(9);
            }
        }
    }
    let mut info = WaitInfo {
        version: 0,
        size: 0,
        process: 0,
        kind: 0,
        code: 0,
    };
    if mode == 13 && wait(key, OUT, 32).status != BAD_ADDRESS {
        return Err(10);
    }
    let r = wait(key, (&raw mut info) as u64, WAIT_SIZE as u64);
    if r.status != OK
        || r.value != 32
        || info.version != VERSION
        || info.size != 32
        || info.process != target
        || info.kind != expected_kind
        || info.code != expected_code
    {
        return Err(11);
    }
    if wait(key, 0x800200, 32).status != DENIED {
        return Err(12);
    }
    Ok(mode)
}

// Sûreté : appelé uniquement par le démarrage utilisateur avec une pile ABI alignée.
#[unsafe(no_mangle)]
pub extern "C" fn nova_abi_probe() -> u64 {
    probe().unwrap_or_else(|n| 0x7000 + n)
}
