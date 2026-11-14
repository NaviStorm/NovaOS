//! Dispatch commun des appels natifs. Les adresses de l’appelant restent des entiers.
use super::*;
use crate::abi::*;
use core::sync::atomic::{AtomicBool, Ordering};

fn caller(s: &Core) -> Pid {
    s.thread(s.current).owner.expect("appel hors processus")
}

fn debug_write(addr: u64, len: u64) -> Result<u64, u64> {
    if !(1..=DEBUG_MAX as u64).contains(&len) {
        return Err(BAD_ARGUMENT);
    }
    let mut buf = [0u8; DEBUG_MAX];
    let bytes = &mut buf[..len as usize];
    let pid = with(|s| {
        let p = s.processes.get_mut(caller(s));
        if p.debug_messages >= 16 {
            return Err(LIMIT);
        }
        let space = p.space.as_ref().unwrap();
        let valid = heap::with_pages(|m| Ok(space.copy_from_user(m, addr, bytes))).unwrap();
        if !valid {
            return Err(BAD_ADDRESS);
        }
        if !bytes.iter().all(|b| (0x20..=0x7e).contains(b)) {
            return Err(BAD_ARGUMENT);
        }
        p.debug_messages += 1;
        Ok(p.pid.number)
    })?;
    let msg = core::str::from_utf8(bytes).expect("ASCII filtré");
    if !storage::time_marker(msg) {
        hal::log(format_args!("DEBUG pid={pid} {msg}"));
    }
    Ok(len)
}

fn wait(key: u64, out: u64, len: u64) -> Result<u64, u64> {
    if len != WAIT_SIZE as u64 || out % 8 != 0 {
        return Err(BAD_ARGUMENT);
    }
    loop {
        let _irq = Guard::new();
        // La vérification des droits précède toute consultation de la cible.
        // Sous le même masque, consommer le résultat ou publier l’attente.
        let target = with(|s| {
            let pid = caller(s);
            let p = s.processes.get(pid);
            let right = p.wait_right.filter(|r| r.key() == key).ok_or(DENIED)?;
            let child = s.processes.get(right.target);
            let space = p.space.as_ref().unwrap();
            if child.space.is_some() {
                if !heap::with_pages(|m| Ok(space.writable(m, out, WAIT_SIZE))).unwrap() {
                    return Err(BAD_ADDRESS);
                }
                s.processes.blocked_waits += 1;
                return Ok(Some(right.target));
            }
            let (kind, code): (u64, u64) = match child.outcome.unwrap() {
                Outcome::Exit(n) => (0, n),
                Outcome::Fault(_) => (1, 0),
            };
            let mut bytes = [0; WAIT_SIZE];
            bytes[..4].copy_from_slice(&VERSION.to_le_bytes());
            bytes[4..8].copy_from_slice(&(WAIT_SIZE as u32).to_le_bytes());
            bytes[8..16].copy_from_slice(&right.target.number.to_le_bytes());
            bytes[16..24].copy_from_slice(&kind.to_le_bytes());
            bytes[24..].copy_from_slice(&code.to_le_bytes());
            if !heap::with_pages(|m| Ok(space.copy_to_user(m, out, &bytes))).unwrap() {
                return Err(BAD_ADDRESS);
            }
            if p.terminal & crate::terminal::QUALIFY != 0 {
                hal::log(format_args!(
                    "CLI collecte pid={} cycles={}",
                    right.target.number,
                    hal::timer::counter() - child.exit_counter
                ));
            }
            s.processes.get_mut(pid).wait_right = None;
            s.processes.release_observer(right.target);
            Ok(None)
        })?;
        match target {
            None => return Ok(WAIT_SIZE as u64),
            Some(pid) => dispatch(Reason::Block(Wait::Process(pid))),
        }
    }
}

/// Les deux portes appellent cette fonction sur la pile privée, IRQ masquées.
/// Le contexte entier est sauvegardé avant de permettre une préemption.
pub(crate) fn call(id: u64, args: [u64; 3], pc: u64, sp: u64, level: &'static str) -> Reply {
    let (exec, stack) = with(|s| {
        let space = s.processes.get(caller(s)).space.as_ref().unwrap();
        (
            space.executable(pc),
            (space.stack_bottom()..=space.stack_top()).contains(&sp),
        )
    });
    if !exec || !stack || (cfg!(target_arch = "aarch64") && (sp % 16 != 0 || pc % 4 != 0)) {
        finish(
            Outcome::Fault(Fault {
                trap: "ABI",
                access: "retour",
                cause: "contexte",
                address: if !exec { pc } else { sp },
                syndrome: 0,
            }),
            pc,
            level,
        );
    }
    with(|s| s.thread_mut(s.current).in_syscall = true);
    context::enable();
    let [a, b, c] = args;
    let result = match id {
        IPC_PUT | IPC_TAKE => ipc::transfer(id, a, b, c),
        IPC_CREATE..=IPC_BOOTSTRAP => ipc::call(id, args),
        crate::hardware::block::wire::PAGE_PORTAL => device_page(a, b, c),
        crate::hardware::block::PORTAL => device(a, b, c),
        HEAP_RESIZE if c == 0 => with(|s| {
            let pid = caller(s);
            let space = s.processes.get_mut(pid).space.as_mut().unwrap();
            heap::with_pages(|m| space.resize_heap(m, a, b)).map_err(|e| {
                if e == crate::memory::Error::Exhausted {
                    NO_MEMORY
                } else {
                    BAD_ARGUMENT
                }
            })
        }),
        HEAP_RESIZE => Err(BAD_ARGUMENT),
        CLOCK_UTC if args == [0; 3] => crate::arch::rtc::seconds().ok_or(REFUSED),
        CLOCK_UTC => Err(BAD_ARGUMENT),
        IPC_FEATURES => ipc::features(a, b, c),
        PROCESS_ID => Ok(with(|s| caller(s).number)),
        THREAD_ID => Ok(with(|s| s.thread(s.current).id.generation)),
        THREAD_YIELD => {
            yield_now();
            Ok(0)
        }
        PROCESS_EXIT => {
            hal::interrupts::mask();
            finish(Outcome::Exit(a), pc, level);
        }
        crate::terminal::PROCESS_SAMPLE => sample_process(a, b, c),
        crate::terminal::SPAWN => ipc::spawn_program(a, b, c),
        crate::terminal::CONSOLE => console_io(a, b, c),
        PROCESS_WAIT => wait(a, b, c),
        DEBUG_WRITE => debug_write(a, b),
        _ if id >> 16 != u64::from(VERSION) => Err(BAD_VERSION),
        _ => Err(UNKNOWN_CALL),
    };
    hal::interrupts::mask();
    with(|s| s.thread_mut(s.current).in_syscall = false);
    match result {
        Ok(value) => Reply { status: OK, value },
        Err(status) => Reply { status, value: 0 },
    }
}

// Port privé du pilote élu. Les seize appels ABI v1 et leurs sélecteurs restent inchangés.
#[inline(never)]
fn device_page(key: u64, desc: u64, addr: u64) -> Result<u64, u64> {
    use crate::hardware::block;
    let pid = with(|s| caller(s));
    if !block::admit(pid.number, key) {
        return Err(DENIED);
    }
    if desc % 8 != 0 {
        return Err(BAD_ARGUMENT);
    }
    let mut w = [0; 128];
    let mut page = [0; 4096];
    let valid = with(|s| {
        let space = s.processes.get(pid).space.as_ref().unwrap();
        heap::with_pages(|m| {
            Ok(space.copy_from_user(m, desc, &mut w)
                && if w[1] == block::wire::WRITE {
                    space.copy_from_user(m, addr, &mut page)
                } else {
                    space.writable(m, addr, page.len())
                })
        })
        .unwrap()
    });
    if !valid {
        return Err(BAD_ADDRESS);
    }
    block::page_command(pid.number, key, &w, &mut page)?;
    copier_reponse_disque(
        pid,
        key,
        addr,
        if w[1] == block::wire::READ {
            &page
        } else {
            &[]
        },
    )?;
    Ok(4096)
}

fn device(key: u64, addr: u64, size: u64) -> Result<u64, u64> {
    use crate::hardware::block;
    let pid = with(|s| caller(s));
    if !block::admit(pid.number, key) {
        return Err(DENIED);
    }
    if size != 128 || addr % 8 != 0 {
        return Err(BAD_ARGUMENT);
    }
    let mut wire = [0u8; 128];
    let valid = with(|s| {
        let space = s.processes.get(pid).space.as_ref().unwrap();
        heap::with_pages(|m| {
            Ok(space.writable(m, addr, 128) && space.copy_from_user(m, addr, &mut wire))
        })
        .unwrap()
    });
    if !valid {
        return Err(BAD_ADDRESS);
    }
    block::command(pid.number, key, &mut wire)?;
    copier_reponse_disque(pid, key, addr, &wire)?;
    Ok(128)
}

// Sur le BSP, aucun décès ou retrait ne peut séparer cette admission de la copie.
fn copier_reponse_disque(pid: Pid, key: u64, addr: u64, octets: &[u8]) -> Result<(), u64> {
    with(|s| {
        if !crate::hardware::block::admit(pid.number, key) {
            return Err(REVOKED);
        }
        if octets.is_empty() {
            return Ok(());
        }
        let espace = s.processes.get(pid).space.as_ref().ok_or(REVOKED)?;
        if heap::with_pages(|m| Ok(espace.copy_to_user(m, addr, octets))).unwrap() {
            Ok(())
        } else {
            Err(BAD_ADDRESS)
        }
    })
}

// Un seul terminal série. Les deux écrans possèdent chacun leur position.
static LINE_OPEN: AtomicBool = AtomicBool::new(false);
static SAVED_LINE_OPEN: AtomicBool = AtomicBool::new(false);
static ALTERNATE: AtomicBool = AtomicBool::new(false);

/// Chaque contrôle est construit ici. Aucune chaîne de contrôle ne provient d’un client.
fn present(action: u64, argument: u64) -> Result<u64, u64> {
    use crate::arch::serial;
    use crate::terminal::*;
    if action == READ_TIMED && (1..=500).contains(&argument) {
        let start = crate::sched::ticks();
        loop {
            if let Some(b) = serial::input() {
                return Ok(b.into());
            }
            if crate::sched::ticks().wrapping_sub(start) >= argument {
                return Err(TIMED_OUT);
            }
            sleep(1)
        }
    }
    let _guard = Guard::new();
    if action == POSITION {
        let row = argument >> 16;
        let col = argument & 0xffff;
        if !(1..=u64::from(MAX_AXIS)).contains(&row) || !(1..=u64::from(MAX_AXIS)).contains(&col) {
            return Err(BAD_ARGUMENT);
        }
        // Trois chiffres au plus par axe. CUP accepte les zéros initiaux.
        let mut seq = *b"\x1b[000;000H";
        decimal(&mut seq[2..5], row);
        decimal(&mut seq[6..9], col);
        serial::output(core::str::from_utf8(&seq).unwrap()).map_err(|_| REFUSED)?;
        return Ok(0);
    }
    if argument != 0 {
        return Err(BAD_ARGUMENT);
    }
    let sequence = match action {
        FRESH_LINE if !LINE_OPEN.load(Ordering::Relaxed) => return Ok(0),
        FRESH_LINE => "\r\n",
        CLEAR_SCREEN => "\x1b[2J\x1b[H",
        ENTER_SCREEN => "\x1b[?1049h",
        LEAVE_SCREEN => "\x1b[?1049l",
        ERASE_EOL => "\x1b[K",
        QUERY_SIZE => "\x1b7\x1b[999;999H\x1b[6n\x1b8",
        _ => return Err(BAD_ARGUMENT),
    };
    serial::output(sequence).map_err(|_| REFUSED)?;
    match action {
        ENTER_SCREEN if !ALTERNATE.swap(true, Ordering::Relaxed) => {
            SAVED_LINE_OPEN.store(LINE_OPEN.swap(false, Ordering::Relaxed), Ordering::Relaxed);
        }
        LEAVE_SCREEN if ALTERNATE.swap(false, Ordering::Relaxed) => {
            LINE_OPEN.store(SAVED_LINE_OPEN.load(Ordering::Relaxed), Ordering::Relaxed);
        }
        FRESH_LINE | CLEAR_SCREEN => LINE_OPEN.store(false, Ordering::Relaxed),
        _ => (),
    }
    Ok(0)
}
/// Les indices sont relatifs au lanceur. Aucun PID numérique ne désigne une cible.
fn sample_process(index: u64, address: u64, len: u64) -> Result<u64, u64> {
    use crate::terminal::{OBSERVE_SESSION, SAMPLE_BYTES};
    with(|s| {
        let owner = s.processes.get(caller(s));
        if owner.terminal & OBSERVE_SESSION == 0 {
            return Err(DENIED);
        }
        let space = owner.space.as_ref().unwrap();
        if len != SAMPLE_BYTES as u64 {
            return Err(BAD_ARGUMENT);
        }
        let target = match index {
            0 => owner,
            1 => match owner.wait_right {
                Some(right) => s.processes.get(right.target),
                None => return Ok(0),
            },
            _ => return Err(BAD_ARGUMENT),
        };
        let pages = target.space.as_ref().map_or(0, |space| {
            let (data, stack) = space.page_counts();
            (data + stack) as u64
        });
        let mut buf = [0; SAMPLE_BYTES];
        for (dst, value) in buf.chunks_exact_mut(8).zip([
            target.pid.number,
            u64::from(target.space.is_some()),
            pages,
        ]) {
            dst.copy_from_slice(&value.to_le_bytes());
        }
        if !heap::with_pages(|m| Ok(space.copy_to_user(m, address, &buf))).unwrap() {
            return Err(BAD_ADDRESS);
        }
        Ok(1)
    })
}
fn decimal(digits: &mut [u8], mut value: u64) {
    for digit in digits.iter_mut().rev() {
        *digit = b'0' + (value % 10) as u8;
        value /= 10;
    }
}

fn console_io(port: u64, address: u64, len: u64) -> Result<u64, u64> {
    use crate::terminal::*;
    let right = u8::try_from(port).map_err(|_| BAD_ARGUMENT)?;
    if right == SCREEN {
        let owner = INPUT | OUTPUT | EDIT;
        if with(|s| s.processes.get(caller(s)).terminal & owner != owner) {
            return Err(DENIED);
        }
        return present(address, len);
    }
    let color = right == DIRECTORY_COLOR;
    let right = if color { OUTPUT } else { right };
    if !matches!(right, INPUT | OUTPUT | ERROR | EDIT) {
        return Err(BAD_ARGUMENT);
    }
    let pid = with(|s| caller(s));
    if with(|s| s.processes.get(pid).terminal & right == 0) {
        return Err(DENIED);
    }
    if right == INPUT {
        if address != 0 || len != 0 {
            return Err(BAD_ARGUMENT);
        }
        loop {
            if let Some(b) = crate::arch::serial::input() {
                return Ok(b.into());
            }
            sleep(1)
        }
    }
    // EDIT expose seulement les gestes du terminal nécessaires à l'éditeur de ligne.
    if right == EDIT {
        if len != 0 {
            return Err(BAD_ARGUMENT);
        }
        if address == 2 && with(|s| s.processes.get(pid).terminal & QUALIFY != 0) {
            let pages = with(|s| s.processes.get(pid).space.as_ref().unwrap().page_counts());
            let audit = heap::audit(true);
            measure("cli_repos");
            hal::log(format_args!(
                "CLI repos pages={} libres={} buddy={}",
                pages.0 + pages.1,
                audit.free_pages,
                audit.occupied
            ));
            return Ok(0);
        }
        let text = match address {
            0 => "\r",
            1 => "\x08",
            _ => return Err(BAD_ARGUMENT),
        };
        crate::arch::serial::output(text).map_err(|_| REFUSED)?;
        return Ok(0);
    }
    let mut buffer = [0; DEBUG_MAX];
    let n = usize::try_from(len).map_err(|_| LIMIT)?;
    let bytes = buffer
        .get_mut(..n)
        .filter(|b| !b.is_empty())
        .ok_or(BAD_ARGUMENT)?;
    if !with(|s| {
        heap::with_pages(|m| {
            Ok(s.processes
                .get(pid)
                .space
                .as_ref()
                .unwrap()
                .copy_from_user(m, address, bytes))
        })
        .unwrap()
    }) {
        return Err(BAD_ADDRESS);
    }
    let text = core::str::from_utf8(bytes).map_err(|_| BAD_ARGUMENT)?;
    // Les programmes peuvent imprimer du texte et changer de ligne. Seul EDIT contrôle le curseur.
    if text.chars().any(|c| c != '\n' && !printable(c)) {
        return Err(BAD_ARGUMENT);
    }
    let _guard = Guard::new();
    if color {
        crate::arch::serial::output("\x1b[34m").map_err(|_| REFUSED)?;
    }
    for piece in text.split_inclusive('\n') {
        if let Some(line) = piece.strip_suffix('\n') {
            crate::arch::serial::output(line)
                .and_then(|_| crate::arch::serial::output("\r\n"))
                .map_err(|_| REFUSED)?;
        } else {
            crate::arch::serial::output(piece).map_err(|_| REFUSED)?;
        }
    }
    if color {
        crate::arch::serial::output("\x1b[0m").map_err(|_| REFUSED)?;
    }
    LINE_OPEN.store(!text.ends_with('\n'), Ordering::Relaxed);
    Ok(len)
}
