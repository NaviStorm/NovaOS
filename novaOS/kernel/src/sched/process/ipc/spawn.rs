//! Le catalogue associe un numéro à un ELF immuable. Le service des fichiers
//! interprète seul la sélection d'objet. Un parent ne peut suivre qu'un fils à la fois.
use super::*;
use crate::terminal as tty;
const IMAGES: &[(&[u8], bool)] = include!(concat!(env!("OUT_DIR"), "/cli_catalog.rs"));

pub(in crate::sched::process) fn spawn_program(number: u64, addr: u64, len: u64) -> Res<u64> {
    let _guard = Guard::new();
    let parent = with(|s| s.thread(s.current).owner.unwrap());
    let authority = with(|s| s.processes.get(parent).terminal);
    if authority & tty::LAUNCH == 0 {
        return Err(DENIED);
    }
    if with(|s| s.processes.get(parent).wait_right.is_some()) {
        return Err(LIMIT);
    }
    let (image, fixture) = *IMAGES
        .get(usize::try_from(number).map_err(|_| BAD_ARGUMENT)?)
        .ok_or(BAD_ARGUMENT)?;
    let mut bytes = [0; tty::HEADER + tty::ARG_MAX];
    let n = usize::try_from(len).map_err(|_| BAD_ARGUMENT)?;
    let data = bytes
        .get_mut(..n)
        .filter(|b| b.len() >= tty::HEADER)
        .ok_or(BAD_ARGUMENT)?;
    let valid = with(|s| {
        heap::with_pages(|m| {
            let space = s.processes.get(parent).space.as_ref().unwrap();
            Ok(data.chunks_mut(DEBUG_MAX).enumerate().all(|(i, b)| {
                addr.checked_add((i * DEBUG_MAX) as u64)
                    .is_some_and(|p| space.copy_from_user(m, p, b))
            }))
        })
        .unwrap()
    });
    if !valid {
        return Err(BAD_ADDRESS);
    }
    let word = |i: usize| u64::from_le_bytes(data[i * 8..i * 8 + 8].try_into().unwrap());
    let (input, output, ports) = (word(0), word(1), word(2));
    if word(4) != (n - tty::HEADER) as u64
        || ports & !u64::from(authority & (tty::OUTPUT | tty::ERROR)) != 0
        || (input == 0) != (output == 0)
        || (input != 0 && input == output)
    {
        return Err(BAD_ARGUMENT);
    }
    if fixture && authority & tty::QUALIFY == 0 {
        return Err(DENIED);
    }
    data[..24].fill(0);
    let start = hal::timer::counter();
    let child = create_image(image, data, None, None).map_err(launch_error)?;
    let setup = with(|s| {
        let pid = child.0;
        if input != 0 {
            let tx = s.ipc.derive(parent, input, pid, SEND.into())?;
            let rx = s.ipc.derive(parent, output, pid, RECEIVE.into())?;
            s.ipc.locals.get_mut(pid.slot).bootstrap = tx;
            heap::with_pages(|m| {
                s.processes
                    .get(pid)
                    .space
                    .as_ref()
                    .unwrap()
                    .write_data(m, 0, rx);
                Ok(())
            })
            .unwrap();
        }
        s.processes.get_mut(pid).terminal = ports as u8 | (authority & tty::QUALIFY);
        let right = WaitRight { target: pid };
        s.processes.get_mut(parent).wait_right = Some(right);
        Ok(right.key())
    });
    match setup {
        Ok(key) => {
            let sample = authority & tty::QUALIFY != 0;
            if sample {
                let pages = with(|s| {
                    s.processes
                        .get(child.0)
                        .space
                        .as_ref()
                        .unwrap()
                        .page_counts()
                });
                hal::log(format_args!(
                    "CLI spawn image={number} pid={} cycles={} pages={}",
                    child.0.number,
                    hal::timer::counter() - start,
                    pages.0 + pages.1
                ));
                measure("cli_enfant");
            }
            core::mem::forget(child);
            Ok(key)
        }
        Err(e) => {
            retire_unpublished(child);
            Err(e)
        }
    }
}
