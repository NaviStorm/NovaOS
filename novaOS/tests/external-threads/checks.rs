//! Contrôles complémentaires du même ELF, dans son propre espace utilisateur.
use super::*;
use nova_system::{Error, ipc};

/// Page de paramètres NovaOS. Chaque mot du protocole est aligné et partagé
/// uniquement avec le contrôleur de ce processus de qualification.
pub(super) fn word(index: usize) -> &'static AtomicU64 {
    assert!(index < 16);
    unsafe { &*((0x401000 as *const AtomicU64).add(index)) }
}

pub(super) fn mark(text: &[u8]) {
    let reply = unsafe { raw::call(raw::DEBUG_WRITE, text.as_ptr() as u64, text.len() as u64, 0) };
    assert_eq!(reply.status, raw::OK);
}

static PARK: AtomicU64 = AtomicU64::new(0);
static ARRIVED: AtomicU64 = AtomicU64::new(0);
static DONE: AtomicU64 = AtomicU64::new(0);

struct Returning;
impl thread::Task for Returning {
    fn run(value: usize) -> usize {
        // Le slot de la vague précédente a été collecté. Son TLS est neuf.
        assert_eq!(thread::tls_get(1).unwrap(), 0);
        thread::tls_set(1, value).unwrap();
        DONE.fetch_add(1, Ordering::Release);
        value
    }
}

struct Waiting;
impl thread::Task for Waiting {
    fn run(_: usize) -> usize {
        ARRIVED.fetch_add(1, Ordering::Release);
        let err = sync::wait(&PARK, 0, u64::MAX).unwrap_err();
        assert_eq!(err, Error(raw::CANCELLED));
        assert!(thread::cancelled().unwrap());
        63
    }
}

fn wait_arrivals(expected: u64) {
    while ARRIVED.load(Ordering::Acquire) != expected {
        thread::yield_now();
    }
}

struct Joining;
impl thread::Task for Joining {
    fn run(target: usize) -> usize {
        assert_eq!(
            unsafe { raw::call(raw::THREAD_JOIN, thread::id().unwrap(), 0, 0) }.status,
            raw::BAD_ARGUMENT
        );
        let reply = unsafe { raw::call(raw::THREAD_JOIN, target as u64, 0, 0) };
        assert_eq!(reply.status, raw::CANCELLED);
        assert!(thread::cancelled().unwrap());
        67
    }
}

fn cancelled_join() {
    let target = thread::spawn::<Waiting>(0, 4).unwrap();
    let old = target.raw();
    let joiner = thread::spawn::<Joining>(old as usize, 4).unwrap();
    // Le contrôleur a observé le join enregistré et la cible endormie.
    while word(1).load(Ordering::Acquire) != 1 {
        thread::yield_now();
    }
    for call in [raw::THREAD_JOIN, raw::THREAD_DETACH] {
        assert_eq!(unsafe { raw::call(call, old, 0, 0) }.status, raw::REFUSED);
    }
    joiner.cancel().unwrap();
    assert_eq!(joiner.join().unwrap(), 67);
    target.cancel().unwrap();
    assert_eq!(target.join().unwrap(), 63);

    let replacement = thread::spawn::<Waiting>(0, 4).unwrap();
    word(2).store(replacement.raw(), Ordering::Release);
    // Le contrôleur confirme le même slot et une nouvelle génération vivante.
    while word(1).load(Ordering::Acquire) != 2 {
        thread::yield_now();
    }
    for call in [raw::THREAD_CANCEL, raw::THREAD_JOIN, raw::THREAD_DETACH] {
        assert_eq!(unsafe { raw::call(call, old, 0, 0) }.status, raw::DENIED);
    }
    replacement.cancel().unwrap();
    assert_eq!(replacement.join().unwrap(), 63);
    mark(b"auto-join refuse attenteur unique join annule cible collectee ancien handle refuse");
}

static DELEGATED: sync::Mutex<Option<ipc::Endpoint>> = sync::Mutex::new(None);
struct Sender;
impl thread::Task for Sender {
    fn run(tag: usize) -> usize {
        let held = DELEGATED.lock().unwrap();
        let sender = held.as_ref().unwrap();
        assert!(matches!(
            sender.duplicate(raw::MANAGE),
            Err(Error(raw::DENIED))
        ));
        match sender.send(tag as u64, b"avant", 0) {
            Ok(()) => 0,
            Err(e) => e.0 as usize,
        }
    }
}

pub(super) fn lifecycle() {
    let end = nova_system::memory::heap_end().unwrap();
    assert!(end > 0 && end % 4096 == 0);
    assert_eq!(nova_system::memory::heap_end().unwrap(), end);
    let utc = nova_system::clock::utc_seconds().unwrap();
    assert!(utc >= 946684800 && nova_system::clock::utc_seconds().unwrap() >= utc);
    assert_eq!(
        unsafe { raw::call(raw::IPC_BOOTSTRAP, 0, 0, 0) }.status,
        raw::DENIED
    );
    let root = ipc::Endpoint::create().unwrap();
    let requests = root.duplicate(raw::RECEIVE).unwrap();
    let mut files =
        nova_system::files::Session::delegated(requests, ipc::Endpoint::create().unwrap());
    assert!(matches!(
        files.open(1, b"temoin", nova_system::files::READABLE),
        Err(Error(raw::DENIED))
    ));
    let mut buffer = [0x6b; 8];
    assert_eq!(files.read_view(1, 0, &mut buffer), Err(Error(raw::DENIED)));
    assert_eq!(buffer, [0x6b; 8]);
    assert_eq!(
        files.read_directory_view(1, b"", 0, 0),
        Err(Error(raw::DENIED))
    );
    drop(files);
    drop(root);
    mark(b"tas aligne stable RTC UTC ouverture et vues fichier repertoire refusees");
    assert_eq!(
        unsafe { raw::call(raw::THREAD_JOIN, 0, 0, 0) }.status,
        raw::DENIED
    );
    assert_eq!(
        unsafe { raw::call(raw::THREAD_CANCEL, u64::MAX, 0, 0) }.status,
        raw::DENIED
    );
    assert!(matches!(
        thread::spawn::<Returning>(1, 0),
        Err(Error(raw::BAD_ARGUMENT))
    ));
    assert!(matches!(
        thread::spawn::<Returning>(1, 65),
        Err(Error(raw::BAD_ARGUMENT))
    ));
    assert_eq!(sync::wait(&PARK, 0, 1).unwrap_err(), Error(raw::TIMED_OUT));
    let waiting = thread::spawn::<Waiting>(0, 4).unwrap();
    wait_arrivals(1);
    waiting.cancel().unwrap();
    assert_eq!(waiting.join().unwrap(), 63);

    // Le droit d'un thread terminé reste réservé jusqu'au join/détachement.
    let mut children = [const { None }; 8];
    for (i, child) in children.iter_mut().enumerate() {
        *child = Some(thread::spawn::<Returning>(i + 1, 4).unwrap());
    }
    assert!(matches!(
        thread::spawn::<Returning>(9, 4),
        Err(Error(raw::LIMIT))
    ));
    for (i, child) in children.into_iter().enumerate() {
        assert_eq!(child.unwrap().join().unwrap(), i + 1);
    }
    let detached = thread::spawn::<Returning>(17, 4).unwrap();
    let old = detached.raw();
    detached.detach().unwrap();
    while DONE.load(Ordering::Acquire) < 9 {
        thread::yield_now();
    }
    let replacement = thread::spawn::<Returning>(19, 4).unwrap();
    assert_ne!(old, replacement.raw());
    assert_eq!(
        unsafe { raw::call(raw::THREAD_JOIN, old, 0, 0) }.status,
        raw::DENIED
    );
    assert_eq!(replacement.join().unwrap(), 19);
    mark(b"cycle annulation delai detachement quota=8 reemploi-tls=0");

    let endpoint = ipc::Endpoint::create().unwrap();
    *DELEGATED.lock().unwrap() = Some(endpoint.duplicate(raw::SEND).unwrap());
    assert_eq!(thread::spawn::<Sender>(31, 4).unwrap().join().unwrap(), 0);
    let message = endpoint.receive(0).unwrap();
    assert_eq!(
        (message.tag, &message.payload[..message.len as usize]),
        (31, b"avant".as_slice())
    );
    endpoint.revoke().unwrap();
    assert_eq!(
        thread::spawn::<Sender>(32, 4).unwrap().join().unwrap(),
        raw::DENIED as usize
    );
    assert!(matches!(endpoint.receive(0), Err(Error(raw::WOULD_BLOCK))));
    drop(DELEGATED.lock().unwrap().take());
    // La racine demeure utilisable.
    endpoint.send(33, b"racine", 0).unwrap();
    assert_eq!(endpoint.receive(0).unwrap().tag, 33);
    mark(b"capacite delegation envoi puis revocation racine conservee");
}

struct Compute;
impl thread::Task for Compute {
    fn run(index: usize) -> usize {
        let seed = 0xa50 + index;
        assert_eq!(thread::tls_get(5).unwrap(), 0);
        thread::tls_set(5, seed).unwrap();
        word(2 + index).store(thread::id().unwrap(), Ordering::Release);
        // L'assembleur travaille sans appel ni yield, jusqu'au signal du contrôleur.
        let result = unsafe { fp_context(seed, word(4 + index), word(1)) };
        assert_eq!(result, seed);
        assert_eq!(thread::tls_get(5).unwrap(), seed);
        seed
    }
}

struct Receiving;
impl thread::Task for Receiving {
    fn run(_: usize) -> usize {
        let endpoint = ipc::Endpoint::create().unwrap();
        let _ = endpoint.receive(15000);
        panic!("reprise après destruction");
    }
}

struct Notified;
impl thread::Task for Notified {
    fn run(expected: usize) -> usize {
        sync::wait(&PARK, 0, u64::MAX).unwrap();
        assert_eq!(PARK.load(Ordering::Acquire), 29);
        expected
    }
}

struct LastWorker;
impl thread::Task for LastWorker {
    fn run(value: usize) -> usize {
        assert_eq!(thread::tls_get(1).unwrap(), 0);
        thread::tls_set(1, value).unwrap();
        word(2).fetch_add(1, Ordering::Release);
        while word(1).load(Ordering::Acquire) == 0 {
            thread::yield_now();
        }
        thread::tls_get(1).unwrap()
    }
}

struct Burning;
impl thread::Task for Burning {
    fn run(_: usize) -> usize {
        unsafe { fp_context(0xbad, word(5), word(9)) }
    }
}

struct GuardPage;
impl thread::Task for GuardPage {
    fn run(_: usize) -> usize {
        word(2).store(thread::id().unwrap(), Ordering::Release);
        let address = loop {
            let address = word(1).load(Ordering::Acquire);
            if address != 0 {
                break address;
            }
            thread::yield_now();
        };
        // Le contrôleur donne la dernière adresse de la garde basse du thread.
        unsafe { core::ptr::write_volatile(address as *mut u8, 17) };
        1
    }
}

pub(super) fn scenario(mode: u64) {
    match mode {
        1 => {
            let a = thread::spawn::<Compute>(0, 4).unwrap();
            let b = thread::spawn::<Compute>(1, 4).unwrap();
            assert_eq!(a.join().unwrap(), 0xa50);
            assert_eq!(b.join().unwrap(), 0xa51);
            mark(b"FPU SIMD GPR TLS conserves sur deux threads");
        }
        2 => {
            let child = thread::spawn::<GuardPage>(0, 4).unwrap();
            let _ = child.join();
            panic!("garde accessible");
        }
        3 => {
            let first = thread::spawn::<Waiting>(0, 4).unwrap();
            let _joining = thread::spawn::<Joining>(first.raw() as usize, 4).unwrap();
            let _second = thread::spawn::<Waiting>(0, 4).unwrap();
            let _receiving = thread::spawn::<Receiving>(0, 4).unwrap();
            let _running = thread::spawn::<Burning>(0, 4).unwrap();
            wait_arrivals(2);
            while word(1).load(Ordering::Acquire) == 0 {
                thread::yield_now();
            }
            process::exit(33);
        }
        4 => {
            // Un TID valide dans l'autre processus n'octroie aucun droit ici.
            word(14).store(0xdabb, Ordering::Release);
            let handle = word(1).load(Ordering::Acquire);
            assert_eq!(
                unsafe { raw::call(raw::THREAD_CANCEL, handle, 0, 0) }.status,
                raw::DENIED
            );
            assert_eq!(
                unsafe { raw::call(raw::THREAD_JOIN, handle, 0, 0) }.status,
                raw::DENIED
            );
            assert_eq!(thread::tls_get(5).unwrap(), 0);
        }
        5 => {
            let a = thread::spawn::<Notified>(51, 4).unwrap();
            let b = thread::spawn::<Notified>(52, 4).unwrap();
            while word(1).load(Ordering::Acquire) == 0 {
                thread::yield_now();
            }
            PARK.store(29, Ordering::Release);
            assert_eq!(sync::notify(&PARK, 1).unwrap(), 1);
            assert_eq!(sync::notify(&PARK, 1).unwrap(), 1);
            assert_eq!(sync::notify(&PARK, 1).unwrap(), 0);
            assert_eq!(sync::wait(&PARK, 0, u64::MAX), Err(Error(raw::WOULD_BLOCK)));
            assert_eq!(a.join().unwrap(), 51);
            assert_eq!(b.join().unwrap(), 52);
            mark(b"notifications 1+1+0 concurrence valeur-publiee=29");
        }
        6 => {
            thread::tls_set(1, 17).unwrap();
            let _last = thread::spawn::<LastWorker>(19, 4).unwrap();
            thread::exit(17);
        }
        7 => {
            // La sonde superviseur retire cette page privée lorsque l'unique
            // thread est endormi. Aucun accès utilisateur au tampon après l'appel.
            let base = nova_system::memory::heap_end().unwrap();
            let mut frame: raw::ReceiveV1 = unsafe { core::mem::zeroed() };
            frame.version = raw::VERSION;
            frame.size = size_of::<raw::ReceiveV1>() as u32;
            frame.timeout = 15000;
            let endpoint = ipc::Endpoint::create().unwrap();
            unsafe {
                nova_system::memory::resize_heap(base, base + 4096).unwrap();
                (base as *mut raw::ReceiveV1).write(frame);
            }
            let reply = unsafe {
                raw::call(
                    raw::IPC_RECEIVE,
                    endpoint.raw(),
                    base as u64,
                    size_of::<raw::ReceiveV1>() as u64,
                )
            };
            assert_eq!(reply.status, raw::BAD_ADDRESS);
            assert_eq!(nova_system::memory::heap_end().unwrap(), base);
            endpoint.send(37, b"reprise", 0).unwrap();
            assert_eq!(endpoint.receive(0).unwrap().tag, 37);
            mark(b"IPC reception retiree BAD_ADDRESS puis message=37");
        }
        8 => {
            assert!(matches!(
                thread::spawn::<Returning>(17, 1),
                Err(Error(raw::DENIED))
            ));
            assert_eq!(sync::wait(&PARK, 0, 0), Err(Error(raw::DENIED)));
            assert_eq!(thread::tls_get(0), Err(Error(raw::DENIED)));
            assert!(thread::id().unwrap() != 0);
            thread::yield_now();
            assert_eq!(DONE.load(Ordering::Acquire), 0);
            mark(b"admission absente creation attente TLS refusees ID et yield OK");
        }
        9 => cancelled_join(),
        10 => {
            let _a = thread::spawn::<LastWorker>(71, 4).unwrap();
            let _b = thread::spawn::<LastWorker>(73, 4).unwrap();
            thread::exit(17);
        }
        _ => panic!("mode de qualification inconnu"),
    }
}

unsafe extern "C" {
    fn fp_context(seed: usize, progress: &AtomicU64, release: &AtomicU64) -> usize;
}

#[cfg(target_arch = "x86_64")]
core::arch::global_asm!(include_str!("fp_x86_64.S"));
#[cfg(target_arch = "aarch64")]
core::arch::global_asm!(include_str!("fp_aarch64.S"));
