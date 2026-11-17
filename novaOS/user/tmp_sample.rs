//! Épreuve en espace utilisateur de la capacité tmp d’une instance.
#![no_std]
#![no_main]
#[allow(dead_code)]
#[path = "../abi/instances.rs"]
mod instance;
#[allow(dead_code)]
#[path = "../abi/files.rs"]
mod fs;
#[allow(dead_code)]
#[path = "../abi/tmp.rs"]
mod tmp;
#[path = "storage/runtime.rs"]
mod runtime;
use runtime as rt;
use core::sync::atomic::{AtomicU64, Ordering};
use rt::call;
#[path = "tmp_wait.rs"]
mod wait_probe;
#[allow(dead_code)]
#[path = "../abi/terminal.rs"]
mod terminal;
#[path = "tmp_ticket.rs"]
mod ticket_probe;

fn word(i: usize) -> &'static AtomicU64 {
    assert!(i < 20);
    unsafe { &*((0x401000 as *const AtomicU64).add(i)) }
}

fn wait_for(value: u64) {
    while word(5).load(Ordering::Acquire) < value {
        call(rt::THREAD_YIELD, 0, 0, 0);
    }
}

fn check(actual: u64, expected: u64, point: u64) {
    if actual != expected {
        word(8).store(actual, Ordering::Release);
        rt::exit(point)
    }
}

fn cap() -> u64 {
    let r = call(tmp::CAP, 0, 0, 0);
    check(r.status, rt::OK, 51);
    r.value
}

fn send(cap: u64, q: &mut fs::Frame) -> u64 {
    call(tmp::CALL, cap, q.as_mut_ptr() as u64, q.len() as u64).status
}

fn open(cap: u64, name: &[u8], flags: u8, expected: u64) -> u64 {
    let mut q = fs::frame(fs::OPEN_AT, 1, 0);
    q[2] = flags;
    fs::size(&mut q, name.len());
    q[fs::PAYLOAD..fs::PAYLOAD + name.len()].copy_from_slice(name);
    check(send(cap, &mut q), expected, 52);
    fs::word(&q, 16)
}

fn stage_byte(cap: u64, h: u64, byte: u8) {
    let mut q = fs::frame(fs::WRITE, 1, h);
    q[2] = fs::START;
    q[fs::PAYLOAD] = byte;
    fs::size(&mut q, 1);
    fs::set(&mut q, 32, 1);
    check(send(cap, &mut q), rt::OK, 53);
}

fn write(cap: u64, h: u64, byte: u8) {
    stage_byte(cap, h, byte);
    check(send(cap, &mut fs::frame(fs::SYNC, 1, h)), rt::OK, 54);
}

fn read(cap: u64, h: u64, byte: u8) {
    let mut q = fs::frame(fs::READ, 1, h);
    fs::size(&mut q, 1);
    check(send(cap, &mut q), rt::OK, 55);
    check(fs::length(&q) as u64, 1, 56);
    check(q[fs::PAYLOAD] as u64, byte as u64, 57);
}

fn rename(cap: u64, from: &[u8], to: &[u8], expected: u64) {
    for (slot, name) in [from, to].iter().enumerate() {
        let mut q = fs::frame(fs::PATH_FRAGMENT, 1, 0);
        q[2] = slot as u8;
        fs::size(&mut q, name.len());
        fs::set(&mut q, 32, name.len() as u64);
        q[fs::PAYLOAD..fs::PAYLOAD + name.len()].copy_from_slice(name);
        check(send(cap, &mut q), rt::OK, 69);
    }
    check(send(cap, &mut fs::frame(fs::RENAME_AT, 1, 0)), expected, 70);
}

extern "C" fn thread(h: u64) -> ! {
    read(cap(), h, b'A');
    call(rt::THREAD_EXIT, 37, 0, 0);
    rt::exit(58)
}

fn worker(delegated: bool) {
    let mut ident = [0u8; 32];
    check(call(instance::INFO, 2, ident.as_mut_ptr() as u64, 32).status, rt::OK, 59);
    let (op, mode) = if delegated { (tmp::WORKER, 101) } else { (instance::WORKER, 100) };
    let child = call(op, ident.as_ptr() as u64, mode, 32);
    check(child.status, rt::OK, 60);
    let mut result = [0u64; 4];
    check(call(rt::PROCESS_WAIT, child.value, result.as_mut_ptr() as u64, 32).status, rt::OK, 61);
    check(result[3], mode, 62);
}

#[unsafe(no_mangle)]
extern "C" fn run() -> ! {
    let mode = rt::config(0);
    if mode == 210 { ticket_probe::run() }
    if matches!(mode, 200 | 201) {
        wait_probe::run(mode)
    }
    if mode == 100 {
        check(call(tmp::CAP, 0, 0, 0).status, rt::DENIED, 63);
        rt::exit(100)
    }
    let own = cap();
    if mode == 101 {
        let h = open(own, b"commun", fs::READABLE, rt::OK);
        read(own, h, b'A');
        rt::exit(101)
    }
    let mut dir = fs::frame(fs::READDIR, 1, 0);
    check(send(own, &mut dir), rt::OK, 64);
    check(fs::length(&dir) as u64, 0, 65);
    open(own, b"../documents/exemple.txt", fs::READABLE, rt::DENIED);
    open(own, b"/documents/exemple.txt", fs::READABLE, rt::DENIED);
    let h = open(own, b"commun", fs::READABLE | fs::WRITABLE | fs::CREATE, rt::OK);
    if mode == 212 {
        let total = 32768;
        let mut offset = 0;
        while offset < total {
            let mut q = fs::frame(fs::WRITE, 1, h);
            let n = fs::FRAGMENT.min(total-offset);
            q[2] = if offset == 0 { fs::START } else { fs::CONTINUE };
            for (i,b) in q[fs::PAYLOAD..][..n].iter_mut().enumerate() {
                *b = (offset+i) as u8 ^ 0xa5;
            }
            fs::size(&mut q,n);
            fs::set(&mut q,24,offset as u64);
            fs::set(&mut q,32,total as u64);
            check(send(own,&mut q),rt::OK,95);
            offset += n;
        }
        check(send(own, &mut fs::frame(fs::SYNC,1,h)),rt::OK,96);
        let mut q = fs::frame(fs::READ,1,h);
        fs::set(&mut q,24,(total-1) as u64);
        fs::size(&mut q,1);
        check(send(own,&mut q),rt::OK,97);
        check(q[fs::PAYLOAD] as u64,0xff ^ 0xa5,98);
        word(1).store(own,Ordering::Release);
        word(2).store(h,Ordering::Release);
        word(4).store(1,Ordering::Release);
        wait_for(1);
        rt::exit(0)
    }
    let data = if mode == 1 { b'A' } else { b'B' };
    write(own, h, data);
    read(own, h, data);
    if mode == 1 {
        let mut envelope = [0u8; tmp::ENVELOPE];
        check(call(tmp::NEXT, 0, envelope.as_mut_ptr() as u64, tmp::ENVELOPE as u64).status, rt::DENIED, 74);
        check(call(tmp::COMPLETE, 1, 0, 0).status, rt::DENIED, 75);
        static READ_ONLY: fs::Frame = [0; tmp::FRAME];
        check(call(tmp::CALL, own, READ_ONLY.as_ptr() as u64, tmp::FRAME as u64).status, rt::BAD_ADDRESS, 76);
        let t = call(rt::THREAD_CREATE, thread as *const () as u64, h, 2);
        check(t.status, rt::OK, 66);
        let result = call(rt::THREAD_JOIN, t.value, 0, 0);
        check(result.status, rt::OK, 67);
        check(result.value, 37, 68);
        worker(false);
        worker(true);
        rename(own, b"commun", b"/documents/deplace", rt::DENIED);
        rename(own, b"commun", b"../deplace", rt::DENIED);
        rename(own, b"commun", b"renomme", rt::OK);
        read(own, h, b'A');
        rename(own, b"renomme", b"commun", rt::OK);
        let transient = open(own, b"commun", fs::READABLE, rt::OK);
        check(send(own, &mut fs::frame(fs::CLOSE, 1, transient)), rt::OK, 71);
        check(send(own, &mut fs::frame(fs::STAT, 1, transient)), rt::REVOKED, 72);
    } else if mode == 2 {
        // Le superviseur arrête cette instance après WRITE et avant SYNC.
        stage_byte(own, h, b'X');
    }
    word(1).store(own, Ordering::Release);
    word(2).store(h, Ordering::Release);
    word(4).store(1, Ordering::Release);
    wait_for(1);
    if mode == 213 {
        let old_cap = word(3).load(Ordering::Acquire);
        let old_handle = word(6).load(Ordering::Acquire);
        check(send(old_cap, &mut fs::frame(fs::STAT,1,old_handle)),rt::DENIED,99);
        check(send(own, &mut fs::frame(fs::STAT,1,h)),fs::BUSY,100);
        word(4).store(2,Ordering::Release);
        wait_for(2);
        check(send(own, &mut fs::frame(fs::STAT,1,old_handle)),rt::REVOKED,101);
        read(own,h,b'B');
        rt::exit(0)
    }
    if mode == 3 {
        #[cfg(target_arch = "aarch64")]
        unsafe { core::arch::asm!("brk #0", options(noreturn)) }
        #[cfg(target_arch = "x86_64")]
        unsafe { core::arch::asm!("ud2", options(noreturn)) }
    }
    if mode == 1 {
        let other = word(3).load(Ordering::Acquire);
        let mut q = fs::frame(fs::READDIR, 1, 0);
        let foreign_handle = word(6).load(Ordering::Acquire);
        check(send(own, &mut fs::frame(fs::STAT, 1, foreign_handle)), rt::REVOKED, 73);
        let refused = send(other, &mut q);
        assert!(matches!(refused, rt::DENIED | rt::REVOKED));
        read(own, h, b'A');
        word(4).store(2, Ordering::Release);
        wait_for(2);
        assert!(matches!(send(other, &mut q), rt::DENIED | rt::REVOKED));
        read(own, h, b'A');
    }
    // Le dernier handle reste ouvert au moment du retrait.
    rt::exit(0)
}
