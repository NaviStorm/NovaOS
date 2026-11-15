//! Le PID possède les droits de collecte. Le TCB possède le contexte vivant.
//! Toutes les transitions de l’un à l’autre se font sous la garde du scheduler.
use super::*;
use crate::abi::*;
use crate::memory::user::{MAX_SECONDARY_THREADS, MAX_THREAD_STACK_PAGES, ThreadMemory};

pub(in crate::sched) struct Member {
    pub(in crate::sched) layout: ThreadMemory,
    pub(in crate::sched) secondary: bool,
    pub(super) entry: u64,
    pub(super) argument: u64,
    #[cfg(feature = "thread-probe")]
    pub(in crate::sched) preemptions: u64,
    #[cfg(feature = "thread-probe")]
    pub(in crate::sched) preempted_on: u64,
    pub(in crate::sched) wait_status: u64,
    cancelled: bool,
    exit_value: u64,
}
impl Member {
    pub(super) fn new(layout: ThreadMemory, entry: u64, argument: u64, secondary: bool) -> Self {
        Self {
            layout,
            entry,
            argument,
            secondary,
            #[cfg(feature = "thread-probe")]
            preemptions: 0,
            #[cfg(feature = "thread-probe")]
            preempted_on: 0,
            wait_status: OK,
            cancelled: false,
            exit_value: 0,
        }
    }
}

pub(super) struct JoinRight {
    target: Id,
    waiter: Option<Id>,
    result: Option<u64>,
}

fn owner(s: &Core) -> Pid {
    s.thread(s.current()).owner.unwrap()
}

pub(super) fn status(s: &Core) -> Result<(), u64> {
    if s.processes.get(owner(s)).outcome.is_some() {
        Err(CANCELLED)
    } else {
        Ok(())
    }
}

// Frontière sans emprunt ni ressource noyau détenue. Une pile en cours d’usage
// sur un autre CPU ne peut être retirée avant sa propre commutation.
pub(in crate::sched) fn stop_if_dying() {
    if with(|s| status(s).is_err()) {
        dispatch(Reason::Exit);
        unreachable!("reprise après la mort du processus")
    }
}

pub(super) fn wake_dying(s: &mut Core, pid: Pid) {
    for slot in 0..s.slots.len() {
        if s.slots[slot].as_ref().is_some_and(|t| {
            t.owner == Some(pid)
                && matches!(
                    t.state,
                    State::Blocked(
                        Wait::Ipc(_) | Wait::Join(_) | Wait::UserWord { .. } | Wait::Process(_) | Wait::Policy(_)
                    )
                )
        }) {
            s.wake(slot);
        }
    }
}

fn create_member(entry: u64, argument: u64, pages: u64) -> Result<u64, u64> {
    let pages = usize::try_from(pages).map_err(|_| BAD_ARGUMENT)?;
    if !(1..=MAX_THREAD_STACK_PAGES).contains(&pages) {
        return Err(BAD_ARGUMENT);
    }
    let _gate = Guard::new();
    reap();
    let (pid, root, mask, slot, layout) = with(|s| {
        status(s)?;
        let t = s.thread(s.current());
        let (pid, root, mask) = (t.owner.unwrap(), t.space, t.cpu_mask);
        let live = s
            .slots
            .iter()
            .flatten()
            .filter(|t| t.owner == Some(pid) && t.user.as_ref().is_some_and(|m| m.secondary))
            .count();
        let p = s.processes.get_mut(pid);
        let space = p.space.as_mut().unwrap();
        if !space.executable(entry) || (cfg!(target_arch = "aarch64") && entry % 4 != 0) {
            return Err(BAD_ADDRESS);
        }
        if live >= MAX_SECONDARY_THREADS || p.join_rights.len() >= MAX_SECONDARY_THREADS {
            return Err(LIMIT);
        }
        let slot = s.slots.iter().position(Option::is_none).ok_or(LIMIT)?;
        let layout =
            heap::with_pages(|m| space.allocate_thread(m, pages)).map_err(|e| match e {
                crate::memory::Error::Exhausted => NO_MEMORY,
                crate::memory::Error::Occupied => LIMIT,
                _ => BAD_ARGUMENT,
            })?;
        if p.join_rights.try_reserve(1).is_err() {
            release_layout(s, pid, layout);
            return Err(NO_MEMORY);
        }
        Ok((pid, root, mask, slot, layout))
    })?;
    let id = match create(slot, enter, Priority::Normal, mask) {
        Ok(id) => id,
        Err(_) => {
            with(|s| {
                release_layout(s, pid, layout);
                let p = s.processes.get_mut(pid);
                if p.join_rights.is_empty() {
                    p.join_rights = Vec::new();
                }
            });
            return Err(NO_MEMORY);
        }
    };
    with(|s| {
        let p = s.processes.get_mut(pid);
        p.join_rights.push(JoinRight {
            target: id,
            waiter: None,
            result: None,
        });
        p.threads += 1;
        let t = s.thread_mut(slot);
        t.space = root;
        t.owner = Some(pid);
        t.user = Some(Member::new(layout, entry, argument, true));
    });
    Ok(id.generation)
}

fn release_layout(s: &mut Core, pid: Pid, layout: ThreadMemory) {
    let space = s.processes.get_mut(pid).space.as_mut().unwrap();
    heap::with_pages(|m| {
        space.release_thread(m, layout);
        Ok(())
    })
    .unwrap();
}

// Appelé sur une pile tierce, après le retrait du TCB et de sa pile noyau.
pub(in crate::sched) fn collected(pid: Pid, id: Id, member: Member) {
    with(|s| {
        if member.secondary {
            release_layout(s, pid, member.layout);
        } else {
            let p = s.processes.get_mut(pid);
            if p.threads == 1 {
                // detach() restitue immédiatement la racine complète.
                return;
            }
            heap::with_pages(|m| {
                p.space.as_mut().unwrap().release_main_thread(m);
                Ok(())
            })
            .unwrap();
        }
        invalidate_range(s, pid, member.layout.bottom, member.layout.top);
        if member.layout.tls != 0 {
            invalidate_range(
                s,
                pid,
                member.layout.tls,
                member.layout.tls + crate::memory::PAGE,
            );
        }
        if let Some(right) = s
            .processes
            .get_mut(pid)
            .join_rights
            .iter_mut()
            .find(|r| r.target == id)
        {
            right.result = Some(member.exit_value);
            s.signal(Wait::Join(id));
        }
    });
}

pub(super) fn invalidate_range(s: &mut Core, pid: Pid, start: u64, end: u64) {
    let p = s.processes.get_mut(pid);
    let invalidate_process = matches!(p.wait_buffer, WaitBuffer::Pending(address)
        if address < end && start < address.saturating_add(WAIT_SIZE as u64));
    if invalidate_process {
        p.wait_buffer = WaitBuffer::Invalidated;
        if let Some(slot) = s
            .slots
            .iter()
            .flatten()
            .find(|t| t.owner == Some(pid) && matches!(t.state, State::Blocked(Wait::Process(_))))
            .map(|t| t.id.slot)
        {
            s.wake(slot);
        }
    }
    s.ipc.invalidate_receive(pid, start, end);
    s.flush_ipc();
    for slot in 0..s.slots.len() {
        if s.slots[slot].as_ref().is_some_and(|t| {
            t.owner == Some(pid)
                && matches!(t.state,
                State::Blocked(Wait::UserWord { address, .. }) if (start..end).contains(&address))
        }) {
            s.thread_mut(slot).user.as_mut().unwrap().wait_status = BAD_ADDRESS;
            s.wake(slot);
        }
    }
}

fn exit(value: u64, pc: u64, level: &'static str) -> ! {
    let _gate = Guard::new();
    stop_if_dying();
    let last = with(|s| {
        let slot = s.current();
        let pid = owner(s);
        s.thread_mut(slot).user.as_mut().unwrap().exit_value = value;
        !s.slots
            .iter()
            .flatten()
            .any(|t| t.id.slot != slot && t.owner == Some(pid) && t.state != State::Terminated)
    });
    if last {
        finish(Outcome::Exit(value), pc, level);
    }
    dispatch(Reason::Exit);
    unreachable!("reprise après THREAD_EXIT")
}

fn consume(p: &mut Process, index: usize) {
    p.join_rights.swap_remove(index);
    if p.join_rights.is_empty() {
        p.join_rights = Vec::new();
    }
}

fn right_index(p: &Process, key: u64) -> Result<usize, u64> {
    p.join_rights
        .iter()
        .position(|r| r.target.generation == key)
        .ok_or(DENIED)
}

fn join(key: u64) -> Result<u64, u64> {
    use core::ops::ControlFlow::{Break, Continue};
    loop {
        let _gate = Guard::new();
        let target = with(|s| {
            status(s)?;
            let current = s.thread(s.current());
            let id = current.id;
            let cancelled = current.user.as_ref().unwrap().cancelled;
            let p = s.processes.get_mut(current.owner.unwrap());
            let index = right_index(p, key)?;
            let right = &mut p.join_rights[index];
            if right.target == id {
                return Err(BAD_ARGUMENT);
            }
            if right.waiter.is_some_and(|other| other != id) {
                return Err(REFUSED);
            }
            if cancelled {
                right.waiter = None;
                return Err(CANCELLED);
            }
            if let Some(value) = right.result {
                consume(p, index);
                return Ok(Break(value));
            }
            right.waiter = Some(id);
            Ok(Continue(right.target))
        })?;
        match target {
            Break(value) => return Ok(value),
            Continue(id) => dispatch(Reason::Block(Wait::Join(id))),
        }
    }
}

fn detach_member(key: u64) -> Result<u64, u64> {
    with(|s| {
        let p = s.processes.get_mut(owner(s));
        let index = right_index(p, key)?;
        if p.join_rights[index].waiter.is_some() {
            return Err(REFUSED);
        }
        consume(p, index);
        Ok(0)
    })
}

fn cancel(key: u64) -> Result<u64, u64> {
    with(|s| {
        let pid = owner(s);
        let p = s.processes.get(pid);
        let target = p.join_rights[right_index(p, key)?].target;
        let t = s.slots[target.slot]
            .as_mut()
            .filter(|t| t.owner == Some(pid) && t.id == target && t.state != State::Terminated)
            .ok_or(CLOSED)?;
        t.user.as_mut().unwrap().cancelled = true;
        if matches!(
            t.state,
            State::Blocked(Wait::UserWord { .. } | Wait::Join(_))
        ) {
            t.user.as_mut().unwrap().wait_status = CANCELLED;
            s.wake(target.slot);
        }
        Ok(0)
    })
}

fn word(s: &Core, address: u64) -> Result<u64, u64> {
    if address % 8 != 0 {
        return Err(BAD_ARGUMENT);
    }
    let space = s.processes.get(owner(s)).space.as_ref().unwrap();
    heap::with_pages(|m| Ok(space.atomic_word(m, address)))
        .unwrap()
        .ok_or(BAD_ADDRESS)
}

fn wait_word(address: u64, expected: u64, timeout: u64) -> Result<u64, u64> {
    let _gate = Guard::new();
    let deadline = with(|s| {
        status(s)?;
        if s.thread(s.current()).user.as_ref().unwrap().cancelled {
            return Err(CANCELLED);
        }
        if word(s, address)? != expected {
            return Err(WOULD_BLOCK);
        }
        if timeout == 0 {
            return Err(TIMED_OUT);
        }
        s.thread_mut(s.current()).user.as_mut().unwrap().wait_status = OK;
        if timeout == u64::MAX {
            Ok(u64::MAX)
        } else {
            ticks()
                .checked_add(timeout)
                .filter(|&n| n != u64::MAX)
                .ok_or(LIMIT)
        }
    })?;
    dispatch(Reason::Block(Wait::UserWord { address, deadline }));
    with(|s| {
        status(s)?;
        let member = s.thread(s.current()).user.as_ref().unwrap();
        match member.wait_status {
            OK => Ok(0),
            error => Err(error),
        }
    })
}

fn notify(address: u64, count: u64) -> Result<u64, u64> {
    if count == 0 {
        return Err(BAD_ARGUMENT);
    }
    with(|s| {
        word(s, address)?;
        let pid = owner(s);
        let mut woke = 0;
        while woke < count {
            let next = s
                .slots
                .iter()
                .flatten()
                .filter(|t| {
                    t.owner == Some(pid)
                        && matches!(t.state,
                    State::Blocked(Wait::UserWord { address: waited, .. }) if waited == address)
                })
                .min_by_key(|t| t.wait_order)
                .map(|t| t.id.slot);
            let Some(slot) = next else {
                break;
            };
            s.wake(slot);
            woke += 1;
        }
        Ok(woke)
    })
}

fn tls() -> Result<u64, u64> {
    with(|s| {
        let slot = s.current();
        let member = s.thread(slot).user.as_ref().unwrap();
        if member.layout.tls != 0 {
            return Ok(member.layout.tls);
        }
        let space = s.processes.get_mut(owner(s)).space.as_mut().unwrap();
        let address = heap::with_pages(|m| space.main_tls(m)).map_err(|_| NO_MEMORY)?;
        s.thread_mut(slot).user.as_mut().unwrap().layout.tls = address;
        Ok(address)
    })
}

pub(super) fn call(id: u64, [a, b, c]: [u64; 3], pc: u64, level: &'static str) -> Result<u64, u64> {
    // Aucun argument utilisateur ne peut conférer cette admission. Les services
    // à titulaire unique gardent aussi leur contrat de réveil et de supervision.
    if !with(|s| s.processes.get(owner(s)).application_threads) {
        return Err(DENIED);
    }
    match id {
        THREAD_CREATE => create_member(a, b, c),
        THREAD_EXIT if b == 0 && c == 0 => exit(a, pc, level),
        THREAD_JOIN if b == 0 && c == 0 => join(a),
        THREAD_DETACH if b == 0 && c == 0 => detach_member(a),
        THREAD_CANCEL if b == 0 && c == 0 => cancel(a),
        THREAD_CANCELLED if [a, b, c] == [0; 3] => Ok(with(|s| {
            s.thread(s.current()).user.as_ref().unwrap().cancelled as u64
        })),
        THREAD_TLS if [a, b, c] == [0; 3] => tls(),
        THREAD_WAIT => wait_word(a, b, c),
        THREAD_NOTIFY if c == 0 => notify(a, b),
        _ => Err(BAD_ARGUMENT),
    }
}
