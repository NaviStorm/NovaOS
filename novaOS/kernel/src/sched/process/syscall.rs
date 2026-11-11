//! Dispatch commun des appels natifs. Les adresses de l’appelant restent des entiers.
use super::*;
use crate::abi::*;

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
        IPC_CREATE..=IPC_BOOTSTRAP => ipc::call(id, args),
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
    if !with(|s| {
        heap::with_pages(|m| {
            Ok(s.processes
                .get(pid)
                .space
                .as_ref()
                .unwrap()
                .copy_to_user(m, addr, &wire))
        })
        .unwrap()
    }) {
        return Err(BAD_ADDRESS);
    }
    Ok(128)
}

fn console_io(port: u64, address: u64, len: u64) -> Result<u64, u64> {
    use crate::terminal::*;
    let right = u8::try_from(port).map_err(|_| BAD_ARGUMENT)?;
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
    for piece in text.split_inclusive('\n') {
        if let Some(line) = piece.strip_suffix('\n') {
            crate::arch::serial::output(line)
                .and_then(|_| crate::arch::serial::output("\r\n"))
                .map_err(|_| REFUSED)?;
        } else {
            crate::arch::serial::output(piece).map_err(|_| REFUSED)?;
        }
    }
    Ok(len)
}
